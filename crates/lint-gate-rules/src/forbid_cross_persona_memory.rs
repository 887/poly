//! Forbid cross-persona memory access — persona class P1.
//!
//! Ported from `tools/scripts/forbid-cross-persona-memory.sh` (Phase Q.1 of
//! plan-persona-quality-gates.md).
//!
//! Scans `mcp/chat-mcp/src/` for SELECT/DELETE/UPDATE queries against
//! persona-scoped tables (persona_facts, persona_audit, persona_sources,
//! persona_tool_whitelist, persona_outbound_allowlist) that do NOT include
//! a `persona_slug` binding within 10 lines of the query.
//!
//! Allowlist file: `tools/scripts/cross-persona-memory-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow cross-persona-memory — <reason>`

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "mcp/chat-mcp/src";
const RULE: &str = "forbid_cross_persona_memory";
const ALLOWLIST_FILE: &str = "tools/scripts/cross-persona-memory-allowlist.txt";
const SLUG_WINDOW: usize = 10;

const PERSONA_TABLES: &[&str] = &[
    "persona_facts",
    "persona_audit",
    "persona_sources",
    "persona_tool_whitelist",
    "persona_outbound_allowlist",
];

const DML_VERBS: &[&str] = &["SELECT", "DELETE", "UPDATE"];

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

        let lines: Vec<&str> = content.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let line_no = (idx as u32) + 1;

            // Skip inline-allowlisted lines.
            if allowlist::has_inline_allow(line, "cross-persona-memory") {
                continue;
            }

            // Check if line contains a DML verb targeting a persona table.
            let has_dml = DML_VERBS.iter().any(|verb| line.contains(verb));
            if !has_dml {
                continue;
            }
            let has_persona_table = PERSONA_TABLES.iter().any(|tbl| line.contains(tbl));
            if !has_persona_table {
                continue;
            }

            // Look for `persona_slug` in the next SLUG_WINDOW lines (inclusive).
            let window_end = (idx + SLUG_WINDOW + 1).min(lines.len());
            let found_slug = lines[idx..window_end].iter().any(|l| {
                allowlist::has_inline_allow(l, "cross-persona-memory") || l.contains("persona_slug")
            });

            if found_slug {
                continue;
            }

            if allowlist::is_allowed(&allowlist_entries, &rel, line_no) {
                continue;
            }

            violations.push(Violation {
                rule: RULE.to_string(),
                path: rel.clone(),
                line: line_no,
                detail: format!(
                    "cross-persona memory access: DML on persona-scoped table without \
                     persona_slug binding in next {SLUG_WINDOW} lines. Add \
                     `WHERE persona_slug = ?` or annotate with \
                     `// poly-lint: allow cross-persona-memory — <reason>`. \
                     See: docs/plans/plan-persona-quality-gates.md Phase Q.1."
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_violations(src: &str) -> Vec<String> {
        let lines: Vec<&str> = src.lines().collect();
        let mut out = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let line_no = (idx as u32) + 1;
            if allowlist::has_inline_allow(line, "cross-persona-memory") {
                continue;
            }
            let has_dml = DML_VERBS.iter().any(|v| line.contains(v));
            if !has_dml {
                continue;
            }
            let has_tbl = PERSONA_TABLES.iter().any(|t| line.contains(t));
            if !has_tbl {
                continue;
            }
            let window_end = (idx + SLUG_WINDOW + 1).min(lines.len());
            let found_slug = lines[idx..window_end].iter().any(|l| l.contains("persona_slug"));
            if !found_slug {
                out.push(format!("{}:{}", "test.rs", line_no));
            }
        }
        out
    }

    #[test]
    fn flags_select_without_persona_slug() {
        let src = "    let r = db.query(\"SELECT * FROM persona_facts WHERE id = ?\", (id,));";
        let v = make_violations(src);
        assert!(!v.is_empty(), "should flag DML on persona table without persona_slug");
    }

    #[test]
    fn allows_select_with_persona_slug_nearby() {
        let src = "    let r = db.query(\"SELECT * FROM persona_facts WHERE persona_slug = ?\", (slug,));";
        let v = make_violations(src);
        assert!(v.is_empty(), "persona_slug on same line should satisfy the check");
    }

    #[test]
    fn inline_allowlist_suppresses() {
        let src = "    db.query(\"DELETE FROM persona_facts\"); // poly-lint: allow cross-persona-memory — bulk prune";
        let v = make_violations(src);
        assert!(v.is_empty(), "inline allowlist should suppress");
    }
}
