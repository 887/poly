//! Scan for bare `navigator().push(Route::...)` callsites — plan-connected-routes-static-check.md §5.3.3.
//!
//! Every navigation to a known `Route` variant must go through the `nav!`
//! macro (defined in `crates/core/src/lib.rs`) so that future graph-analysis
//! tooling only has to recognise one syntactic form. The `nav!` macro itself
//! expands to `::dioxus::prelude::navigator().push(route)` so this scan
//! looks for the bare pattern `navigator().push(Route::` *outside* the
//! macro definition site.
//!
//! Scope: any `*.rs` file in the workspace except tests/examples/MCP/
//! the lint-gate crate itself, and `crates/core/src/lib.rs` (the file
//! that defines `nav!` and legitimately references the expanded form in
//! its documentation).

use std::io::{BufRead, BufReader};

use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    for path in &walker.files {
        let s = path.to_string_lossy();
        if s.contains("/tests/")
            || s.contains("/examples/")
            || s.contains("/servers/test-")
            || s.contains("/plugin-host-tests/")
            || s.contains("/mcp/")
        {
            continue;
        }
        // The macro definition site (`crates/core/src/lib.rs`) must reference
        // the expanded form in documentation; skip it.
        if s.ends_with("crates/core/src/lib.rs") {
            continue;
        }
        let Ok(file) = std::fs::File::open(path) else {
            continue;
        };
        for (idx, raw) in BufReader::new(file).lines().map_while(Result::ok).enumerate() {
            if !raw.contains("navigator().push(Route::") {
                continue;
            }
            // Allow `crate::nav!(...)` expansions — but the substring match
            // above requires the literal `navigator().push(Route::` chars, so
            // any `nav!(Route::X)` callsite wouldn't match it anyway. The
            // only way to hit this branch is the bare form.
            violations.push(Violation {
                rule: "nav_push_ban".to_string(),
                path: walker.relative(path),
                line: (idx as u32) + 1,
                detail: "bare `navigator().push(Route::...)` — use `crate::nav!(Route::...)`"
                    .to_string(),
            });
        }
    }
}
