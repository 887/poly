//! Scan for banned `#[allow(...)]` attributes per plan-component-lints.md §3.2 / §5.

use std::io::{BufRead, BufReader};

use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

/// Lint names that should never appear inside a non-test `#[allow(...)]`.
const BANNED: &[&str] = &[
    "dead_code",
    "unused",
    "unused_imports",
    "unused_variables",
    "unused_mut",
    "unused_assignments",
    "unused_must_use",
    "warnings",
    "clippy::dead_code",
    "clippy::unwrap_used",
    "clippy::expect_used",
    "clippy::panic",
    "clippy::indexing_slicing",
    "clippy::needless_pass_by_value",
    "clippy::too_many_arguments",
    "clippy::too_many_lines",
];

/// A banned name is still OK when the attribute is one of the accepted
/// escape hatches listed in plan-component-lints.md §5.2:
///   * #[cfg_attr(test, allow(...))]
///   * an #[allow(...)] on a line that falls inside a #[cfg(test)] block
///   * a source line carrying a `// lint-allow-unused: <≥10-char reason>` marker
///
/// The first two are detected per-line; `// lint-allow-unused:` is detected on
/// the line above the `#[allow(...)]` attribute.
pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    for path in &walker.files {
        // tests/, examples/, test-harness crates (servers/test-*, *tests crate),
        // mcp/ and tools/ are skipped wholesale — they're fixtures, not prod code.
        let s = path.to_string_lossy();
        if s.contains("/tests/")
            || s.contains("/examples/")
            || s.contains("/servers/test-")
            || s.contains("/plugin-host-tests/")
            || s.contains("/mcp/")
        {
            continue;
        }
        // The snippet of Rust inside #[cfg(test)] { ... } blocks is also skipped;
        // we track it with a running brace counter from the cfg(test) line.
        let Ok(file) = std::fs::File::open(path) else {
            continue;
        };
        let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();

        let mut cfg_test_depth: i32 = 0;
        let mut pending_marker: Option<usize> = None; // line idx where a marker was seen

        for (idx, raw_line) in lines.iter().enumerate() {
            let line = raw_line.trim();
            // Track cfg(test) depth as a best-effort single-line match.
            // Tracks `#[cfg(test)]` preceding a `mod name {` or `fn name() {`
            // by bumping depth on matching braces after the attribute.
            if line.contains("#[cfg(test)]") || line.contains("#[cfg(all(test") {
                cfg_test_depth = cfg_test_depth.saturating_add(1);
            }
            // Crude brace accounting on the same or following lines.
            // This is intentionally loose — the goal is to avoid false
            // positives, not to be a full Rust parser.
            if cfg_test_depth > 0 && line.contains('}') && !line.contains('{') {
                cfg_test_depth = cfg_test_depth.saturating_sub(1);
            }
            if cfg_test_depth > 0 {
                continue;
            }

            // Remember an immediately-preceding opt-out marker.
            if let Some(reason) = marker_reason(line) {
                if reason.chars().count() >= 10 {
                    pending_marker = Some(idx);
                }
                continue;
            }

            let is_attr = line.starts_with("#[") || line.starts_with("#![");
            if !is_attr {
                // Preserve pending marker only when the next line is the attr.
                pending_marker = None;
                continue;
            }

            // Only interested in the single-keyword `allow(` form. Multi-lint
            // forms are handled by splitting the paren contents on commas.
            if !line.contains("allow(") {
                pending_marker = None;
                continue;
            }

            // cfg_attr(test, allow(...)) / cfg_attr(any(test, ...), allow(...))
            // pass through unconditionally.
            if line.contains("cfg_attr(") && line.contains("test") {
                pending_marker = None;
                continue;
            }

            // Extract the paren contents after `allow(`.
            let Some(start) = line.find("allow(") else {
                continue;
            };
            let rest = &line[start + "allow(".len()..];
            let Some(end) = rest.rfind(')') else {
                continue;
            };
            let inner = &rest[..end];

            // Test each comma-separated lint name against BANNED.
            let names: Vec<&str> = inner.split(',').map(str::trim).collect();
            for name in names {
                if BANNED.contains(&name) {
                    if pending_marker.is_some_and(|m| m + 1 == idx) {
                        // grandfathered by marker; skip
                        continue;
                    }
                    violations.push(Violation {
                        rule: "allow_ban".to_string(),
                        path: walker.relative(path),
                        line: (idx as u32) + 1,
                        detail: format!("banned #[allow({name})]"),
                    });
                }
            }
            pending_marker = None;
        }
    }
}

/// Returns the trimmed reason text when the line carries
/// `// lint-allow-unused: ...`.
fn marker_reason(line: &str) -> Option<&str> {
    line.split_once("// lint-allow-unused:")
        .map(|(_, r)| r.trim())
}
