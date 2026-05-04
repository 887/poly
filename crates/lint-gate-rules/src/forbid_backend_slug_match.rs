//! Scan for backend-slug `match` ladders in the UI crate — plan-client-ui-surface.md §7 WP 7.
//!
//! Per D7 / D10 / D17 of `plan-client-ui-surface.md`, backend-specific UI
//! behaviour is declared by the *plugin* (context menu items, settings
//! sections, sidebar declarations, icons, etc.) rather than selected in the
//! host by matching on a backend slug. A fresh `match bt.as_str() { "discord"
//! => ..., "matrix" => ..., _ => ... }` in `crates/core/src/ui/` is the exact
//! anti-pattern this plan exists to eliminate.
//!
//! This scan fires when a line under `crates/core/src/ui/` contains
//! `match <expr>.as_str()` AND any of the next ~30 lines contains a
//! known backend-slug string literal (`"discord"`, `"stoat"`, etc.). Matches
//! on other string content (e.g. user-entered commands) are not flagged
//! because they won't reference backend slugs.
//!
//! Matching on strongly-typed enums (e.g. `match ch.channel_type {
//! ChannelType::Forum => ... }`) is explicitly allowed — that's a typed
//! discriminator, not a slug ladder.
//!
//! Scope: `*.rs` files under `crates/core/src/ui/`. Everything else — tests,
//! plugin clients (`clients/*`), MCPs, `crates/core/src/state/*` — is out
//! of scope because those layers legitimately bridge the slug/plugin
//! boundary and are governed by separate plan items.

use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

/// Backend slugs that trigger the scan when they appear as string
/// literals below an `.as_str()` match in a UI file.
const BACKEND_SLUG_LITERALS: &[&str] = &[
    "\"discord\"",
    "\"stoat\"",
    "\"matrix\"",
    "\"teams\"",
    "\"demo\"",
    "\"poly\"",
    "\"lemmy\"",
    "\"hackernews\"",
    "\"github\"",
    "\"forgejo\"",
];

/// How many lines after the `match … .as_str()` the backend-slug literal
/// must appear in to be considered part of the same ladder.
const LOOKAHEAD_LINES: usize = 30;

pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    for path in &walker.files {
        let s = path.to_string_lossy();
        // Scope: only files under crates/core/src/ui/.
        if !s.contains("crates/core/src/ui/") {
            continue;
        }
        // Never lint test files — test fixtures intentionally construct
        // slug-matching samples to validate other scanners.
        if s.contains("/tests/") {
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

/// Per-file scan — public so `src/lib.rs` unit tests can call it directly.
pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
    let mut out = Vec::new();

    // Out-of-scope paths: this scanner only applies to `crates/core/src/ui/`.
    // Tests pass synthetic paths, so honour the path prefix check here too.
    if !path.contains("crates/core/src/ui/") {
        return out;
    }

    let lines: Vec<&str> = src.lines().collect();
    for (idx, raw) in lines.iter().enumerate() {
        if !is_match_as_str_line(raw) {
            continue;
        }
        let start = idx + 1;
        let end = (start + LOOKAHEAD_LINES).min(lines.len());
        let follows_slug = lines
            .get(start..end)
            .unwrap_or(&[])
            .iter()
            .any(|l| BACKEND_SLUG_LITERALS.iter().any(|slug| l.contains(slug)));
        if !follows_slug {
            continue;
        }
        out.push(Violation {
            rule: "forbid_backend_slug_match_in_ui".to_string(),
            path: path.to_string(),
            line: (idx as u32) + 1,
            detail:
                "slug-match found in UI — use plugin-declared items \
                 (see plan-client-ui-surface.md §4)"
                    .to_string(),
        });
    }

    out
}

/// Returns `true` iff the trimmed line contains a `match <expr>.as_str()`
/// followed by `{`. Comments and string-literal occurrences inside other
/// constructs are intentionally ignored: a `match .. as_str()` on its own
/// line is the structural shape this scan targets.
pub fn is_match_as_str_line(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("//") {
        return false;
    }
    if !t.contains("match ") {
        return false;
    }
    if !t.contains(".as_str()") {
        return false;
    }
    // Guard against `match_indices` / `match_str` false positives.
    // A real `match` expression begins with the `match` keyword.
    let Some((_, after)) = t.split_once("match ") else {
        return false;
    };
    let Some(as_str_pos) = after.find(".as_str()") else {
        return false;
    };
    // Between `match ` and `.as_str()` must be the matched expression —
    // no `{` (which would signal a different construct), no `;`.
    let expr = &after[..as_str_pos];
    if expr.contains('{') || expr.contains(';') {
        return false;
    }
    true
}
