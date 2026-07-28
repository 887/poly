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
    let mut keys = ContentKeys::new(&ws_root);

    if regen {
        for v in &violations {
            if v.rule == "route_graph" {
                println!("cargo::error={}", v.to_error_line());
            } else {
                let key = keys.of(v);
                baseline.insert(v.clone(), key);
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
        let key = keys.of(v);
        if baseline.contains(v, key.as_ref()) {
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

use std::collections::{HashMap, HashSet};

/// `(fingerprint, ordinal)` of the source line a violation sits on.
///
/// See `poly_lint_gate_rules::allowlist` for the format. Baselines used to be
/// keyed on `(rule, path, line, detail)` alone, which desyncs on any reflow of
/// the file above the violation — the review pass hit exactly that (33 entries
/// desynced by an unrelated edit, flipping sites between "grandfathered" and
/// "new" at random). A content key names the offending *line*, not its address.
type ContentKey = (String, u32);

/// Lazily-computed per-file line keys, so a 773-row baseline reads each source
/// file at most once.
struct ContentKeys {
    ws_root: std::path::PathBuf,
    cache: HashMap<String, Option<Vec<ContentKey>>>,
}

impl ContentKeys {
    fn new(ws_root: &std::path::Path) -> Self {
        Self { ws_root: ws_root.to_path_buf(), cache: HashMap::new() }
    }

    /// The content key of `v`'s source line, or `None` when the file is
    /// unreadable or the line is out of range (coverage-style rules that report
    /// line 0, deleted files) — those keep the legacy line key.
    fn of(&mut self, v: &rules::Violation) -> Option<ContentKey> {
        // Disjoint field borrows: `cache` is borrowed mutably while `ws_root`
        // is only read inside the closure.
        let Self { ws_root, cache } = self;
        let entry = cache.entry(v.path.clone()).or_insert_with_key(|p| {
            std::fs::read_to_string(ws_root.join(p))
                .ok()
                .map(|c| rules::allowlist::line_keys(&c))
        });
        let idx = usize::try_from(v.line).ok()?.checked_sub(1)?;
        entry.as_ref()?.get(idx).cloned()
    }
}

/// One baseline row: the violation plus, when resolvable, its content key.
#[derive(Clone)]
struct Row {
    violation: rules::Violation,
    key: Option<ContentKey>,
}

struct Baseline {
    /// Legacy `(rule, path, line, detail)` keys — rows with no content key.
    line_keys: HashSet<(String, String, u32, String)>,
    /// Reflow-stable `(rule, path, fingerprint, ordinal, detail)` keys.
    content_keys: HashSet<(String, String, String, u32, String)>,
    rows: Vec<Row>,
}

fn line_key(v: &rules::Violation) -> (String, String, u32, String) {
    (v.rule.clone(), v.path.clone(), v.line, v.detail.clone())
}

fn content_key(v: &rules::Violation, k: &ContentKey) -> (String, String, String, u32, String) {
    (v.rule.clone(), v.path.clone(), k.0.clone(), k.1, v.detail.clone())
}

impl Baseline {
    fn empty() -> Self {
        Self {
            line_keys: HashSet::new(),
            content_keys: HashSet::new(),
            rows: Vec::new(),
        }
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
            let fp = item.get("fp").and_then(|v| v.as_str()).map(str::to_string);
            let ord = item
                .get("ord")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
            let key = match (fp, ord) {
                (Some(f), Some(o)) => Some((f, o)),
                _ => None,
            };
            b.insert(rules::Violation { rule, path: path_s, line, detail }, key);
        }
        b
    }

    fn insert(&mut self, v: rules::Violation, key: Option<ContentKey>) {
        // A row is keyed EITHER by content OR by line — never both. Keeping the
        // line key alongside a content key would re-open the desync hazard: a
        // brand-new violation that happened to land on the old line number,
        // with the same rule + detail, would be silently grandfathered.
        let fresh = match &key {
            Some(k) => self.content_keys.insert(content_key(&v, k)),
            None => self.line_keys.insert(line_key(&v)),
        };
        if fresh {
            self.rows.push(Row { violation: v, key });
        }
    }

    fn contains(&self, v: &rules::Violation, key: Option<&ContentKey>) -> bool {
        if let Some(k) = key
            && self.content_keys.contains(&content_key(v, k))
        {
            return true;
        }
        self.line_keys.contains(&line_key(v))
    }

    fn save(&self, path: &std::path::Path) {
        let mut sorted = self.rows.clone();
        sorted.sort_by(|a, b| {
            let (x, y) = (&a.violation, &b.violation);
            (&x.rule, &x.path, x.line, &x.detail).cmp(&(&y.rule, &y.path, y.line, &y.detail))
        });
        let items: Vec<serde_json::Value> = sorted
            .iter()
            .map(|row| {
                let mut obj = serde_json::json!({
                    "rule": row.violation.rule,
                    "path": row.violation.path,
                    "line": row.violation.line,
                    "detail": row.violation.detail,
                });
                if let Some((fp, ord)) = &row.key
                    && let Some(map) = obj.as_object_mut()
                {
                    drop(map.insert("fp".to_string(), serde_json::json!(fp)));
                    drop(map.insert("ord".to_string(), serde_json::json!(ord)));
                }
                obj
            })
            .collect();
        let obj = serde_json::json!({ "violations": items });
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
