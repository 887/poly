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
const INLINE_ALLOW_TOKEN: &str = "poly-lint: allow raw backend.read().await";

/// The awaited-read call we forbid. Deliberately **receiver-agnostic**: the
/// original port from the shell script matched the literal string
/// `backend.read().await`, but no call site in this codebase actually names its
/// handle `backend` — they bind it as `handle`, `bh`, `cat_handle`, `b`, … so
/// the rule matched nothing and sat at zero rows in the baseline while the hang
/// class it guards was still reachable. Match the call shape, not the receiver.
const READ_CALL: &str = ".read()";
const AWAIT_SUFFIX: &str = ".await";

/// Returns the 1-based line numbers of every raw awaited `.read()` in `content`.
///
/// Handles the multi-line builder form as well as the single-line one:
///
/// ```text
/// let g = handle.read().await;          // flagged
/// let g = handle                        // flagged (reported on the `.read()` line)
///     .read()
///     .await;
/// ```
///
/// A line carrying [`INLINE_ALLOW_TOKEN`] anywhere in the span between the
/// `.read()` and its `.await` suppresses the hit.
fn find_violations(content: &str) -> Vec<u32> {
    let bytes = content.as_bytes();
    // Byte offset -> 1-based line number, via a prefix scan of newlines.
    let line_of = |offset: usize| -> u32 {
        let nl = bytes.get(..offset).map_or(0, bytecount_newlines);
        nl + 1
    };

    let mut out = Vec::new();
    for (idx, _) in content.match_indices(READ_CALL) {
        // Skip matches inside a line comment. Doc comments routinely *quote*
        // the forbidden pattern (e.g. "was a multi-line `b.read().await`"),
        // and flagging prose would make the rule unfixable.
        let line_start = content
            .get(..idx)
            .and_then(|p| p.rfind('\n').map(|n| n + 1))
            .unwrap_or(0);
        if content
            .get(line_start..idx)
            .is_some_and(|prefix| prefix.contains("//"))
        {
            continue;
        }
        let after = idx + READ_CALL.len();
        let Some(rest) = content.get(after..) else {
            continue;
        };
        // Skip whitespace (including newlines) between `.read()` and `.await`.
        let trimmed = rest.trim_start();
        if !trimmed.starts_with(AWAIT_SUFFIX) {
            continue;
        }
        let start_line = line_of(idx);
        let gap = rest.len() - trimmed.len();
        let end_line = line_of(after + gap);

        // Inline allow may sit on any line of the `.read() … .await` span.
        let suppressed = content
            .lines()
            .skip((start_line as usize).saturating_sub(1))
            .take((end_line - start_line + 1) as usize)
            .any(|l| l.contains(INLINE_ALLOW_TOKEN));
        if suppressed {
            continue;
        }
        out.push(start_line);
    }
    out
}

fn bytecount_newlines(b: &[u8]) -> u32 {
    let n = b.iter().filter(|&&c| c == b'\n').count();
    u32::try_from(n).unwrap_or(u32::MAX)
}

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

        for line_no in find_violations(&content) {
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

    // NOTE: these call the REAL matcher (`find_violations`). The previous
    // version of this module re-implemented the match inside the test helper,
    // so the tests passed while `scan` matched nothing in the whole workspace.
    // Never assert against a re-implementation of the thing under test.

    #[test]
    fn flags_raw_backend_read_await() {
        assert_eq!(find_violations("    let g = backend.read().await;"), vec![1]);
    }

    /// The regression that made this rule inert: no real call site is named
    /// `backend`. Every one of these must flag.
    #[test]
    fn flags_every_receiver_name_not_just_backend() {
        for recv in ["handle", "bh", "cat_handle", "b", "self.inner", "guard_src"] {
            let src = format!("    let g = {recv}.read().await;");
            assert_eq!(
                find_violations(&src),
                vec![1],
                "receiver `{recv}` must be flagged — the rule is receiver-agnostic"
            );
        }
    }

    /// The multi-line builder form a line-based scanner cannot see.
    #[test]
    fn flags_multiline_read_await() {
        let src = "let g = handle\n    .read()\n    .await;";
        assert_eq!(
            find_violations(src),
            vec![2],
            "multi-line `.read()` / `.await` must flag, reported on the `.read()` line"
        );
    }

    #[test]
    fn allows_inline_allowlisted() {
        let src =
            "    let g = backend.read().await; // poly-lint: allow raw backend.read().await — legacy";
        assert!(find_violations(src).is_empty(), "inline allow should pass");
    }

    #[test]
    fn allows_inline_allowlisted_on_multiline_span() {
        let src = "let g = handle\n    .read() // poly-lint: allow raw backend.read().await — legacy\n    .await;";
        assert!(
            find_violations(src).is_empty(),
            "inline allow anywhere in the span should pass"
        );
    }

    /// `read_with_timeout` is the sanctioned replacement — it must never flag.
    #[test]
    fn does_not_flag_read_with_timeout() {
        let src = "let g = handle.read_with_timeout(Duration::from_secs(5)).await;";
        assert!(find_violations(src).is_empty(), "the canonical fix must not flag");
    }

    /// A non-awaited `.read()` is a Signal read (hang class #7's territory),
    /// not an async RwLock starvation site. Out of scope for this rule.
    #[test]
    fn does_not_flag_unawaited_read() {
        assert!(find_violations("let v = *some_signal.read();").is_empty());
    }

    #[test]
    fn reports_every_hit_in_a_file() {
        // Line 4 is the `.read()` line of the multi-line hit — the rule anchors
        // on `.read()`, not on the receiver line.
        let src = "a.read().await;\nlet x = 1;\nb\n  .read()\n  .await;";
        assert_eq!(find_violations(src), vec![1, 4]);
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
