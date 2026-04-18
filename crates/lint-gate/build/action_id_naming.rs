//! Action ID naming scanner — plan-client-ui-surface.md D25.
//!
//! Scans plugin source files (`clients/*/src/**/*.rs`) for `id: "..."` string
//! literals inside declarations of `MenuItem { ... }`, `ComposerButton { ... }`,
//! or `SidebarItem { ... }`. Each found ID is checked against the kebab-case
//! convention: `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
//!
//! Non-matching IDs are violations with rule `"action_id_naming"`.
//!
//! At WP 1 no plugin declarations exist yet, so this scanner always finds zero
//! violations in the current repo. The full scan logic is implemented now so
//! it works automatically when items are declared in WP 2–6.

use crate::baseline::Violation;
use crate::walk::WorkspaceWalker;

/// Declaration block openers whose `id:` fields we check.
const DECL_KEYWORDS: &[&str] = &["MenuItem", "ComposerButton", "SidebarItem"];

pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    for path in &walker.files {
        let s = path.to_string_lossy();
        // Only scan client plugin source dirs.
        if !s.contains("clients/") {
            continue;
        }
        // Skip tests, examples, build artefacts.
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

/// Per-file scan — public so `src/lib.rs` unit tests can call it directly.
pub fn scan_src(src: &str, path: &str) -> Vec<Violation> {
    let mut out = Vec::new();

    // State machine: track whether we're inside a declaration block.
    // We use brace depth to know when a declaration block closes.
    let lines: Vec<&str> = src.lines().collect();
    let mut in_decl_depth: Option<i32> = None; // Some(brace_depth_at_open)
    let mut brace_depth: i32 = 0;

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Update brace depth for the whole line before any other logic.
        // We do it character-by-character below when needed, but for state
        // tracking we need the net change first.
        let line_delta: i32 = line
            .chars()
            .map(|c| match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            })
            .sum();

        // Check if this line opens a declaration block.
        let opens_decl = DECL_KEYWORDS
            .iter()
            .any(|kw| trimmed.contains(kw) && trimmed.contains('{'));

        // Check whether we're currently inside a declaration (or enter one now).
        let was_in_decl = in_decl_depth.is_some();

        if opens_decl && in_decl_depth.is_none() {
            // Enter declaration scope at the current brace depth + 1
            // (the `{` on this line increments depth).
            in_decl_depth = Some(brace_depth + 1);
        }

        let currently_in_decl = in_decl_depth.is_some() || was_in_decl;

        if currently_in_decl {
            // Scan for `id: "<value>"` on this line.
            if let Some(id) = extract_id_field(trimmed) {
                if !id.is_empty() && !is_kebab_case(&id) {
                    out.push(Violation {
                        rule: "action_id_naming".into(),
                        path: path.to_string(),
                        line: (line_idx as u32) + 1,
                        detail: format!(
                            "action ID '{id}' is not kebab-case — use lowercase letters, digits, \
                             and hyphens only, starting with a letter (e.g. 'invite-user'); \
                             file: {path}:{}",
                            line_idx + 1
                        ),
                    });
                }
            }
        }

        // Update brace depth after processing the line.
        brace_depth += line_delta;

        // Exit declaration scope if brace depth drops back to the entry depth - 1.
        if let Some(entry_depth) = in_decl_depth {
            if brace_depth < entry_depth {
                in_decl_depth = None;
            }
        }
    }

    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the string literal value after `id:` on a line.
/// Returns `None` if no such pattern is found.
pub fn extract_id_field(line: &str) -> Option<String> {
    // Look for `id:` optionally followed by whitespace then `"<value>"`.
    let pos = line.find("id:")?;
    let after = &line[pos + 3..];
    let after = after.trim_start();
    // Must be a double-quoted string literal.
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// Returns `true` iff `s` matches `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
/// This is the D25 kebab-case convention: starts with lowercase letter,
/// segments separated by single hyphens, no trailing hyphen.
pub fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars().peekable();

    // First character must be a lowercase ASCII letter.
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }

    // Remaining characters: lowercase letters, digits, or hyphens.
    // A hyphen must be followed by at least one alphanumeric (no trailing hyphen,
    // no double hyphen).
    let mut prev_was_hyphen = false;
    for c in chars {
        if c == '-' {
            if prev_was_hyphen {
                return false; // double hyphen
            }
            prev_was_hyphen = true;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_was_hyphen = false;
        } else {
            return false; // uppercase, underscore, or other
        }
    }

    // Must not end with a hyphen.
    !prev_was_hyphen
}
