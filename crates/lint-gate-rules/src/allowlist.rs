//! Shared allowlist loader for all `forbid-*.sh`-style scanners.
//!
//! Each bash script reimplemented the same allowlist-loading logic. This
//! module extracts that logic once, handling:
//!  - `# comment` lines and blank lines (stripped)
//!  - Whole-file entries: just the repo-relative path
//!  - Line entries: `path:line`
//!  - Range entries: `path:start-end`
//!  - Receiver entries (for signal-write): `path:receiver`
//!
//! Inline allowlist: the `// poly-lint: allow <name> — <reason>` comment
//! syntax is checked by individual scanners at hit-detection time.

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
}

/// Load an allowlist file and parse its entries.
///
/// Format per line (after stripping `#` comments and blank lines):
///   `path`                   → [`AllowlistEntry::WholePath`]
///   `path:42`                → [`AllowlistEntry::PathLine`]
///   `path:10-20`             → [`AllowlistEntry::PathRange`]
///   `path:receiver_name`     → [`AllowlistEntry::PathReceiver`] (if non-numeric)
pub fn load(path: &Path) -> Vec<AllowlistEntry> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for raw_line in content.lines() {
        // Strip inline comments.
        let line = match raw_line.split_once('#') {
            Some((before, _)) => before.trim(),
            None => raw_line.trim(),
        };
        if line.is_empty() {
            continue;
        }
        // Try to parse `path:suffix`.
        if let Some(colon_pos) = line.rfind(':') {
            let path_part = &line[..colon_pos];
            let suffix = &line[colon_pos + 1..];
            // Is suffix a plain integer (line number)?
            if let Ok(n) = suffix.parse::<u32>() {
                entries.push(AllowlistEntry::PathLine(path_part.to_string(), n));
                continue;
            }
            // Is suffix a range `start-end`?
            if let Some((lo_str, hi_str)) = suffix.split_once('-') {
                if let (Ok(lo), Ok(hi)) = (lo_str.parse::<u32>(), hi_str.parse::<u32>()) {
                    entries.push(AllowlistEntry::PathRange(path_part.to_string(), lo, hi));
                    continue;
                }
            }
            // Otherwise treat as a receiver/name suffix.
            entries.push(AllowlistEntry::PathReceiver(
                path_part.to_string(),
                suffix.to_string(),
            ));
            continue;
        }
        // No colon — whole-file allow.
        entries.push(AllowlistEntry::WholePath(line.to_string()));
    }
    entries
}

/// Check whether a hit at `(rel_path, line)` is covered by any allowlist entry.
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
        }
    }
    false
}

/// Check whether a hit at `(rel_path, line, receiver)` is covered by any allowlist entry.
/// Used by `forbid_signal_write` which has the three-way allowlist.
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
        }
    }
    false
}

/// Checks if a source line contains an inline allowlist comment for the given lint name.
///
/// Pattern: `// poly-lint: allow <name>` (anywhere on the line).
pub fn has_inline_allow(line: &str, lint_name: &str) -> bool {
    // Allow both `—` (em dash) and `-` separators after the name.
    let needle = format!("poly-lint: allow {lint_name}");
    line.contains(&needle)
}
