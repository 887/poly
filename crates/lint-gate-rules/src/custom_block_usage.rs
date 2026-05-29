//! Custom-block usage scanner — plan-client-ui-polish.md Pack G P40.
//!
//! Counts `CustomBlock { ... }` literal constructions per client plugin and
//! emits a violation when a plugin exceeds the per-plugin threshold. Heavy
//! reliance on plugin-injected sanitized HTML is a smell — most surfaces
//! should expose their state via typed plugin items (menu / settings /
//! sidebar / view rows) rather than raw HTML blocks.
//!
//! Threshold: 5 literal sites per plugin. Today's max is 1; the headroom
//! gives existing plugins room to grow without hitting the gate accidentally,
//! while still catching a regression where someone replaces typed surfaces
//! with HTML blobs.

use std::collections::BTreeMap;

use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const THRESHOLD: usize = 5;

pub fn scan(walker: &WorkspaceWalker, violations: &mut Vec<Violation>) {
    let mut per_plugin: BTreeMap<String, (u32, String)> = BTreeMap::new();

    for path in &walker.files {
        let s = path.to_string_lossy();
        if !s.contains("clients/") {
            continue;
        }
        if s.contains("/tests/")
            || s.contains("/examples/")
            || s.contains("/target/")
            || s.contains("/mcp/")
        {
            continue;
        }
        let Some(plugin) = plugin_id_from_path(&s) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        let count = count_custom_block_literals(&content) as u32;
        if count == 0 {
            continue;
        }
        let rel = walker.relative(path);
        let entry = per_plugin.entry(plugin).or_insert((0, rel.clone()));
        entry.0 += count;
    }

    for (plugin, (count, sample_path)) in per_plugin {
        if (count as usize) > THRESHOLD {
            violations.push(Violation {
                rule: "custom_block_usage".into(),
                path: sample_path.clone(),
                line: 1,
                detail: format!(
                    "plugin '{plugin}' constructs {count} CustomBlock {{...}} literals \
                     (threshold: {THRESHOLD}). Prefer typed plugin surfaces (menu items, \
                     settings sections, sidebar items, view rows) over sanitized HTML blobs; \
                     reserve CustomBlock for genuinely free-form rich content."
                ),
            });
        }
    }
}

/// Per-source scan — count `CustomBlock {` literal sites in `src`.
/// Public so unit tests in `src/lib.rs` can call directly.
#[must_use] 
pub fn count_custom_block_literals(src: &str) -> usize {
    // Match `CustomBlock` followed by optional whitespace then `{`.
    // Must NOT be preceded by an identifier character (so we don't count
    // `MyCustomBlock`) and must be at a struct-literal position (rough
    // heuristic: the next non-whitespace char after the identifier is `{`).
    let bytes = src.as_bytes();
    let needle = b"CustomBlock";
    let mut count = 0usize;
    let mut i = 0usize;

    while i + needle.len() <= bytes.len() {
        if bytes.get(i..i + needle.len()) == Some(needle) {
            let prev_ok = i == 0 || bytes.get(i - 1).is_none_or(|&b| !is_ident_char(b));
            // After the identifier, skip whitespace; expect '{'.
            let mut j = i + needle.len();
            while bytes.get(j).is_some_and(|&b| b == b' ' || b == b'\t') {
                j += 1;
            }
            let next_ok = bytes.get(j) == Some(&b'{');
            if prev_ok && next_ok {
                count += 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }

    count
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Extract a plugin id from a workspace path — `clients/<plugin>/src/...`
/// → `Some("<plugin>")`. Returns `None` for any path not under `clients/`.
#[must_use] 
pub fn plugin_id_from_path(p: &str) -> Option<String> {
    let idx = p.find("clients/")?;
    let after = &p[idx + "clients/".len()..];
    let end = after.find('/')?;
    Some(after[..end].to_string())
}
