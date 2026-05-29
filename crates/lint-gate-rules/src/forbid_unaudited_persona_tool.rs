//! Forbid unaudited persona tool handlers — persona class P2.
//!
//! Ported from `tools/scripts/forbid-unaudited-persona-tool.sh` (Phase Q.2 of
//! plan-persona-quality-gates.md).
//!
//! After J.1 (tools.rs split), scans two files:
//!   - `mcp/chat-mcp/src/tools/persona.rs`        — `fn handle_meta_persona_*`
//!   - `mcp/chat-mcp/src/tools/client_settings.rs` — `fn handle_client_settings_*`
//!
//! For `handle_meta_persona_*`:
//!   Acceptable: `audit(mem,` or `record_persona_audit(`
//! For `handle_client_settings_*`:
//!   Acceptable: `audit_client_settings(` or `record_client_settings_audit(`
//!
//! Allowlist files:
//!   `tools/scripts/unaudited-persona-tool-allowlist.txt`
//!   `tools/scripts/unaudited-client-settings-tool-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow unaudited-persona-tool — <reason>`

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const PERSONA_TOOLS_FILE: &str = "mcp/chat-mcp/src/tools/persona.rs";
const CLIENT_SETTINGS_TOOLS_FILE: &str = "mcp/chat-mcp/src/tools/client_settings.rs";
const RULE: &str = "forbid_unaudited_persona_tool";
const PERSONA_ALLOWLIST: &str = "tools/scripts/unaudited-persona-tool-allowlist.txt";
const CLIENT_ALLOWLIST: &str = "tools/scripts/unaudited-client-settings-tool-allowlist.txt";

pub fn scan(_walker: &WorkspaceWalker, ws_root: &Path, violations: &mut Vec<Violation>) {
    let persona_allowlist = load_suffix_allowlist(&ws_root.join(PERSONA_ALLOWLIST));
    let client_allowlist = load_suffix_allowlist(&ws_root.join(CLIENT_ALLOWLIST));

    // Scan persona.rs for handle_meta_persona_* functions.
    let persona_path = ws_root.join(PERSONA_TOOLS_FILE);
    if persona_path.exists()
        && let Ok(content) = std::fs::read_to_string(&persona_path) {
            for func in extract_functions(&content) {
                if !func.name.starts_with("handle_meta_persona_") {
                    continue;
                }
                let suffix = func.name["handle_meta_persona_".len()..].to_string();
                let allow_suffix = format!("_{suffix}");
                if persona_allowlist.contains(&allow_suffix) || persona_allowlist.contains(&suffix) {
                    continue;
                }
                if func.has_inline_allow {
                    continue;
                }
                let has_audit = func.body.contains("audit(mem,") || func.body.contains("record_persona_audit(");
                if !has_audit {
                    violations.push(Violation {
                        rule: RULE.to_string(),
                        path: PERSONA_TOOLS_FILE.to_string(),
                        line: func.start_line,
                        detail: format!(
                            "`handle_meta_persona_{suffix}` has no audit call — persona class P2. \
                             Add audit(mem, slug, \"invoke\", ...) or add the suffix to \
                             tools/scripts/unaudited-persona-tool-allowlist.txt. \
                             See: docs/plans/plan-persona-quality-gates.md Phase Q.2."
                        ),
                    });
                }
            }
        }

    // Scan client_settings.rs for handle_client_settings_* functions.
    let client_path = ws_root.join(CLIENT_SETTINGS_TOOLS_FILE);
    if client_path.exists()
        && let Ok(content) = std::fs::read_to_string(&client_path) {
            for func in extract_functions(&content) {
                if !func.name.starts_with("handle_client_settings_") {
                    continue;
                }
                let suffix = func.name["handle_client_settings_".len()..].to_string();
                let allow_suffix = format!("_{suffix}");
                if client_allowlist.contains(&allow_suffix) || client_allowlist.contains(&suffix) {
                    continue;
                }
                if func.has_inline_allow {
                    continue;
                }
                let has_audit = func.body.lines().any(|l| {
                    let trimmed = l.trim_start();
                    if trimmed.starts_with("//") {
                        return false;
                    }
                    l.contains("audit_client_settings(") || l.contains("record_client_settings_audit(")
                });
                if !has_audit {
                    violations.push(Violation {
                        rule: RULE.to_string(),
                        path: CLIENT_SETTINGS_TOOLS_FILE.to_string(),
                        line: func.start_line,
                        detail: format!(
                            "`handle_client_settings_{suffix}` has no audit call — persona class P2. \
                             Add audit_client_settings(...) or add the suffix to \
                             tools/scripts/unaudited-client-settings-tool-allowlist.txt. \
                             See: docs/plans/plan-persona-quality-gates.md Phase Q.2."
                        ),
                    });
                }
            }
        }
}

struct ExtractedFn {
    name: String,
    start_line: u32,
    body: String,
    has_inline_allow: bool,
}

/// Extract all `handle_meta_persona_*` and `handle_client_settings_*` functions
/// from the source. Returns function name, start line, and body text.
fn extract_functions(content: &str) -> Vec<ExtractedFn> {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        // Detect function definition lines.
        let fn_name = extract_fn_name(line, "handle_meta_persona_")
            .or_else(|| extract_fn_name(line, "handle_client_settings_"));

        if let Some(name) = fn_name {
            let start_line = (i as u32) + 1;
            // Collect the function body by brace matching.
            let mut body = String::new();
            let mut depth: i32 = 0;
            let mut has_inline_allow = false;
            let mut j = i;
            let mut started = false;

            while j < lines.len() {
                let l = lines[j];
                body.push_str(l);
                body.push('\n');

                if allowlist::has_inline_allow(l, "unaudited-persona-tool") {
                    has_inline_allow = true;
                }

                for ch in l.chars() {
                    match ch {
                        '{' => {
                            depth += 1;
                            started = true;
                        }
                        '}' => {
                            depth -= 1;
                            if started && depth <= 0 {
                                // Function body ended.
                                out.push(ExtractedFn {
                                    name: name.clone(),
                                    start_line,
                                    body: body.clone(),
                                    has_inline_allow,
                                });
                                i = j + 1;
                                // Break inner loop.
                                j = lines.len(); // signal break
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if j >= lines.len() {
                    break;
                }
                j += 1;
            }
            if i <= j && j < lines.len() {
                i = j + 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Extract function name if the line defines a function matching the given prefix.
fn extract_fn_name(line: &str, prefix: &str) -> Option<String> {
    // Patterns: `fn prefix_suffix(`, `async fn prefix_suffix(`, etc.
    let trimmed = line.trim_start();
    // Skip comments.
    if trimmed.starts_with("//") {
        return None;
    }
    // Look for `fn handle_meta_persona_` or `fn handle_client_settings_` anywhere on line.
    let fn_marker = format!("fn {prefix}");
    let pos = line.find(&fn_marker)?;
    let rest = &line[pos + 3..]; // skip `fn `
    let name_end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
    let name = &rest[..name_end];
    if name.starts_with(prefix) {
        Some(name.to_string())
    } else {
        None
    }
}

/// Load an allowlist of handler name suffixes.
fn load_suffix_allowlist(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let trimmed = match line.split_once('#') {
                Some((before, _)) => before.trim(),
                None => line.trim(),
            };
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_missing_audit() {
        let src = r#"
async fn handle_meta_persona_update(mem: &Mem, slug: &str) -> Result<()> {
    db.execute("UPDATE persona_facts SET name = ?", (name,))?;
    Ok(())
}
"#;
        let fns = extract_functions(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "handle_meta_persona_update");
        let has_audit = fns[0].body.contains("audit(mem,") || fns[0].body.contains("record_persona_audit(");
        assert!(!has_audit, "should not have audit");
    }

    #[test]
    fn allows_with_audit_call() {
        let src = r#"
async fn handle_meta_persona_create(mem: &Mem, slug: &str) -> Result<()> {
    db.execute("INSERT INTO persona_facts ...", ...)?;
    audit(mem, slug, "create", None, "ok", None);
    Ok(())
}
"#;
        let fns = extract_functions(src);
        assert_eq!(fns.len(), 1);
        let has_audit = fns[0].body.contains("audit(mem,");
        assert!(has_audit, "should detect audit call");
    }

    #[test]
    fn inline_allow_detected() {
        let src = r#"
async fn handle_meta_persona_list(mem: &Mem) -> Result<Vec<Persona>> {
    // poly-lint: allow unaudited-persona-tool — read-only, no state mutation
    db.query("SELECT * FROM persona_facts WHERE persona_slug = ?", (slug,))
}
"#;
        let fns = extract_functions(src);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].has_inline_allow);
    }
}
