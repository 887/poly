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
//!
//! # Content-keyed allowlist (plan-lint-gate-integrity Phase B)
//!
//! This allowlist is the one that broke: it was keyed on `path:line`, and 90%
//! of its 1469 entries pointed at a line holding no `.read()` at all after
//! years of reflow. Those entries suppressed nothing while the sites they meant
//! to cover quietly moved into `crates/lint-gate/baseline.json`, so a later
//! one-line shift flipped sites between "allowlisted" and "baselined" and
//! manufactured a false new-hang-class violation.
//!
//! Entries are therefore keyed on `path@<ordinal>:<fingerprint>` (see
//! [`crate::allowlist`]), and [`check_allowlist_integrity`] fails the gate if
//! any entry stops resolving to a real hit — the 90%-inert state cannot recur
//! silently.

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_render_time_read";
/// Integrity self-check on the allowlist file itself (Phase B.5).
const RULE_ALLOWLIST: &str = "render_time_read_allowlist";
pub(crate) const ALLOWLIST_FILE: &str = "tools/scripts/render-time-read-allowlist.txt";

const DETAIL: &str = "render-time .read() silently subscribes the parent component — \
     CLAUDE.md hang class #7. Use .peek() for key computation / snapshots. \
     See docs/dev/reactive-state.md. \
     Inline-allowlist: // poly-lint: allow render-time-read — <reason>";

/// 1-based line numbers of every render-time `.read()` in `content`, **before**
/// the file allowlist is consulted.
///
/// Inline `// poly-lint: allow` suppression and the safe-context heuristics are
/// applied here because they are properties of the line itself, not of the
/// allowlist file.
#[must_use]
pub fn candidate_lines(content: &str) -> Vec<u32> {
    let mut out = Vec::new();
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

        out.push((line_idx as u32) + 1);
    }
    out
}

/// Is `path` inside this rule's scan scope?
#[must_use]
pub fn in_scan_scope(path: &str) -> bool {
    path.contains(SCAN_SUBDIR)
}

pub fn scan(walker: &WorkspaceWalker, ws_root: &Path, violations: &mut Vec<Violation>) {
    let scan_dir = ws_root.join(SCAN_SUBDIR);
    if !scan_dir.is_dir() {
        return;
    }
    let allowlist_path = ws_root.join(ALLOWLIST_FILE);
    let allowlist_entries = allowlist::load(&allowlist_path);

    for path in &walker.files {
        let s = path.to_string_lossy();
        if !in_scan_scope(&s) {
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

        let keys = allowlist::line_keys(&content);
        for line_no in candidate_lines(&content) {
            let suppressed = match keys.get((line_no as usize).saturating_sub(1)) {
                Some((fp, ordinal)) => {
                    allowlist::is_allowed_content(&allowlist_entries, &rel, fp, *ordinal)
                }
                None => false,
            }
            // Legacy `path:line` entries still work, so a hand-added entry is
            // not silently ignored — but `check_allowlist_integrity` reports it
            // so it gets re-keyed rather than accumulating.
            || allowlist::is_allowed(&allowlist_entries, &rel, line_no);
            if suppressed {
                continue;
            }

            violations.push(Violation {
                rule: RULE.to_string(),
                path: rel.clone(),
                line: line_no,
                detail: DETAIL.to_string(),
            });
        }
    }

    check_allowlist_integrity(ws_root, violations);
}

/// Phase B.5 — fail the gate when an allowlist entry suppresses nothing.
///
/// Two failure modes, both of which produced the 90%-inert state:
///   * a **legacy `path:line` entry**, which desyncs on the next reflow;
///   * a **content entry whose site no longer exists**, i.e. a stale suppression
///     that would silently start covering an unrelated future site if the file
///     ever regrew that exact line.
pub fn check_allowlist_integrity(ws_root: &Path, violations: &mut Vec<Violation>) {
    let allowlist_path = ws_root.join(ALLOWLIST_FILE);
    for (src_line, entry) in allowlist::load_with_lines(&allowlist_path) {
        let (rel, ordinal, fingerprint) = match &entry {
            allowlist::AllowlistEntry::PathContent {
                path,
                ordinal,
                fingerprint,
            } => (path.clone(), *ordinal, fingerprint.clone()),
            other @ (allowlist::AllowlistEntry::WholePath(_)
            | allowlist::AllowlistEntry::PathLine(..)
            | allowlist::AllowlistEntry::PathRange(..)
            | allowlist::AllowlistEntry::PathReceiver(..)) => {
                violations.push(Violation {
                    rule: RULE_ALLOWLIST.to_string(),
                    path: ALLOWLIST_FILE.to_string(),
                    line: src_line,
                    detail: format!(
                        "{ALLOWLIST_FILE} must be content-keyed (`path@<ordinal>:<fingerprint>`); \
                         found a line-keyed/whole-path entry ({other:?}). Line keys desync on the \
                         next reflow — that is what left 90% of this file inert. Regenerate with \
                         `cargo test -p poly-lint-gate-rules -- --ignored regenerate_render_time_read_allowlist`."
                    ),
                });
                continue;
            }
        };

        let resolved = std::fs::read_to_string(ws_root.join(&rel)).is_ok_and(|content| {
            let keys = allowlist::line_keys(&content);
            candidate_lines(&content).into_iter().any(|line_no| {
                keys.get((line_no as usize).saturating_sub(1))
                    .is_some_and(|(fp, ord)| *fp == fingerprint && *ord == ordinal)
            })
        });
        if !resolved {
            violations.push(Violation {
                rule: RULE_ALLOWLIST.to_string(),
                path: ALLOWLIST_FILE.to_string(),
                line: src_line,
                detail: format!(
                    "dead allowlist entry: `{key}` resolves to no render-time `.read()` in \
                     {rel}. Either the site was fixed (delete this line) or it moved content \
                     (regenerate). An entry that suppresses nothing is how this file went 90% \
                     inert while its real sites drifted into baseline.json.",
                    key = allowlist::render_content_key(&rel, ordinal, &fingerprint),
                ),
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

    /// Same anti-tautology guard as `forbid_raw_backend_read`: a scan scope
    /// that resolves to nothing makes every other test in this module vacuous.
    #[test]
    fn scan_scope_resolves_to_real_files_in_this_workspace() {
        let ws_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("crates/lint-gate-rules sits two levels below the workspace root");
        let walker = WorkspaceWalker::new(ws_root);
        let in_scope = walker
            .files
            .iter()
            .filter(|p| in_scan_scope(&p.to_string_lossy()))
            .count();
        assert!(
            in_scope > 50,
            "hang-class #7 scope ({SCAN_SUBDIR}) resolved to {in_scope} files — the rule is inert"
        );
    }

    /// The allowlist shipped in-tree must be fully content-keyed and fully
    /// live — Phase B.5's guarantee, asserted against the real file so a
    /// hand-added `path:line` entry fails `cargo test`, not just the gate.
    #[test]
    fn shipped_allowlist_is_content_keyed_and_live() {
        let ws_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("crates/lint-gate-rules sits two levels below the workspace root");
        let mut violations = Vec::new();
        check_allowlist_integrity(ws_root, &mut violations);
        assert!(
            violations.is_empty(),
            "{ALLOWLIST_FILE} has {} dead or line-keyed entries:\n{}",
            violations.len(),
            violations
                .iter()
                .map(Violation::to_error_line)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

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

    // ── candidate_lines: the matcher `scan` and the migration tool share ──────

    #[test]
    fn candidate_lines_reports_only_unsafe_render_reads() {
        let src = "\
let a = sig.read();
// let b = sig.read();
use_memo(|| sig.read());
let c = handle.read().await;
let d = sig.read(); // poly-lint: allow render-time-read — intentional
onclick: move |_| { sig.read(); },
let e = sig.read();
";
        assert_eq!(
            candidate_lines(src),
            vec![1, 7],
            "only the two bare render-time reads are candidates"
        );
    }

    // ── Phase B.5 — allowlist integrity self-check ────────────────────────────

    /// Build a throwaway workspace with one scanned file and one allowlist.
    fn fixture_ws(name: &str, source: &str, allowlist_body: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("poly-lint-gate-rtr-{name}"));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(root.join(SCAN_SUBDIR)).unwrap();
        std::fs::create_dir_all(root.join("tools/scripts")).unwrap();
        std::fs::write(root.join(SCAN_SUBDIR).join("f.rs"), source).unwrap();
        std::fs::write(root.join(ALLOWLIST_FILE), allowlist_body).unwrap();
        root
    }

    #[test]
    fn integrity_check_accepts_a_live_content_entry() {
        let src = "let a = sig.read();\n";
        let key = allowlist::render_content_key(
            "crates/core/src/ui/f.rs",
            0,
            &allowlist::fingerprint("let a = sig.read();"),
        );
        let root = fixture_ws("live", src, &format!("{key} # reason\n"));
        let mut violations = Vec::new();
        check_allowlist_integrity(&root, &mut violations);
        assert!(violations.is_empty(), "live entry must not be flagged: {violations:?}");
        drop(std::fs::remove_dir_all(&root));
    }

    /// The 90%-inert state: an entry whose site no longer exists.
    #[test]
    fn integrity_check_rejects_a_dead_content_entry() {
        let key = allowlist::render_content_key(
            "crates/core/src/ui/f.rs",
            0,
            &allowlist::fingerprint("let gone = sig.read();"),
        );
        let root = fixture_ws("dead", "let a = sig.read();\n", &format!("{key} # reason\n"));
        let mut violations = Vec::new();
        check_allowlist_integrity(&root, &mut violations);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, RULE_ALLOWLIST);
        assert!(violations[0].detail.contains("dead allowlist entry"));
        drop(std::fs::remove_dir_all(&root));
    }

    /// A hand-added `path:line` entry still suppresses, but must be reported so
    /// line keys cannot re-accumulate in this file.
    #[test]
    fn integrity_check_rejects_a_line_keyed_entry() {
        let root = fixture_ws(
            "linekeyed",
            "let a = sig.read();\n",
            "crates/core/src/ui/f.rs:1 # reason\n",
        );
        let mut violations = Vec::new();
        check_allowlist_integrity(&root, &mut violations);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].detail.contains("must be content-keyed"));
        drop(std::fs::remove_dir_all(&root));
    }

    /// The acceptance property: a reflow must not change what is suppressed.
    #[test]
    fn content_key_survives_a_reflow_that_a_line_key_would_not() {
        let before = "let a = sig.read();\n";
        let after = "// a new comment\n\nlet a = sig.read();\n";
        let keys_before = allowlist::line_keys(before);
        let keys_after = allowlist::line_keys(after);
        let hit_before = candidate_lines(before);
        let hit_after = candidate_lines(after);
        assert_eq!(hit_before, vec![1]);
        assert_eq!(hit_after, vec![3], "the line number moved");
        assert_eq!(
            keys_before.first(),
            keys_after.get(2),
            "the content key did not move"
        );
    }
}
