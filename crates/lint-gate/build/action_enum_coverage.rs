#![allow(dead_code)]
//! UI action-enum coverage scanner — Phase C of typed UI action enums plan.
//!
//! Enforces two rules across all `.rs` files in `crates/core/src/ui/`
//! and `clients/*/src/`:
//!
//! - Rule A-enum (`action_enum_coverage`): Every `#[component]` must have
//!   `#[ui_action(...)]` somewhere in the attribute block directly above it.
//! - Rule B-enum (`action_enum_coverage_none_with_handler`): A component
//!   annotated `#[ui_action(None)]` must not contain a non-noop event handler.
//!
//! Additionally emits a `cargo::warning` counter showing how many components
//! carry a typed `#[ui_action(SomeEnum)]` vs. the total still missing one.

use crate::baseline::Violation;
use crate::walk::WorkspaceWalker;

/// Paths that are in scope for this scanner.
fn is_in_scope(s: &str) -> bool {
    (s.contains("crates/core/src/ui/") || s.contains("clients/")) && is_ui_rs(s)
}

fn is_ui_rs(s: &str) -> bool {
    s.ends_with(".rs")
}

/// Paths that should always be excluded regardless of scope.
fn is_excluded(s: &str) -> bool {
    s.contains("/tests/")
        || s.contains("/examples/")
        || s.contains("/mcp/")
        || s.contains("/servers/test-")
        || s.contains("/plugin-host-tests/")
        || s.contains("/target/")
}

pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    let mut typed_count: u32 = 0;
    let mut remaining_count: u32 = 0;

    for path in &walker.files {
        let s = path.to_string_lossy();
        if !is_in_scope(&s) || is_excluded(&s) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = walker.relative(path);
        let mut vs = scan_src(&content, &rel);

        // Tally coverage for the warning counter.
        // Count each #[component] in this file.
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.trim().starts_with("#[component]") {
                continue;
            }
            let annotation = find_ui_action_above(&lines, i);
            match annotation {
                UiActionKind::Typed => typed_count += 1,
                UiActionKind::None | UiActionKind::Inherit => {} // not "typed actions"
                UiActionKind::Missing => remaining_count += 1,
            }
        }

        violations.append(&mut vs);
    }

    println!(
        "cargo::warning=poly-action-coverage: {typed_count} components declare #[ui_action(SomeEnum)] ({remaining_count} remaining without typed actions)"
    );
}

/// Per-file scan — returns violations for `src` at `path`.
/// Split out so it can be re-used in lib.rs unit tests.
pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        if !line.trim().starts_with("#[component]") {
            continue;
        }

        let annotation = find_ui_action_above(&lines, i);
        let fn_name = extract_fn_name_below(&lines, i).unwrap_or_else(|| "<unknown>".into());

        // Rule A-enum: missing #[ui_action(...)] entirely.
        if annotation == UiActionKind::Missing {
            out.push(Violation {
                rule: "action_enum_coverage".into(),
                path: path.to_string(),
                line: (i as u32) + 1,
                detail: format!(
                    "#[component] fn {fn_name} missing #[ui_action(...)] — add one of \
                     `(YourActionEnum)` (typed actions), `(None)` (display-only), or \
                     `(inherit)` (sub-component delegates to parent)"
                ),
            });
        }

        // Rule B-enum: #[ui_action(None)] with a non-noop event handler.
        if annotation == UiActionKind::None {
            scan_rule_b_for_component(&lines, i, &fn_name, path, &mut out);
        }
    }

    out
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(PartialEq, Eq, Debug)]
enum UiActionKind {
    /// `#[ui_action(SomeName)]` where SomeName is not None or inherit.
    Typed,
    /// `#[ui_action(None)]`
    None,
    /// `#[ui_action(inherit)]`
    Inherit,
    /// No `#[ui_action(...)]` found in the attribute block above.
    Missing,
}

/// Walk upward from `component_idx` past attribute/blank/comment lines and
/// classify the `#[ui_action(...)]` found (or report Missing).
fn find_ui_action_above(lines: &[&str], component_idx: usize) -> UiActionKind {
    let mut i = component_idx;
    while i > 0 {
        i -= 1;
        let Some(line) = lines.get(i) else { break };
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("///") {
            continue;
        }
        if t.starts_with("#[") {
            if let Some(inner) = t.strip_prefix("#[ui_action(") {
                // inner is something like `None)]` or `inherit)]` or `MyEnum)]`
                let arg = inner
                    .trim_end_matches(|c: char| c == ')' || c == ']')
                    .trim();
                return if arg.eq_ignore_ascii_case("none") {
                    UiActionKind::None
                } else if arg.eq_ignore_ascii_case("inherit") {
                    UiActionKind::Inherit
                } else {
                    UiActionKind::Typed
                };
            }
            // Some other attribute — keep scanning upward.
            continue;
        }
        // Hit a non-attribute, non-blank line (e.g. `pub fn`, comment prose, etc.) — stop.
        break;
    }
    UiActionKind::Missing
}

/// Scan the body of the component starting just after `component_idx` for
/// event handlers that don't call `ui_noop!`. Only called when the component
/// is annotated `#[ui_action(None)]`.
fn scan_rule_b_for_component(
    lines: &[&str],
    component_idx: usize,
    fn_name: &str,
    path: &str,
    out: &mut Vec<Violation>,
) {
    let event_names = [
        "onclick:",
        "onchange:",
        "onsubmit:",
        "oninput:",
        "onkeydown:",
        "onkeyup:",
        "onmousedown:",
        "onmouseup:",
        "ondblclick:",
        "onfocus:",
        "onblur:",
    ];

    // Scan up to 200 lines after the #[component] line, or until the next #[component].
    let start = component_idx + 1;
    let end = (start + 200).min(lines.len());

    for (offset, line) in lines[start..end].iter().enumerate() {
        let abs_line = start + offset;
        let t = line.trim();

        // Stop at the next #[component] to avoid scanning a different component's body.
        if t.starts_with("#[component]") {
            break;
        }

        for ev in &event_names {
            if !t.contains(ev) {
                continue;
            }
            // Flag if no `ui_noop!` on the same line.
            if !t.contains("ui_noop!") {
                out.push(Violation {
                    rule: "action_enum_coverage_none_with_handler".into(),
                    path: path.to_string(),
                    line: (abs_line as u32) + 1,
                    detail: format!(
                        "#[ui_action(None)] component {fn_name} has a non-noop event handler \
                         at line {} — either change to #[ui_action(SomeEnum)] or use \
                         ui_noop!(UiNoopReason::X)",
                        abs_line + 1
                    ),
                });
                // One violation per line is enough.
                break;
            }
        }
    }
}

fn extract_fn_name_below(lines: &[&str], component_idx: usize) -> Option<String> {
    for line in lines.iter().skip(component_idx + 1).take(10) {
        let t = line.trim_start();
        // Skip further attrs / doc comments between #[component] and the fn itself.
        if t.starts_with("#[") || t.starts_with("//") || t.is_empty() {
            continue;
        }
        // Typical: `pub fn Name(...)` / `fn Name(...)` / `pub(crate) fn Name(...)`
        let after_fn = t.split_once(" fn ")?.1;
        let name_end = after_fn.find(|c: char| !c.is_alphanumeric() && c != '_')?;
        return Some(after_fn[..name_end].to_string());
    }
    None
}
