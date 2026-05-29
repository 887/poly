//! Forbid raw `backend.read().await` — hang class #4 and persona class P4.
//!
//! Ported from `tools/scripts/forbid-raw-backend-read.sh` (Phase 5 Track A of
//! plan-backend-read-timeout.md, Phase Q.4 of plan-persona-quality-gates.md).
//!
//! **Phase K.8 extension** (plan-voice-video-calls.md): also scans voice transport
//! files in `clients/{discord,stoat,teams}/src/voice*.rs`. Voice code runs on native
//! only (never on wasm32 — see `#[cfg(feature = "voice")]` guards) but the rule
//! still applies because the async runtime can starve under a perpetual writer, and
//! the canonical pattern (`BackendHandleExt::read_with_timeout`) applies to all
//! async Rust code in this codebase.
//!
//! Scans:
//!   - `crates/core/src/ui/`          — original hang-class #4 scope
//!   - `mcp/chat-mcp/src/persona/`    — persona class P4 scope
//!   - `clients/discord/src/voice*`   — Phase K.8 extension
//!   - `clients/stoat/src/voice*`     — Phase K.8 extension (Phase F when shipped)
//!   - `clients/teams/src/voice*`     — Phase K.8 extension (Phase I stub)
//!
//! Allowlist file: `tools/scripts/raw-backend-read-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow raw backend.read().await — <reason>`

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR_UI: &str = "crates/core/src/ui";
const SCAN_SUBDIR_PERSONA: &str = "mcp/chat-mcp/src/persona";
// Phase K.8: voice transport files in the three clients that ship voice code.
const SCAN_SUBDIR_DISCORD_VOICE: &str = "clients/discord/src/voice";
const SCAN_SUBDIR_STOAT_VOICE: &str = "clients/stoat/src/voice";
const SCAN_SUBDIR_TEAMS_VOICE: &str = "clients/teams/src/voice";
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
        // Phase K.8: voice transport files — match any path segment starting with "voice"
        // inside the three client crates' src/ directories.
        let in_discord_voice = s.contains(SCAN_SUBDIR_DISCORD_VOICE);
        let in_stoat_voice = s.contains(SCAN_SUBDIR_STOAT_VOICE);
        let in_teams_voice = s.contains(SCAN_SUBDIR_TEAMS_VOICE);
        if !in_ui && !in_persona && !in_discord_voice && !in_stoat_voice && !in_teams_voice {
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
                detail: "raw `backend.read().await` — hang class #4 (RwLock starvation on WASM) \
                     and Phase K.8 voice transport lint. \
                     Use BackendHandleExt::read_with_timeout(Duration::from_secs(5)) instead. \
                     See: crates/core/src/client_manager_timeout.rs. \
                     Voice files (clients/*/src/voice*.rs) are also covered per \
                     docs/plans/plan-voice-video-calls.md Phase K.8. \
                     Inline-allowlist: // poly-lint: allow raw backend.read().await — <reason>".to_string(),
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

    // ── Phase K.8 — voice transport path inclusion ────────────────────────────

    /// Verify that paths matching the voice transport subdirs are in-scope.
    #[test]
    fn voice_paths_are_in_scope() {
        let voice_paths = [
            "clients/discord/src/voice.rs",
            "clients/discord/src/voice_ws.rs",
            "clients/stoat/src/voice.rs",
            "clients/stoat/src/voice_transport.rs",
            "clients/teams/src/voice.rs",
        ];
        for p in &voice_paths {
            let in_discord_voice = p.contains(SCAN_SUBDIR_DISCORD_VOICE);
            let in_stoat_voice = p.contains(SCAN_SUBDIR_STOAT_VOICE);
            let in_teams_voice = p.contains(SCAN_SUBDIR_TEAMS_VOICE);
            assert!(
                in_discord_voice || in_stoat_voice || in_teams_voice,
                "path {p} should be in scope for the voice lint"
            );
        }
    }

    /// Verify that non-voice client files are NOT in scope (avoid over-scanning).
    #[test]
    fn non_voice_client_paths_not_in_scope() {
        let non_voice_paths = [
            "clients/discord/src/lib.rs",
            "clients/discord/src/api.rs",
            "clients/stoat/src/lib.rs",
            "clients/teams/src/lib.rs",
            "clients/demo/src/lib.rs",
        ];
        for p in &non_voice_paths {
            let in_ui = p.contains(SCAN_SUBDIR_UI);
            let in_persona = p.contains(SCAN_SUBDIR_PERSONA);
            let in_discord_voice = p.contains(SCAN_SUBDIR_DISCORD_VOICE);
            let in_stoat_voice = p.contains(SCAN_SUBDIR_STOAT_VOICE);
            let in_teams_voice = p.contains(SCAN_SUBDIR_TEAMS_VOICE);
            assert!(
                !in_ui && !in_persona && !in_discord_voice && !in_stoat_voice && !in_teams_voice,
                "path {p} must NOT be in scope (would cause over-scanning of non-voice code)"
            );
        }
    }
}
