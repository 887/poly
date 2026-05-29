//! Forbid `use_effect` bodies that read and write the same signal — hang class #8.
//!
//! Ported from `tools/scripts/forbid-effect-self-write.sh` (Phase 5 lint for
//! CLAUDE.md hang class #8).
//!
//! Scans `crates/core/src/ui/**/*.rs` for `use_effect(move ||` bodies where
//! the same identifier is both READ (via `X.read(`) and WRITTEN (via `X.set(`
//! or `X.batch(`) without using the `_if_changed` variants.
//!
//! Allowlist file: `tools/scripts/effect-self-write-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow effect-self-write — <reason>` anywhere in body.

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_effect_self_write";
const ALLOWLIST_FILE: &str = "tools/scripts/effect-self-write-allowlist.txt";

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

/// Result of scanning one `use_effect` body.
struct EffectScan {
    /// 1-based line number where the effect body ends (the closing `}`).
    end_line: usize,
    read_set: std::collections::HashSet<String>,
    /// (ident, method) pairs — method is "set" or "batch".
    write_set: std::collections::HashSet<(String, String)>,
    has_inline_allow: bool,
}

/// Scan one `use_effect` body starting at line index `start` (0-based).
/// Returns `None` if the body never closes (malformed / unterminated).
/// Returns `Some(EffectScan)` with the 0-based index of the closing brace line.
fn scan_effect_body(lines: &[&str], start: usize) -> Option<EffectScan> {
    let mut depth: i32 = 0;
    let mut read_set = std::collections::HashSet::new();
    let mut write_set = std::collections::HashSet::new();
    let mut has_inline_allow = false;

    for (offset, &l) in lines[start..].iter().enumerate() {
        let j = start + offset;

        // Strip line comments for brace counting and pattern matching.
        let stripped = match l.find("//") {
            Some(pos) => &l[..pos],
            None => l,
        };

        // Check inline allowlist on the original line (comment text).
        if allowlist::has_inline_allow(l, "effect-self-write") {
            has_inline_allow = true;
        }

        // Count reads: `ident.read(`
        let mut search = stripped;
        while let Some(pos) = search.find(".read(") {
            let before = &search[..pos];
            if let Some(ident) = extract_last_ident(before)
                && ident != "self" {
                    read_set.insert(ident);
                }
            search = &search[pos + 1..];
        }

        // Count writes: `ident.set(` or `ident.batch(` — but NOT `_if_changed`.
        let mut search2 = stripped;
        while let Some(dot_pos) = find_write_call(search2) {
            let before = &search2[..dot_pos];
            let after = &search2[dot_pos + 1..];
            let method = if after.starts_with("set(") {
                Some("set")
            } else if after.starts_with("batch(") {
                Some("batch")
            } else {
                None
            };
            if let Some(method) = method
                && let Some(ident) = extract_last_ident(before)
                    && ident != "self" {
                        write_set.insert((ident, method.to_string()));
                    }
            search2 = &search2[dot_pos + 1..];
        }

        // Update brace depth AFTER pattern scanning so we can detect body-end
        // after processing this line's reads/writes.
        for ch in stripped.chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth <= 0 && j > start {
                        return Some(EffectScan {
                            end_line: j,
                            read_set,
                            write_set,
                            has_inline_allow,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    // Never found the closing brace — treat as no effect.
    None
}

fn scan_file_content(
    content: &str,
    rel: &str,
    allowlist_entries: &[allowlist::AllowlistEntry],
    violations: &mut Vec<Violation>,
) {
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        // Look for `use_effect(move ||` start.
        if !line.contains("use_effect(move") || !line.contains("||") {
            i += 1;
            continue;
        }
        let effect_start_line = (i as u32) + 1; // 1-based for violation reporting

        match scan_effect_body(&lines, i) {
            None => {
                // Malformed / unterminated — skip one line and keep going.
                i += 1;
            }
            Some(scan) => {
                // Advance outer loop PAST this effect body before checking violations,
                // so we don't re-enter the same effect body on the next iteration.
                i = scan.end_line + 1;

                if scan.has_inline_allow {
                    continue;
                }
                if allowlist::is_allowed(allowlist_entries, rel, effect_start_line) {
                    continue;
                }

                // Report at most one violation per effect body (the first overlapping ident).
                for (wr_ident, wr_method) in &scan.write_set {
                    if scan.read_set.contains(wr_ident.as_str()) {
                        violations.push(Violation {
                            rule: RULE.to_string(),
                            path: rel.to_string(),
                            line: effect_start_line,
                            detail: format!(
                                "use_effect body reads AND writes `{wr_ident}` \
                                 via `.{wr_method}()` without `_if_changed` — \
                                 CLAUDE.md hang class #8. Use \
                                 BatchedSignal::set_if_changed() or \
                                 batch_if_changed(). See: \
                                 docs/plans/plan-batched-signal.md"
                            ),
                        });
                        break; // one violation per effect
                    }
                }
            }
        }
    }
}

/// Find the position of a `.set(` or `.batch(` call (not `_if_changed`) in `s`.
fn find_write_call(s: &str) -> Option<usize> {
    let patterns = [".set(", ".batch("];
    let mut earliest: Option<usize> = None;
    for pat in &patterns {
        let mut search_from = 0;
        while let Some(pos) = s[search_from..].find(pat) {
            let abs_pos = search_from + pos;
            let suffix_after = &s[abs_pos + 1..]; // after the `.`
            let is_if_changed = suffix_after.starts_with("set_if_changed(")
                || suffix_after.starts_with("batch_if_changed(");
            if !is_if_changed {
                if earliest.is_none_or(|e| abs_pos < e) {
                    earliest = Some(abs_pos);
                }
                break;
            }
            search_from = abs_pos + 1;
        }
    }
    earliest
}

/// Extract the last identifier (word chars + underscore) before a given string position.
fn extract_last_ident(before: &str) -> Option<String> {
    let trimmed = before.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map_or(0, |i| i + 1);
    let name = &trimmed[start..];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_self_write_violation() {
        let src = r#"
fn foo() {
    use_effect(move || {
        let _v = my_signal.read();
        my_signal.set(42);
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(!violations.is_empty(), "should detect self-write");
    }

    #[test]
    fn allows_if_changed_variant() {
        let src = r#"
fn foo() {
    use_effect(move || {
        let _v = my_signal.read();
        my_signal.set_if_changed(42);
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "set_if_changed should not be flagged");
    }

    #[test]
    fn inline_allow_suppresses() {
        let src = r#"
fn foo() {
    use_effect(move || {
        let _v = my_signal.read();
        my_signal.set(42); // poly-lint: allow effect-self-write — converging state machine
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "inline allow should suppress");
    }

    #[test]
    fn different_idents_no_violation() {
        let src = r#"
fn foo() {
    use_effect(move || {
        let _v = signal_a.read();
        signal_b.set(42);
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(
            violations.is_empty(),
            "different idents should not be flagged"
        );
    }

    #[test]
    fn two_effects_only_one_flagged() {
        let src = r#"
fn foo() {
    use_effect(move || {
        let _v = sig_a.read();
        sig_b.set(1); // no overlap
    });
    use_effect(move || {
        let _v = sig_x.read();
        sig_x.batch(|v| *v = 99); // self-write
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert_eq!(violations.len(), 1, "only the second effect should fire");
        assert_eq!(violations[0].line, 7, "violation on line 7 (second use_effect)");
    }
}
