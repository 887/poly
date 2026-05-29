//! FTL label-key coverage scanner — plan-client-ui-surface.md D21.
//!
//! Scans plugin source files (`clients/*/src/**/*.rs`) for `label_key: "..."` or
//! `label-key: "..."` string-literal values. Each discovered key is cross-referenced
//! against the plugin's English FTL bundle (`clients/<name>/locales/en/*.ftl`).
//! A key declared in source but absent from the bundle is a violation.
//!
//! At WP 1 no plugin declarations exist yet, so this scanner always finds zero
//! violations in the current repo. The full scan logic is implemented now so
//! it works automatically when items are declared in WP 2–6.

use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    let ws_root = &walker.root;

    // Discover plugin directories: any subdirectory of `clients/` that has a `src/` dir.
    let clients_dir = ws_root.join("clients");
    let plugin_dirs: Vec<std::path::PathBuf> = match std::fs::read_dir(&clients_dir) {
        Ok(rd) => rd
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .filter(|p| p.join("src").is_dir())
            .collect(),
        Err(_) => return,
    };

    for plugin_dir in plugin_dirs {
        let plugin_name = plugin_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        // Collect FTL keys from the English bundle for this plugin.
        let ftl_keys = collect_ftl_keys(&plugin_dir);

        // Scan all .rs source files under this plugin's src/.
        let src_dir = plugin_dir.join("src");
        let src_files = collect_rs_files(&src_dir);

        for src_path in src_files {
            let Ok(content) = std::fs::read_to_string(&src_path) else {
                continue;
            };
            let rel = src_path
                .strip_prefix(ws_root)
                .unwrap_or(&src_path)
                .to_string_lossy()
                .into_owned();

            let mut vs = scan_src(&content, &rel, &ftl_keys, &plugin_name);
            violations.append(&mut vs);
        }
    }
}

/// Per-file scan — public so `src/lib.rs` unit tests can call it directly.
///
/// `ftl_keys`: the set of FTL message identifiers found in the plugin's English bundle.
/// `plugin_name`: used only for the violation detail message.
#[must_use]
pub fn scan_src<S: ::std::hash::BuildHasher>(
    src: &str,
    path: &str,
    ftl_keys: &std::collections::HashSet<String, S>,
    plugin_name: &str,
) -> Vec<Violation> {
    let mut out = Vec::new();

    for (line_idx, line) in src.lines().enumerate() {
        let trimmed = line.trim();

        // Quick pre-filter: must contain `label_key` or `label-key`.
        if !trimmed.contains("label_key") && !trimmed.contains("label-key") {
            continue;
        }

        // Extract string literal after `label_key:` or `label-key:`.
        // Pattern: `label[_-]key\s*:\s*"<key>"`
        if let Some(key) = extract_label_key(trimmed) {
            // Empty key strings are a different kind of error; skip them here.
            if key.is_empty() {
                continue;
            }

            if !ftl_keys.contains(&key) {
                let ftl_hint = format!("clients/{plugin_name}/locales/en/*.ftl");
                out.push(Violation {
                    rule: "ftl_label_key_coverage".into(),
                    path: path.to_string(),
                    line: (line_idx as u32) + 1,
                    detail: format!(
                        "FTL key '{key}' declared but missing from bundle; \
                         expected in {ftl_hint}; file: {path}:{}",
                        line_idx + 1
                    ),
                });
            }
        }
    }

    out
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the string literal value after `label_key:` or `label-key:`.
/// Returns `None` if no such pattern is found on the line.
#[must_use] 
pub fn extract_label_key(line: &str) -> Option<String> {
    // Try both `label_key` and `label-key` prefixes.
    for prefix in &["label_key", "label-key"] {
        let Some(pos) = line.find(prefix) else { continue };
        let after = &line[pos + prefix.len()..];
        let after = after.trim_start();
        let after = after.strip_prefix(':')?.trim_start();
        // Must be a double-quoted string literal.
        let after = after.strip_prefix('"')?;
        let end = after.find('"')?;
        return Some(after[..end].to_string());
    }
    None
}

/// Collect all FTL message identifiers from `clients/<plugin>/locales/en/*.ftl`.
///
/// FTL message identifier lines look like:
///   `my-key = value`
/// (identifier at column 0, followed by ` =`).
/// Attribute lines start with `.` — those are sub-keys, not top-level ids, skip them.
/// Term lines start with `-` — also skip (terms aren't used as label keys in this plan).
#[must_use] 
pub fn collect_ftl_keys(plugin_dir: &std::path::Path) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();
    let en_dir = plugin_dir.join("locales").join("en");
    let Ok(rd) = std::fs::read_dir(&en_dir) else {
        return keys;
    };

    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("ftl") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&p) else {
            continue;
        };
        for line in content.lines() {
            // Message identifier: starts at column 0 with an ASCII alphanumeric or hyphen,
            // not with `.` (attribute) or `-` (term) or `#` (comment) or whitespace.
            let first = line.chars().next().unwrap_or(' ');
            if !first.is_ascii_alphanumeric() {
                continue;
            }
            // Must contain ` =` to be a message definition.
            if let Some(eq_pos) = line.find(" =") {
                let id = &line[..eq_pos];
                // Validate that the identifier only contains alphanumerics and hyphens/underscores.
                if id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    keys.insert(id.to_string());
                }
            }
        }
    }

    keys
}

/// Recursively collect all `.rs` files under a directory.
fn collect_rs_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(collect_rs_files(&p));
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p);
        }
    }
    out
}
