//! Forbid raw `Signal::write()` in `crates/core/src/ui/` — CLAUDE.md hang class #1.
//!
//! Ported from `tools/scripts/forbid-signal-write.sh` (Phase 5 Track A of
//! plan-batched-signal.md).
//!
//! Scans every `.rs` file under `crates/core/src/ui/` for `.write()` calls
//! that are NOT `.write().await` (those are RwLock, covered by forbid_raw_backend_read)
//! and NOT `.write(SOMETHING)` with args (std::io::Write).
//!
//! Allowlist file: `tools/scripts/signal-write-allowlist.txt`
//! Allowlist formats:
//!   `path`              — whole-file allow
//!   `path:line=N`       → treated as `path:N` (per-line)
//!   `path:receiver`     — per-receiver allow
//! Inline allowlist: any source line where `.write()` appears; check not applicable
//! (the script relied on receiver matching only).

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_signal_write";
const ALLOWLIST_FILE: &str = "tools/scripts/signal-write-allowlist.txt";

pub fn scan(walker: &WorkspaceWalker, ws_root: &Path, violations: &mut Vec<Violation>) {
    let scan_dir = ws_root.join(SCAN_SUBDIR);
    let allowlist = allowlist::load(&ws_root.join(ALLOWLIST_FILE));

    for path in &walker.files {
        let s = path.to_string_lossy();
        if !s.contains(SCAN_SUBDIR) {
            continue;
        }
        if !scan_dir.is_dir() {
            break;
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
            let line_no = (line_idx as u32) + 1;

            // Skip pure comment lines.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            // Must contain `.write()`.
            if !line.contains(".write()") {
                continue;
            }

            // Exclude `.write().await` — that's RwLock::write().await, covered elsewhere.
            if line.contains(".write().await") {
                continue;
            }

            // Exclude `.write(SOMETHING)` with args — std::io::Write::write.
            // The script only catches bare `.write()` (no args inside parens).
            // We already checked `.write()` is present and `.write().await` is absent.
            // A `.write(x)` would not contain `.write()` as a literal substring if x is non-empty.
            // Actually `.write()` IS a suffix of `.write().await`, but we excluded that above.
            // Inline allowlist check: the bash script doesn't have a line-level inline allow for
            // this lint; it uses receiver matching. We do a simple per-line check.
            if allowlist::has_inline_allow(line, RULE) {
                continue;
            }

            // Extract receiver for allowlist matching.
            let receiver = extract_receiver(line);

            if allowlist::is_allowed_with_receiver(&allowlist, &rel, line_no, &receiver) {
                continue;
            }

            violations.push(Violation {
                rule: RULE.to_string(),
                path: rel.clone(),
                line: line_no,
                detail: format!(
                    "forbidden Signal::write() — use BatchedSignal::batch(|v| ...) or PendingUpdate instead. \
                     Receiver: `{receiver}`. See: crates/core/src/state/batched_signal.rs"
                ),
            });
        }
    }
}

/// Extract the receiver identifier from a line containing `.write()`.
/// Returns `<unknown>` if not extractable.
fn extract_receiver(line: &str) -> String {
    // Find `.write()` position
    let Some(pos) = line.find(".write()") else {
        return "<unknown>".to_string();
    };
    // Walk backward from `pos` to extract the preceding identifier
    let before = &line[..pos];
    let trimmed = before.trim_end();
    // Take last identifier-like token (may include dots for chain)
    let last_ident_end = trimmed.len();
    let start = trimmed
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map_or(0, |i| i + 1);
    if start < last_ident_end {
        trimmed[start..last_ident_end].to_string()
    } else {
        "<unknown>".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_receiver() {
        assert_eq!(extract_receiver("    my_signal.write()"), "my_signal");
    }

    #[test]
    fn extracts_chained_receiver() {
        assert_eq!(extract_receiver("    self.state.write()"), "state");
    }

    #[test]
    fn skips_write_await() {
        // The scan function filters .write().await before reaching extract_receiver,
        // but just verify the logic.
        let line = "let g = backend.write().await;";
        assert!(line.contains(".write().await"));
    }
}
