//! UI action coverage scanner.
//!
//! Enforces three rules across all `.rs` files in `crates/core/src/ui/`
//! and `clients/*/src/`:
//!
//! - Rule A: no bare empty event handlers (`onclick: move |_| {}`)
//! - Rule B: no empty RSX view bodies (`rsx! {}` inside a `#[component]`)
//! - Rule C: `ui_noop!` argument must be `UiNoopReason::X`

use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

/// Scan all relevant source files and push violations.
pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    for path in &walker.files {
        let s = path.to_string_lossy();
        // Only scan UI source dirs; skip test/example/build artefacts.
        let in_ui = s.contains("crates/core/src/ui/") || s.contains("clients/");
        if !in_ui {
            continue;
        }
        if s.contains("/tests/")
            || s.contains("/examples/")
            || s.contains("/target/")
            || s.contains("/mcp/")
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = walker.relative(path);
        let mut vs = scan_src(&content, &rel);
        violations.append(&mut vs);
    }
}

/// Per-file scan — returns violations for `src` at `path`.
/// Split out so it can be re-used in lib.rs unit tests.
#[must_use] 
pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    scan_rule_a(src, path, &mut out);
    scan_rule_b(src, path, &mut out);
    scan_rule_c(src, path, &mut out);
    out
}

// ── Rule A ────────────────────────────────────────────────────────────────────
// Bare empty event handlers: `onclick: move |_| {}` / `onclick: |_| ()`
// Pattern: `on\w+:\s*(move\s*)?\|[^|{}\n]*\|\s*(\{\s*\}|\(\s*\))`

fn scan_rule_a(src: &str, path: &str, out: &mut Vec<Violation>) {
    let event_names = [
        "onclick",
        "onchange",
        "onsubmit",
        "oninput",
        "onkeydown",
        "onkeyup",
        "onmousedown",
        "onmouseup",
        "ondblclick",
        "onfocus",
        "onblur",
    ];

    for (line_idx, line) in src.lines().enumerate() {
        let trimmed = line.trim();
        for ev in &event_names {
            // Quick pre-filter before the heavier check.
            if !trimmed.contains(ev) {
                continue;
            }
            // Find the handler start after `ev:`
            let Some(after_colon) = find_handler_start(trimmed, ev) else {
                continue;
            };
            let rest = after_colon.trim_start();
            // Strip optional `move`
            let rest = rest.strip_prefix("move").map_or(rest, str::trim_start);
            // Must start with `|`
            if !rest.starts_with('|') {
                continue;
            }
            // Find matching closing `|`
            let after_open = &rest[1..];
            let Some(close_pipe) = after_open.find('|') else {
                continue;
            };
            let after_params = &after_open[close_pipe + 1..].trim_start();
            // Empty body: `{}` or `()`
            if is_empty_body(after_params) {
                out.push(Violation {
                    rule: "ui_action_coverage".into(),
                    path: path.to_string(),
                    line: (line_idx as u32) + 1,
                    detail: format!(
                        "empty event handler — use ui_noop!(UiNoopReason::X) for decorative \
                         elements or implement the handler; file: {path}:{}",
                        line_idx + 1
                    ),
                });
                break; // one violation per line is enough
            }
        }
    }
}

/// Returns the slice after `ev:` if `ev:` appears in `line`.
fn find_handler_start<'a>(line: &'a str, ev: &str) -> Option<&'a str> {
    // Look for `ev:` (with optional whitespace around the colon isn't typical in rsx,
    // but handle `ev :` just in case).
    let mut search = line;
    loop {
        let pos = search.find(ev)?;
        let after = &search[pos + ev.len()..];
        let after_ws = after.trim_start();
        if let Some(stripped) = after_ws.strip_prefix(':') {
            return Some(stripped);
        }
        // Keep searching past this occurrence.
        search = &search[pos + 1..];
    }
}

fn is_empty_body(s: &str) -> bool {
    if let Some(inner) = s.strip_prefix('{') {
        let rest = inner.trim_start();
        rest.starts_with('}')
    } else if let Some(inner) = s.strip_prefix('(') {
        let rest = inner.trim_start();
        rest.starts_with(')')
    } else {
        false
    }
}

// ── Rule B ────────────────────────────────────────────────────────────────────
// `#[component]` functions with `rsx! {}` (whitespace-only) as their entire body.

fn scan_rule_b(src: &str, path: &str, out: &mut Vec<Violation>) {
    let lines: Vec<&str> = src.lines().collect();
    let mut in_component = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[component]") {
            in_component = true;
            continue;
        }
        if !in_component {
            continue;
        }
        // Skip blank lines, doc comments, and further attributes between
        // #[component] and the fn body.
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with("#[")
            || trimmed.starts_with("pub fn")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("pub(crate) fn")
        {
            // If we hit a `fn` line, stay in component context until the body.
            continue;
        }
        // Check whether this line (or the next few) forms `rsx! { }` with nothing.
        // We look for `rsx!` followed by `{` and then only whitespace before `}`.
        if is_empty_rsx_on_line(trimmed) {
            out.push(Violation {
                rule: "ui_action_coverage".into(),
                path: path.to_string(),
                line: (i as u32) + 1,
                detail: format!(
                    "empty rsx! view body — implement the view or remove the route entry; \
                     file: {path}:{}",
                    i + 1
                ),
            });
        }
        // After finding any non-attribute, non-fn content line, reset.
        in_component = false;
    }
}

fn is_empty_rsx_on_line(s: &str) -> bool {
    // Match `rsx!` then optional whitespace then `{` then only whitespace then `}`
    let Some(after_rsx) = s.find("rsx!").map(|p| &s[p + 4..]) else {
        return false;
    };
    let rest = after_rsx.trim_start();
    // Strip optional `(`  — Dioxus supports both `rsx! {}` and `rsx!(...)`; focus on `{}`
    let rest = if let Some(stripped) = rest.strip_prefix('(') {
        stripped.trim_start()
    } else {
        rest
    };
    if !rest.starts_with('{') {
        return false;
    }
    let inner = rest[1..].trim();
    inner.starts_with('}')
}

// ── Rule C ────────────────────────────────────────────────────────────────────
// `ui_noop!` argument must start with `UiNoopReason::`.

fn scan_rule_c(src: &str, path: &str, out: &mut Vec<Violation>) {
    for (line_idx, line) in src.lines().enumerate() {
        if !line.contains("ui_noop!") {
            continue;
        }
        let trimmed = line.trim();
        // Find each `ui_noop!(` occurrence on this line.
        let mut search = trimmed;
        while let Some(pos) = search.find("ui_noop!(") {
            let after_open = &search[pos + "ui_noop!(".len()..];
            let inner = after_open.trim_start();
            // OK: starts with `UiNoopReason::`
            if inner.starts_with("UiNoopReason::") {
                search = &search[pos + 1..];
                continue;
            }
            // Flag: empty call or non-UiNoopReason argument.
            out.push(Violation {
                rule: "ui_action_coverage".into(),
                path: path.to_string(),
                line: (line_idx as u32) + 1,
                detail: format!(
                    "ui_noop! argument must be UiNoopReason::X — use one of DragHandle, \
                     ReadOnlyIndicator, DecorativeIcon, LayoutSpacer, EventBarrier, \
                     ProgressIndicator; file: {path}:{}",
                    line_idx + 1
                ),
            });
            break; // one violation per line
        }
    }
}
