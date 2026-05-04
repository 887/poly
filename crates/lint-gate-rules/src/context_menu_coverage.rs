//! Context-menu coverage scan — plan-context-menu-quality-control.md §3.1.2.
//!
//! Walks every `#[component]` site and emits a violation if the component lacks
//! a sibling `#[context_menu(...)]` attribute. Pass-through attribute macros
//! today accept any args, so the scan is purely structural: either an attribute
//! exists directly above the `#[component]` site or it doesn't.
//!
//! Skips:
//!   * tests/, examples/, mcp/, test-server crates — non-UI surfaces.
//!   * files whose crate-level attrs opt out via `#![allow(lint_gate::context_menu_missing)]`.

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
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        if content.contains("#![allow(lint_gate::context_menu_missing)]") {
            continue;
        }
        let rel = walker.relative(path);
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.trim().starts_with("#[component]") {
                continue;
            }
            if has_context_menu_above(&lines, i) {
                continue;
            }
            let fn_name = extract_fn_name_below(&lines, i).unwrap_or_else(|| "<unknown>".into());
            violations.push(Violation {
                rule: "context_menu_coverage".into(),
                path: rel.clone(),
                line: (i as u32) + 1,
                detail: format!(
                    "#[component] fn {fn_name} missing #[context_menu(...)] — add one of \
                     `(YourMenu)` (attach a menu), `(None)` (opt out), `(allow_default)` \
                     (native menu, e.g. images/inputs), or `(inherit)` (defer to parent)"
                ),
            });
        }
    }
}

fn has_context_menu_above(lines: &[&str], component_idx: usize) -> bool {
    // Walk upward past any attribute-ish lines (start with `#[`) and blank/comment lines;
    // stop at the first non-attribute line. Any `#[context_menu(...)]` in that window counts.
    let mut i = component_idx;
    while i > 0 {
        i -= 1;
        let Some(line) = lines.get(i) else { break };
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("///") {
            continue;
        }
        if t.starts_with("#[") {
            if t.starts_with("#[context_menu") {
                return true;
            }
            continue;
        }
        break;
    }
    false
}

fn extract_fn_name_below(lines: &[&str], component_idx: usize) -> Option<String> {
    for line in lines.iter().skip(component_idx + 1).take(10) {
        let t = line.trim_start();
        // skip further attrs / doc comments between #[component] and the fn itself
        if t.starts_with("#[") || t.starts_with("//") || t.is_empty() {
            continue;
        }
        // typical: `pub fn Name(...)` / `fn Name(...)` / `pub(crate) fn Name(...)`
        let after_fn = t.split_once(" fn ")?.1;
        let name_end = after_fn.find(|c: char| !c.is_alphanumeric() && c != '_')?;
        return Some(after_fn[..name_end].to_string());
    }
    None
}
