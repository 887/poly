//! Workspace-wide build-time lint gate — thin driver.
//!
//! All scanner logic lives in `crates/lint-gate-rules`. This build script is
//! a ~30-line driver that locates the workspace root, builds a WorkspaceWalker,
//! loads/saves the grandfathering baseline, calls `lint_gate_rules::all_rules()`
//! to get violations, and emits `cargo::error=` for new violations.

use poly_lint_gate_rules as rules;

fn main() {
    let ws_root = workspace_root();
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed={}", ws_root.display());
    println!("cargo::rerun-if-env-changed=CARGO_FEATURE_REGEN_BASELINE");
    println!(
        "cargo::rerun-if-changed={}",
        ws_root
            .join("crates")
            .join("lint-gate")
            .join("build")
            .join("ui_action_baseline.toml")
            .display()
    );

    let regen = std::env::var("CARGO_FEATURE_REGEN_BASELINE").is_ok();
    let baseline_path = ws_root
        .join("crates")
        .join("lint-gate")
        .join("baseline.json");

    let mut baseline = if regen {
        Baseline::empty()
    } else {
        Baseline::load(&baseline_path)
    };

    let walker = rules::WorkspaceWalker::new(&ws_root);
    let violations = rules::all_rules(&walker, &ws_root);

    if regen {
        for v in &violations {
            if v.rule == "route_graph" {
                println!("cargo::error={}", v.to_error_line());
            } else {
                baseline.insert(v.clone());
            }
        }
        baseline.save(&baseline_path);
        println!(
            "cargo::warning=lint-gate: wrote {} entries to baseline.json (route_graph violations always fail)",
            violations.iter().filter(|v| v.rule != "route_graph").count()
        );
        return;
    }

    let mut new_count = 0_u32;
    let mut grandfathered = 0_u32;
    for v in &violations {
        if baseline.contains(v) {
            grandfathered = grandfathered.saturating_add(1);
            continue;
        }
        println!("cargo::error={}", v.to_error_line());
        new_count = new_count.saturating_add(1);
    }

    if grandfathered > 0 {
        println!(
            "cargo::warning=lint-gate: {grandfathered} grandfathered violations (run `cargo check --features regen-baseline` to refresh baseline)"
        );
    }
    let _ = new_count;
}

fn workspace_root() -> std::path::PathBuf {
    // `CARGO_MANIFEST_DIR` is `.../crates/lint-gate`; parent-of-parent is the workspace root.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let p = std::path::Path::new(&manifest).to_path_buf();
    p.parent()
        .and_then(|p| p.parent())
        .map_or_else(|| p.clone(), std::path::Path::to_path_buf)
}

// Baseline — inlined here because build scripts use build-dependencies, not lib targets.
// Uses serde_json from the build-deps to parse/write the JSON format.

use std::collections::HashSet;

struct Baseline {
    keys: HashSet<(String, String, u32, String)>,
    violations: Vec<rules::Violation>,
}

impl Baseline {
    fn empty() -> Self {
        Self { keys: HashSet::new(), violations: Vec::new() }
    }

    fn load(path: &std::path::Path) -> Self {
        let Ok(s) = std::fs::read_to_string(path) else { return Self::empty(); };
        let Ok(raw): Result<serde_json::Value, _> = serde_json::from_str(&s) else {
            println!("cargo::warning=lint-gate: baseline.json parse failed");
            return Self::empty();
        };
        let arr = match raw.get("violations").and_then(|v| v.as_array()) {
            Some(a) => a.clone(),
            None => return Self::empty(),
        };
        let mut b = Self::empty();
        for item in arr {
            let rule = item.get("rule").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let path_s = item.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let line = u32::try_from(
                item.get("line").and_then(serde_json::Value::as_u64).unwrap_or(0),
            )
            .unwrap_or(0);
            let detail = item.get("detail").and_then(|v| v.as_str()).unwrap_or("").to_string();
            b.insert(rules::Violation { rule, path: path_s, line, detail });
        }
        b
    }

    fn insert(&mut self, v: rules::Violation) {
        let key = (v.rule.clone(), v.path.clone(), v.line, v.detail.clone());
        if self.keys.insert(key) {
            self.violations.push(v);
        }
    }

    fn contains(&self, v: &rules::Violation) -> bool {
        let key = (v.rule.clone(), v.path.clone(), v.line, v.detail.clone());
        self.keys.contains(&key)
    }

    fn save(&self, path: &std::path::Path) {
        let mut sorted = self.violations.clone();
        sorted.sort_by(|a, b| {
            (&a.rule, &a.path, a.line, &a.detail).cmp(&(&b.rule, &b.path, b.line, &b.detail))
        });
        let obj = serde_json::json!({ "violations": sorted });
        let Ok(json) = serde_json::to_string_pretty(&obj) else {
            println!("cargo::warning=lint-gate: failed to serialize baseline");
            return;
        };
        if let Some(dir) = path.parent() { drop(std::fs::create_dir_all(dir)); }
        if let Err(e) = std::fs::write(path, json) {
            println!("cargo::warning=lint-gate: failed to write baseline: {e}");
        }
    }
}
