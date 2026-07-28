//! Shared allowlist loader for all `forbid-*.sh`-style scanners.
//!
//! Each bash script reimplemented the same allowlist-loading logic. This
//! module extracts that logic once, handling:
//!  - `# comment` lines and blank lines (stripped)
//!  - Whole-file entries: just the repo-relative path
//!  - Line entries: `path:line`
//!  - Range entries: `path:start-end`
//!  - Receiver entries (for signal-write): `path:receiver`
//!  - **Content entries**: `path@<ordinal>:h<16 hex>` — see below
//!
//! Inline allowlist: the `// poly-lint: allow <name> — <reason>` comment
//! syntax is checked by individual scanners at hit-detection time.
//!
//! # Why content keys exist
//!
//! A `path:line` key desyncs the moment anything above it in the file grows or
//! shrinks by a line. `tools/scripts/render-time-read-allowlist.txt` reached a
//! state where 90% of its entries named a line holding no `.read()` at all: the
//! suppressions were inert and the sites they meant to cover had silently
//! migrated into `crates/lint-gate/baseline.json` instead. A later reflow then
//! flipped sites between "allowlisted" and "baselined" at random and produced a
//! *false* new-hang-class violation.
//!
//! A content key survives reflow because it names *what* is suppressed, not
//! *where* it currently sits:
//!
//! ```text
//! crates/core/src/ui/foo.rs@0:h1f3a…  # reason | let n = sig.read().len();
//!                          │  └── FNV-1a 64 of the whitespace-normalised line
//!                          └───── which occurrence of that exact line in the file
//! ```
//!
//! The **ordinal** is what keeps a content key from silently widening: two
//! byte-identical `.read()` lines in one file are distinct sites, and a bare
//! content hash would suppress both from one entry.

use std::path::Path;

/// A parsed entry from an allowlist file.
#[derive(Debug, Clone)]
pub enum AllowlistEntry {
    /// The entire file is allowed.
    WholePath(String),
    /// A specific line in a file is allowed.
    PathLine(String, u32),
    /// A range of lines in a file is allowed.
    PathRange(String, u32, u32),
    /// A specific receiver name in a file is allowed (signal-write).
    PathReceiver(String, String),
    /// A specific *line content* in a file is allowed — reflow-stable.
    ///
    /// Serialised as `path@<ordinal>:<fingerprint>`.
    PathContent {
        /// Repo-relative path of the suppressed file.
        path: String,
        /// 0-based index of this occurrence among identical normalised lines.
        ordinal: u32,
        /// [`fingerprint`] of the normalised source line.
        fingerprint: String,
    },
}

/// Length of a rendered [`fingerprint`]: `h` + 16 lowercase hex digits.
const FINGERPRINT_LEN: usize = 17;

/// Collapse every run of whitespace to a single space and trim the ends.
///
/// Re-indentation (a rustfmt pass, a wrapping `if` block) must not invalidate a
/// suppression, so indentation is not part of the key.
#[must_use]
pub fn normalise_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for word in line.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// FNV-1a 64 of the normalised line, rendered as `h` + 16 lowercase hex digits.
///
/// The leading `h` is deliberate: it guarantees the suffix can never parse as a
/// `u32` line number, so a content entry is never mistaken for a `path:line`
/// entry by [`load`].
#[must_use]
pub fn fingerprint(line: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let normalised = normalise_line(line);
    let mut hash = OFFSET;
    for byte in normalised.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("h{hash:016x}")
}

/// Is `s` a rendered [`fingerprint`]?
#[must_use]
pub fn is_fingerprint(s: &str) -> bool {
    s.len() == FINGERPRINT_LEN
        && s.starts_with('h')
        && s.bytes()
            .skip(1)
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Per-line `(fingerprint, ordinal)` keys for a whole file, in line order.
///
/// Index `i` of the returned vec is the key for 1-based line `i + 1`. Computed
/// in one pass so a scanner with many hits in one file stays linear.
#[must_use]
pub fn line_keys(content: &str) -> Vec<(String, u32)> {
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for line in content.lines() {
        let fp = fingerprint(line);
        let counter = seen.entry(fp.clone()).or_insert(0);
        out.push((fp, *counter));
        *counter = counter.saturating_add(1);
    }
    out
}

/// The `(fingerprint, ordinal)` key of 1-based `line_no` in `content`.
///
/// Returns `None` when `line_no` is 0 or past the end of the file — the caller
/// must then fall back to a line key rather than invent a content key.
#[must_use]
pub fn key_at(content: &str, line_no: u32) -> Option<(String, u32)> {
    let idx = usize::try_from(line_no).ok()?.checked_sub(1)?;
    line_keys(content).get(idx).cloned()
}

/// Load an allowlist file and parse its entries.
///
/// Format per line (after stripping `#` comments and blank lines):
///   `path`                   → [`AllowlistEntry::WholePath`]
///   `path:42`                → [`AllowlistEntry::PathLine`]
///   `path:10-20`             → [`AllowlistEntry::PathRange`]
///   `path@0:h0123456789abcdef` → [`AllowlistEntry::PathContent`]
///   `path:receiver_name`     → [`AllowlistEntry::PathReceiver`] (if non-numeric)
#[must_use]
pub fn load(path: &Path) -> Vec<AllowlistEntry> {
    load_with_lines(path)
        .into_iter()
        .map(|(_, entry)| entry)
        .collect()
}

/// Like [`load`], but pairs every entry with its 1-based line number **in the
/// allowlist file**, so an integrity check can point a maintainer at the exact
/// entry to delete.
#[must_use]
pub fn load_with_lines(path: &Path) -> Vec<(u32, AllowlistEntry)> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for (idx, raw_line) in content.lines().enumerate() {
        let src_line = u32::try_from(idx.saturating_add(1)).unwrap_or(u32::MAX);
        // Strip inline comments.
        let line = match raw_line.split_once('#') {
            Some((before, _)) => before.trim(),
            None => raw_line.trim(),
        };
        if line.is_empty() {
            continue;
        }
        if let Some(entry) = parse_entry(line) {
            entries.push((src_line, entry));
        }
    }
    entries
}

/// Parse one already-comment-stripped, already-trimmed, non-empty entry.
fn parse_entry(line: &str) -> Option<AllowlistEntry> {
    // Try to parse `path:suffix`.
    if let Some(colon_pos) = line.rfind(':') {
        let path_part = &line[..colon_pos];
        let suffix = &line[colon_pos + 1..];
        // Content key: `path@<ordinal>:h<hex16>`. Checked first because its
        // suffix is unambiguous — a fingerprint can never parse as a u32.
        if is_fingerprint(suffix)
            && let Some((p, ord_str)) = path_part.rsplit_once('@')
            && let Ok(ordinal) = ord_str.parse::<u32>()
        {
            return Some(AllowlistEntry::PathContent {
                path: p.to_string(),
                ordinal,
                fingerprint: suffix.to_string(),
            });
        }
        // Is suffix a plain integer (line number)?
        if let Ok(n) = suffix.parse::<u32>() {
            return Some(AllowlistEntry::PathLine(path_part.to_string(), n));
        }
        // Is suffix a range `start-end`?
        if let Some((lo_str, hi_str)) = suffix.split_once('-')
            && let (Ok(lo), Ok(hi)) = (lo_str.parse::<u32>(), hi_str.parse::<u32>())
        {
            return Some(AllowlistEntry::PathRange(path_part.to_string(), lo, hi));
        }
        // Otherwise treat as a receiver/name suffix.
        return Some(AllowlistEntry::PathReceiver(
            path_part.to_string(),
            suffix.to_string(),
        ));
    }
    // No colon — whole-file allow.
    Some(AllowlistEntry::WholePath(line.to_string()))
}

/// Render a content entry back to its on-disk form (without the `# reason`).
#[must_use]
pub fn render_content_key(rel_path: &str, ordinal: u32, fingerprint: &str) -> String {
    format!("{rel_path}@{ordinal}:{fingerprint}")
}

/// Check whether a hit at `(rel_path, line)` is covered by any allowlist entry.
///
/// [`AllowlistEntry::PathContent`] entries are **never** matched here — deciding
/// them needs the source line, which this signature does not carry. Callers that
/// support content keys use [`is_allowed_content`] in addition.
#[must_use]
pub fn is_allowed(entries: &[AllowlistEntry], rel_path: &str, line: u32) -> bool {
    for entry in entries {
        match entry {
            AllowlistEntry::WholePath(p) => {
                if p == rel_path {
                    return true;
                }
            }
            AllowlistEntry::PathLine(p, n) => {
                if p == rel_path && *n == line {
                    return true;
                }
            }
            AllowlistEntry::PathRange(p, lo, hi) => {
                if p == rel_path && line >= *lo && line <= *hi {
                    return true;
                }
            }
            AllowlistEntry::PathReceiver(p, _r) => {
                // Receiver matching requires caller to pass the receiver; for
                // generic is_allowed check we just match the path.
                if p == rel_path {
                    return true;
                }
            }
            AllowlistEntry::PathContent { .. } => {}
        }
    }
    false
}

/// Check whether a hit whose source line has key `(fingerprint, ordinal)` in
/// `rel_path` is covered by a content entry — or by a whole-file entry.
#[must_use]
pub fn is_allowed_content(
    entries: &[AllowlistEntry],
    rel_path: &str,
    fingerprint: &str,
    ordinal: u32,
) -> bool {
    entries.iter().any(|entry| match entry {
        AllowlistEntry::WholePath(p) => p == rel_path,
        AllowlistEntry::PathContent {
            path,
            ordinal: o,
            fingerprint: fp,
        } => path == rel_path && *o == ordinal && fp == fingerprint,
        AllowlistEntry::PathLine(..)
        | AllowlistEntry::PathRange(..)
        | AllowlistEntry::PathReceiver(..) => false,
    })
}

/// Check whether a hit at `(rel_path, line, receiver)` is covered by any allowlist entry.
/// Used by `forbid_signal_write` which has the three-way allowlist.
#[must_use] 
pub fn is_allowed_with_receiver(
    entries: &[AllowlistEntry],
    rel_path: &str,
    line: u32,
    receiver: &str,
) -> bool {
    for entry in entries {
        match entry {
            AllowlistEntry::WholePath(p) => {
                if p == rel_path {
                    return true;
                }
            }
            AllowlistEntry::PathLine(p, n) => {
                if p == rel_path && *n == line {
                    return true;
                }
            }
            AllowlistEntry::PathRange(p, lo, hi) => {
                if p == rel_path && line >= *lo && line <= *hi {
                    return true;
                }
            }
            AllowlistEntry::PathReceiver(p, r) => {
                if p == rel_path && r == receiver {
                    return true;
                }
            }
            AllowlistEntry::PathContent { .. } => {}
        }
    }
    false
}

/// Checks if a source line contains an inline allowlist comment for the given lint name.
///
/// Pattern: `// poly-lint: allow <name>` (anywhere on the line).
#[must_use] 
pub fn has_inline_allow(line: &str, lint_name: &str) -> bool {
    // Allow both `—` (em dash) and `-` separators after the name.
    let needle = format!("poly-lint: allow {lint_name}");
    line.contains(&needle)
}

/// How far past the flagged line an inline allow may sit and still suppress it.
///
/// `rustfmt` relocates a trailing comment when it reformats the line it sits
/// on. A suppression written as
///
/// ```text
/// use_effect(move || { // poly-lint: allow stale-effect-capture — reason
/// ```
///
/// becomes
///
/// ```text
/// use_effect(move || {
///     // poly-lint: allow stale-effect-capture — reason
/// ```
///
/// — the comment moves *into* the block, one line down. A strictly same-line
/// check therefore stops suppressing after any formatting pass. A repo-wide
/// `cargo fmt` unsuppressed ~70 legitimately-allowed sites exactly this way;
/// they then present as brand-new hang-class violations and invite the
/// baseline regen `CLAUDE.md` forbids, which would grandfather them for real.
///
/// This is the *line-key* analogue of the content-key fix above: a content key
/// stops a suppression drifting when the file reflows, and this stops an
/// **inline** suppression detaching when its own line reflows.
///
/// Two lines covers rustfmt's relocation.
const INLINE_ALLOW_LOOKAHEAD: usize = 2;

/// Could `line` have had a trailing comment relocated off it by rustfmt?
///
/// Only lines that **open a block** lose their trailing comment — rustfmt moves
/// it inside the braces. A complete statement (`let x = sig.read().clone();`)
/// keeps its trailing comment exactly where it is.
///
/// This guard is load-bearing, not decorative. Without it the lookahead reaches
/// forward from one flagged line onto the *next* one's suppression comment. In
/// `account_bar.rs` three `.read()` calls sit on consecutive lines and only the
/// third carries an allow; an unguarded window silently suppressed the first two
/// as well. Over-suppression in a hang-class gate is worse than the bug it was
/// meant to fix.
fn opens_a_block(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or(line).trim_end();
    code.ends_with('{')
}

/// Checks for an inline allowlist comment on `lines[idx]` — or, when that line
/// opens a block, within [`INLINE_ALLOW_LOOKAHEAD`] lines after it.
///
/// Prefer this over [`has_inline_allow`] in any rule that can flag a
/// block-opening line; see [`INLINE_ALLOW_LOOKAHEAD`] for why.
#[must_use]
pub fn has_inline_allow_near(lines: &[&str], idx: usize, lint_name: &str) -> bool {
    let Some(flagged) = lines.get(idx) else {
        return false;
    };
    if has_inline_allow(flagged, lint_name) {
        return true;
    }
    if !opens_a_block(flagged) {
        return false;
    }
    lines
        .iter()
        .skip(idx.saturating_add(1))
        .take(INLINE_ALLOW_LOOKAHEAD)
        .any(|l| has_inline_allow(l, lint_name))
}

#[cfg(test)]
mod inline_allow_tests {
    use super::*;

    #[test]
    fn same_line_allow_still_suppresses() {
        let lines = vec!["use_effect(move || { // poly-lint: allow stale-effect-capture — ok"];
        assert!(has_inline_allow_near(&lines, 0, "stale-effect-capture"));
    }

    /// The regression this exists for: rustfmt pushes the trailing comment into
    /// the block, one line down.
    #[test]
    fn allow_relocated_by_rustfmt_still_suppresses() {
        let lines = vec![
            "use_effect(move || {",
            "    // poly-lint: allow stale-effect-capture — ok",
            "    let x = 1;",
        ];
        assert!(has_inline_allow_near(&lines, 0, "stale-effect-capture"));
    }

    #[test]
    fn allow_beyond_the_window_does_not_suppress() {
        let lines = vec![
            "use_effect(move || {",
            "    let a = 1;",
            "    let b = 2;",
            "    // poly-lint: allow stale-effect-capture — too far",
        ];
        assert!(!has_inline_allow_near(&lines, 0, "stale-effect-capture"));
    }

    #[test]
    fn a_different_lints_allow_does_not_suppress() {
        let lines = vec![
            "use_effect(move || {",
            "    // poly-lint: allow render-time-read — other",
        ];
        assert!(!has_inline_allow_near(&lines, 0, "stale-effect-capture"));
    }

    #[test]
    fn no_allow_does_not_suppress() {
        let lines = vec!["use_effect(move || {", "    let x = 1;"];
        assert!(!has_inline_allow_near(&lines, 0, "stale-effect-capture"));
    }

    /// The over-suppression regression: consecutive flagged statements where
    /// only the last carries an allow. Taken from a real site in
    /// `account_bar.rs`. An unguarded lookahead suppressed all three.
    #[test]
    fn a_statements_allow_does_not_leak_onto_the_lines_above_it() {
        let lines = vec![
            "    let voice_conn = voice_state.read().voice_connection.clone();",
            "    let nav_snap = nav_state.read().clone();",
            "    let as_snap = account_sessions.read().clone(); // poly-lint: allow render-time-read — snapshot",
        ];
        assert!(
            !has_inline_allow_near(&lines, 0, "render-time-read"),
            "line 0 is a complete statement; the allow two lines below is not its own"
        );
        assert!(!has_inline_allow_near(&lines, 1, "render-time-read"));
        assert!(
            has_inline_allow_near(&lines, 2, "render-time-read"),
            "the line actually carrying the allow is still suppressed"
        );
    }

    /// A trailing comment on a non-block line is never relocated, so no
    /// lookahead applies there.
    #[test]
    fn lookahead_only_applies_to_block_openers() {
        let stmt = vec!["let x = sig.read();", "// poly-lint: allow render-time-read — nope"];
        assert!(!has_inline_allow_near(&stmt, 0, "render-time-read"));

        let block = vec!["use_effect(move || {", "// poly-lint: allow render-time-read — yes"];
        assert!(has_inline_allow_near(&block, 0, "render-time-read"));
    }

    #[test]
    fn window_does_not_run_past_the_end_of_file() {
        let lines = vec!["use_effect(move || {"];
        assert!(!has_inline_allow_near(&lines, 0, "stale-effect-capture"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_collapses_indentation_and_inner_runs() {
        assert_eq!(normalise_line("    let n =  sig.read();  "), "let n = sig.read();");
        assert_eq!(normalise_line("\tlet n\t= sig.read();"), "let n = sig.read();");
    }

    /// The whole point of the key: re-indentation must not invalidate it.
    #[test]
    fn fingerprint_is_indentation_invariant() {
        assert_eq!(
            fingerprint("let n = sig.read();"),
            fingerprint("            let n = sig.read();")
        );
    }

    #[test]
    fn fingerprint_differs_for_different_content() {
        assert_ne!(fingerprint("let a = x.read();"), fingerprint("let a = y.read();"));
    }

    #[test]
    fn fingerprint_shape_is_never_a_line_number() {
        let fp = fingerprint("let n = sig.read();");
        assert!(is_fingerprint(&fp), "{fp} must be recognised as a fingerprint");
        assert!(fp.parse::<u32>().is_err(), "a fingerprint must not parse as a line number");
        assert_eq!(fp.len(), FINGERPRINT_LEN);
    }

    /// A hash that happened to be all decimal digits with leading zeros WOULD
    /// parse as a `u32` — the `h` prefix is what rules that out.
    #[test]
    fn all_digit_hash_still_not_a_line_number() {
        assert!(is_fingerprint("h0000000000000123"));
        assert!("h0000000000000123".parse::<u32>().is_err());
    }

    #[test]
    fn line_keys_number_identical_lines_by_ordinal() {
        let src = "let a = x.read();\nother();\nlet a = x.read();\n";
        let keys = line_keys(src);
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0].0, keys[2].0, "identical lines share a fingerprint");
        assert_eq!(keys[0].1, 0);
        assert_eq!(keys[2].1, 1, "the second occurrence gets ordinal 1");
    }

    #[test]
    fn key_at_is_one_based_and_bounded() {
        let src = "a\nb\n";
        assert_eq!(key_at(src, 1).map(|k| k.1), Some(0));
        assert!(key_at(src, 0).is_none(), "line 0 does not exist");
        assert!(key_at(src, 3).is_none(), "past the end has no key");
    }

    #[test]
    fn parses_content_entry() {
        let fp = fingerprint("let n = sig.read();");
        let raw = render_content_key("crates/core/src/ui/foo.rs", 2, &fp);
        match parse_entry(&raw) {
            Some(AllowlistEntry::PathContent { path, ordinal, fingerprint: got }) => {
                assert_eq!(path, "crates/core/src/ui/foo.rs");
                assert_eq!(ordinal, 2);
                assert_eq!(got, fp);
            }
            other => panic!("expected a content entry, got {other:?}"),
        }
    }

    /// Legacy formats must keep parsing — the other eight allowlists still use them.
    #[test]
    fn still_parses_legacy_formats() {
        assert!(matches!(parse_entry("a/b.rs:42"), Some(AllowlistEntry::PathLine(_, 42))));
        assert!(matches!(parse_entry("a/b.rs:10-20"), Some(AllowlistEntry::PathRange(_, 10, 20))));
        assert!(matches!(parse_entry("a/b.rs:my_signal"), Some(AllowlistEntry::PathReceiver(..))));
        assert!(matches!(parse_entry("a/b.rs"), Some(AllowlistEntry::WholePath(_))));
    }

    #[test]
    fn content_entry_matches_only_its_own_ordinal() {
        let fp = fingerprint("let a = x.read();");
        let entries = vec![AllowlistEntry::PathContent {
            path: "a/b.rs".to_string(),
            ordinal: 1,
            fingerprint: fp.clone(),
        }];
        assert!(is_allowed_content(&entries, "a/b.rs", &fp, 1));
        assert!(
            !is_allowed_content(&entries, "a/b.rs", &fp, 0),
            "a duplicate line elsewhere in the file is a DIFFERENT site"
        );
        assert!(!is_allowed_content(&entries, "other.rs", &fp, 1));
    }

    /// A content entry must never leak into the line-keyed matcher: that would
    /// re-introduce exactly the path-wide over-suppression it replaces.
    #[test]
    fn content_entry_does_not_match_by_line() {
        let entries = vec![AllowlistEntry::PathContent {
            path: "a/b.rs".to_string(),
            ordinal: 0,
            fingerprint: fingerprint("x"),
        }];
        assert!(!is_allowed(&entries, "a/b.rs", 1));
        assert!(!is_allowed_with_receiver(&entries, "a/b.rs", 1, "sig"));
    }

    #[test]
    fn load_with_lines_reports_the_allowlist_file_line() {
        let dir = std::env::temp_dir().join("poly-lint-gate-allowlist-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("al.txt");
        std::fs::write(&file, "# header\n\na/b.rs:12 # reason\n").unwrap();
        let loaded = load_with_lines(&file);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, 3, "entry sits on line 3 of the allowlist file");
        std::fs::remove_file(&file).unwrap();
    }
}
