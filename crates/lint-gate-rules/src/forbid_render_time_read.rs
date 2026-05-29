//! Forbid render-time `.read()` subscriptions — hang class #7.
//!
//! Ported from `tools/scripts/forbid-render-time-read.sh` (Phases 1+2 of
//! docs/plans/plan-peek-vs-read.md).
//!
//! Flags every `.read()` call in `crates/core/src/ui/**/*.rs` that is NOT
//! clearly inside a safe closure or async body on the SAME LINE.
//!
//! Safe patterns (not flagged):
//!   `.read().await`         — backend arc lock (covered by forbid_raw_backend_read)
//!   `use_effect(move ||`   — subscription is intentional
//!   `use_resource(`        — subscription is intentional
//!   `use_memo(`            — subscription is intentional
//!   `spawn(async`          — async body
//!   `on*: move |` / `on*: |`  — event handler
//!
//! Allowlist file: `tools/scripts/render-time-read-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow render-time-read — <reason>`

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_render_time_read";
const ALLOWLIST_FILE: &str = "tools/scripts/render-time-read-allowlist.txt";

pub fn scan(walker: &WorkspaceWalker, ws_root: &Path, violations: &mut Vec<Violation>) {
    let scan_dir = ws_root.join(SCAN_SUBDIR);
    if !scan_dir.is_dir() {
        return;
    }
    let allowlist_entries = allowlist::load(&ws_root.join(ALLOWLIST_FILE));

    for path in &walker.files {
        let s = path.to_string_lossy();
        if !s.contains(SCAN_SUBDIR) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(ws_root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();

        for (line_idx, line) in content.lines().enumerate() {
            // Must contain `.read()`
            if !line.contains(".read()") {
                continue;
            }

            // Skip pure comment lines.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            // Skip `.read().await` — backend arc, covered by separate lint.
            if line.contains(".read().await") {
                continue;
            }

            // Skip inline allowlist.
            if allowlist::has_inline_allow(line, "render-time-read") {
                continue;
            }

            // Skip safe same-line patterns.
            if is_safe_context(line) {
                continue;
            }

            let line_no = (line_idx as u32) + 1;
            if allowlist::is_allowed(&allowlist_entries, &rel, line_no) {
                continue;
            }

            violations.push(Violation {
                rule: RULE.to_string(),
                path: rel.clone(),
                line: line_no,
                detail: "render-time .read() silently subscribes the parent component — \
                     CLAUDE.md hang class #7. Use .peek() for key computation / snapshots. \
                     See docs/dev/reactive-state.md. \
                     Inline-allowlist: // poly-lint: allow render-time-read — <reason>".to_string(),
            });
        }
    }
}

/// Returns true if the line contains a safe pattern where `.read()` is intentional.
fn is_safe_context(line: &str) -> bool {
    if line.contains("use_effect(move ||") {
        return true;
    }
    if line.contains("use_resource(") {
        return true;
    }
    if line.contains("use_memo(") {
        return true;
    }
    if line.contains("spawn(async") {
        return true;
    }
    // Event handlers: `on*: move |` or `on*: |`
    // Simple heuristic: look for `on` followed by lowercase letters then `: `
    if is_event_handler_line(line) {
        return true;
    }
    false
}

fn is_event_handler_line(line: &str) -> bool {
    // Pattern: `\bon[a-z_]+:\s*(move\s*)?\|`
    let mut search = line;
    while let Some(pos) = search.find("on") {
        let after = &search[pos..];
        // Check `on<lowercase>+:` followed by optional `move` then `|`
        let rest = &after[2..]; // skip "on"
        let ident_end = rest.find(|c: char| !c.is_ascii_lowercase() && c != '_').unwrap_or(rest.len());
        if ident_end == 0 {
            search = &search[pos + 1..];
            continue;
        }
        let after_ident = &rest[ident_end..].trim_start();
        if let Some(stripped) = after_ident.strip_prefix(':') {
            let rest2 = stripped.trim_start();
            let rest2 = rest2.strip_prefix("move").map_or(rest2, |s| s.trim_start());
            if rest2.starts_with('|') {
                return true;
            }
        }
        search = &search[pos + 1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_render_time_read() {
        // Not in any safe context
        let line = "    let val = my_signal.read().clone();";
        assert!(!is_safe_context(line), "plain read should not be safe context");
    }

    #[test]
    fn allows_use_effect() {
        let line = "    use_effect(move || { my_signal.read(); });";
        assert!(is_safe_context(line));
    }

    #[test]
    fn allows_event_handler() {
        let line = "    onclick: move |_| { my_signal.read(); },";
        assert!(is_safe_context(line));
    }

    #[test]
    fn allows_spawn_async() {
        let line = "    spawn(async move { my_signal.read(); });";
        assert!(is_safe_context(line));
    }
}
