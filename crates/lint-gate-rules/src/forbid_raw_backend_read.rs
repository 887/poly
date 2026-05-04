//! Forbid raw `backend.read().await` — hang class #4 and persona class P4.
//!
//! Ported from `tools/scripts/forbid-raw-backend-read.sh` (Phase 5 Track A of
//! plan-backend-read-timeout.md, Phase Q.4 of plan-persona-quality-gates.md).
//!
//! Scans `crates/core/src/ui/` and `mcp/chat-mcp/src/persona/` for raw
//! `backend.read().await` calls. Use `BackendHandleExt::read_with_timeout` instead.
//!
//! Allowlist file: `tools/scripts/raw-backend-read-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow raw backend.read().await — <reason>`

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR_UI: &str = "crates/core/src/ui";
const SCAN_SUBDIR_PERSONA: &str = "mcp/chat-mcp/src/persona";
const RULE: &str = "forbid_raw_backend_read";
const ALLOWLIST_FILE: &str = "tools/scripts/raw-backend-read-allowlist.txt";
const NEEDLE: &str = "backend.read().await";
const INLINE_ALLOW_TOKEN: &str = "poly-lint: allow raw backend.read().await";

pub fn scan(walker: &WorkspaceWalker, ws_root: &Path, violations: &mut Vec<Violation>) {
    let allowlist_entries = allowlist::load(&ws_root.join(ALLOWLIST_FILE));

    for path in &walker.files {
        let s = path.to_string_lossy();
        let in_ui = s.contains(SCAN_SUBDIR_UI);
        let in_persona = s.contains(SCAN_SUBDIR_PERSONA);
        if !in_ui && !in_persona {
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
            if !line.contains(NEEDLE) {
                continue;
            }
            // Inline allowlist.
            if line.contains(INLINE_ALLOW_TOKEN) {
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
                detail: format!(
                    "raw `backend.read().await` — hang class #4 (RwLock starvation on WASM). \
                     Use BackendHandleExt::read_with_timeout(Duration::from_secs(5)) instead. \
                     See: crates/core/src/client_manager_timeout.rs. \
                     Inline-allowlist: // poly-lint: allow raw backend.read().await — <reason>"
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations_for(src: &str) -> Vec<Violation> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if line.contains(NEEDLE) && !line.contains(INLINE_ALLOW_TOKEN) {
                out.push(Violation {
                    rule: RULE.to_string(),
                    path: "test.rs".to_string(),
                    line: (i as u32) + 1,
                    detail: "test".to_string(),
                });
            }
        }
        out
    }

    #[test]
    fn flags_raw_backend_read_await() {
        let src = "    let g = backend.read().await;";
        assert!(!violations_for(src).is_empty(), "should flag raw backend.read().await");
    }

    #[test]
    fn allows_inline_allowlisted() {
        let src = "    let g = backend.read().await; // poly-lint: allow raw backend.read().await — legacy";
        assert!(violations_for(src).is_empty(), "inline allow should pass");
    }
}
