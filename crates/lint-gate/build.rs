//! Workspace-wide build-time lint gate.
//!
//! Runs on every `cargo check` / `cargo clippy` / `cargo build`. Scans the
//! workspace for violations of three rules and emits `cargo::error=` for each.
//! Existing violations are grandfathered via `baseline.json`.
//!
//! Rules:
//!  1. Banned `#[allow(...)]` attributes (plan-component-lints.md §3.2).
//!  2. Context-menu decorator coverage (plan-context-menu-quality-control.md §3.1.2).
//!  3. Connected-routes graph reachability (plan-connected-routes-static-check.md §3).
//!  4. UI action coverage (plan-ui-completeness.md §B).
//!  5. UI action-enum coverage (typed-ui-action-enums plan Phase C).
//!  6. FTL label-key coverage (plan-client-ui-surface.md D21).
//!  7. Action ID naming convention — kebab-case (plan-client-ui-surface.md D25).
//!  8. Backend-slug `match` ladders in UI (plan-client-ui-surface.md §7 WP 7).
//!
//! Existing violations are grandfathered via `baseline.json`.

#[path = "build/action_enum_coverage.rs"]
mod action_enum_coverage;
#[path = "build/action_id_naming.rs"]
mod action_id_naming;
#[path = "build/allow_ban.rs"]
mod allow_ban;
#[path = "build/baseline.rs"]
mod baseline;
#[path = "build/context_menu_coverage.rs"]
mod context_menu_coverage;
#[path = "build/forbid_backend_slug_match.rs"]
mod forbid_backend_slug_match;
#[path = "build/ftl_label_key_coverage.rs"]
mod ftl_label_key_coverage;
#[path = "build/nav_push_ban.rs"]
mod nav_push_ban;
#[path = "build/route_graph.rs"]
mod route_graph;
#[path = "build/ui_action_coverage.rs"]
mod ui_action_coverage;
#[path = "build/walk.rs"]
mod walk;

use baseline::Baseline;

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

    let walker = walk::WorkspaceWalker::new(&ws_root);

    let mut violations: Vec<baseline::Violation> = Vec::new();
    allow_ban::scan(&walker, &mut violations);
    action_enum_coverage::scan(&walker, &mut violations);
    action_id_naming::scan(&walker, &mut violations);
    context_menu_coverage::scan(&walker, &mut violations);
    forbid_backend_slug_match::scan(&walker, &mut violations);
    ftl_label_key_coverage::scan(&walker, &mut violations);
    nav_push_ban::scan(&walker, &mut violations);
    route_graph::scan(&ws_root, &mut violations);
    ui_action_coverage::scan(&walker, &mut violations);

    if regen {
        for v in &violations {
            // §5.3.2: route-graph violations are never grandfathered — the whole
            // point of the graph scan is to catch new orphans immediately. Regen
            // only silences allow_ban / context_menu_coverage / nav_push_ban /
            // ui_action_coverage.
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

    let mut new_count = 0u32;
    let mut grandfathered = 0u32;
    for v in &violations {
        if baseline.contains(v) {
            grandfathered += 1;
            continue;
        }
        println!("cargo::error={}", v.to_error_line());
        new_count += 1;
    }

    if grandfathered > 0 {
        println!(
            "cargo::warning=lint-gate: {grandfathered} grandfathered violations (run `cargo check --features regen-baseline` to refresh baseline)"
        );
    }
    if new_count > 0 {
        // cargo::error directives already failed the build; nothing more to do here.
    }
}

fn workspace_root() -> std::path::PathBuf {
    // `CARGO_MANIFEST_DIR` is `.../crates/lint-gate`; parent-of-parent is the workspace root.
    // Cargo always sets this env var for build scripts, and the parent/grandparent
    // directories always exist for a properly nested workspace crate.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let p = std::path::Path::new(&manifest).to_path_buf();
    p.parent()
        .and_then(|p| p.parent())
        .map_or_else(|| p.clone(), std::path::Path::to_path_buf)
}
