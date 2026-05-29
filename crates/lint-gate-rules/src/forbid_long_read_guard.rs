//! Forbid long-scoped Signal::read() guards — hang class #2.
//!
//! Ported from `tools/scripts/forbid-long-read-guard.sh` (Phase 5 Track A of
//! plan-read-guard-scoping.md).
//!
//! Scans `crates/core/src/ui/**/*.rs` for bare `let <var> = <sig>.read();`
//! bindings where the same signal is written (`.batch(`, `.write(`, `.set(`,
//! `.pending_update(`) within 60 lines while the guard is still live.
//!
//! Allowlist file: `tools/scripts/long-read-guard-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow long-read-guard — <reason>` on the `let` line.

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_long_read_guard";
const ALLOWLIST_FILE: &str = "tools/scripts/long-read-guard-allowlist.txt";
const LOOK_AHEAD: usize = 60;

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

        scan_file_content(&content, &rel, &allowlist_entries, violations);
    }
}

fn scan_file_content(
    content: &str,
    rel: &str,
    allowlist_entries: &[allowlist::AllowlistEntry],
    violations: &mut Vec<Violation>,
) {
    let lines: Vec<&str> = content.lines().collect();

    for (i, raw) in lines.iter().enumerate() {
        let trimmed = raw.trim_start();

        // Must be a bare `let [mut] <var> = <sig>.read();` pattern.
        if !trimmed.starts_with("let ") {
            continue;
        }
        if !trimmed.contains(".read();") {
            continue;
        }
        // Exclude chained forms like `.read().foo` — look for `.read();` at end
        // of the significant portion.
        // The bash script heuristic: the signal expr ends with `.read();` with no trailing chain.
        if !is_bare_read_let(trimmed) {
            continue;
        }

        // Inline allowlist on the let line.
        if allowlist::has_inline_allow(raw, "long-read-guard") {
            continue;
        }

        let line_no = (i as u32) + 1;
        if allowlist::is_allowed(allowlist_entries, rel, line_no) {
            continue;
        }

        // Extract variable name and signal name.
        let Some((var_name, sig_name)) = extract_var_and_sig(trimmed) else { continue };

        let lookahead_lines = {
            let start = i + 1;
            let end = (i + LOOK_AHEAD + 1).min(lines.len());
            &lines[start..end]
        };
        let hit = guard_written_before_drop(&var_name, &sig_name, lookahead_lines);

        if hit {
            violations.push(Violation {
                rule: RULE.to_string(),
                path: rel.to_string(),
                line: line_no,
                detail: format!(
                    "long-scoped Signal::read() guard `{var_name}` on `{sig_name}` is live \
                     when `{sig_name}` is written — CLAUDE.md hang class #2. Use \
                     BatchedSignal::with(|v| ...) or wrap in a tightly-scoped block. \
                     Inline-allowlist: // poly-lint: allow long-read-guard — <reason>"
                ),
            });
        }
    }
}

/// Returns `true` if `sig_name` is written while the read guard is live
/// (i.e. before a `drop(var_name)` or an enclosing `}` that closes the scope).
fn guard_written_before_drop(var_name: &str, sig_name: &str, lines: &[&str]) -> bool {
    let mut depth: i32 = 0;
    for &line_j in lines {
        // Explicit drop ends scope cleanly.
        if line_j.contains(&format!("drop({var_name})"))
            || line_j.contains(&format!("drop( {var_name})"))
        {
            return false;
        }

        // Write on the same signal while guard is live.
        if depth >= 0 {
            for write_method in &[".batch(", ".write(", ".set(", ".pending_update("] {
                if line_j.contains(&format!("{sig_name}{write_method}")) {
                    return true;
                }
            }
        }

        // Count braces (strip comments first).
        let stripped = strip_comment(line_j);
        for ch in stripped.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth < 0 {
                        // Enclosing block closed — guard dropped.
                        return false;
                    }
                }
                _ => {}
            }
        }
        if depth < 0 {
            return false;
        }
    }
    false
}

/// Returns true if the trimmed line is a bare `let [mut] X = Y.read();` —
/// no trailing method chain after `.read()`.
fn is_bare_read_let(trimmed: &str) -> bool {
    // Pattern: starts with `let`, contains `.read();`, and the `.read();` is the end
    // of the significant part (no more method chaining).
    // The trimmed line may have trailing whitespace or a comment; we strip those.
    let without_comment = trimmed
        .find("//")
        .map_or_else(|| trimmed.trim_end(), |pos| trimmed[..pos].trim_end());
    // Must end with `.read();`
    without_comment.ends_with(".read();")
        && without_comment.contains("= ")
        // Exclude `let _ =` assignments (not actually used).
        && !without_comment.contains("let _ =")
}

/// Extract `(var_name, sig_name)` from `let [mut] var = sig.read();`.
fn extract_var_and_sig(trimmed: &str) -> Option<(String, String)> {
    // Strip `let ` or `let mut `
    let rest = trimmed.strip_prefix("let ")?;
    let rest = rest.strip_prefix("mut ").unwrap_or(rest);
    // Split at ` = `
    let (var_part, rhs) = rest.split_once(" = ")?;
    let var_name = var_part.trim().to_string();
    // rhs ends with `.read();` — strip that suffix.
    let sig_part = rhs.strip_suffix(".read();")?;
    // Handle chained access like `foo.bar.read()` — take the whole chain as sig_name.
    let sig_name = sig_part.trim().to_string();
    Some((var_name, sig_name))
}

/// Strip `//` line comment from a line.
fn strip_comment(line: &str) -> &str {
    line.find("//").map_or(line, |pos| &line[..pos])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_long_guard_hit() {
        let src = r#"
fn foo() {
    let guard = my_signal.read();
    // some logic
    my_signal.batch(|v| { *v = 1; });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(!violations.is_empty(), "should detect live guard across write");
    }

    #[test]
    fn allows_drop_before_write() {
        let src = r#"
fn foo() {
    let guard = my_signal.read();
    drop(guard);
    my_signal.batch(|v| { *v = 1; });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "explicit drop before write is safe");
    }

    #[test]
    fn allows_block_scoped_read() {
        let src = r#"
fn foo() {
    {
        let guard = my_signal.read();
        // use guard
    }
    my_signal.batch(|v| { *v = 1; });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "block-scoped read is safe");
    }

    #[test]
    fn inline_allow_suppresses() {
        let src = r#"
fn foo() {
    let guard = my_signal.read(); // poly-lint: allow long-read-guard — legacy code
    my_signal.batch(|v| { *v = 1; });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "inline allow suppresses");
    }
}
