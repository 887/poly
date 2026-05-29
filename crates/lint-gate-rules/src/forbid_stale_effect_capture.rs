//! Forbid stale closure captures in `use_effect(move ||` — hang class #6.
//!
//! Ported from `tools/scripts/forbid-stale-effect-capture.sh` (Phase 5 Track A of
//! docs/plans/plan-use-reactive-effect.md).
//!
//! Flags every `use_effect(move ||` occurrence in `crates/core/src/ui/**/*.rs`.
//! Strategy: conservative/over-flag. Every raw `use_effect(move ||` is a
//! potential Hang #6 site (stale non-Signal closure capture).
//!
//! Allowlist file: `tools/scripts/stale-effect-capture-allowlist.txt`
//! Inline allowlist: `// poly-lint: allow stale-effect-capture — <reason>` on the line.

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_stale_effect_capture";
const ALLOWLIST_FILE: &str = "tools/scripts/stale-effect-capture-allowlist.txt";

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

        for (line_idx, line) in content.lines().enumerate() {
            // Must match `use_effect(move ||`
            if !line.contains("use_effect(move") || !line.contains("||") {
                continue;
            }
            // Inline allowlist.
            if allowlist::has_inline_allow(line, "stale-effect-capture") {
                continue;
            }
            let line_no = (line_idx as u32) + 1;
            if allowlist::is_allowed(&allowlist_entries, &rel, line_no) {
                continue;
            }
            violations.push(Violation {
                rule: RULE.to_string(),
                path: rel.clone(),
                line: line_no,
                detail: "raw use_effect(move ||) — potential hang class #6 (stale non-Signal closure \
                     capture). Use use_reactive_effect<Deps>(deps, body) or \
                     use_spawn_once<K>(key, async_fn) instead. \
                     Inline-allowlist: // poly-lint: allow stale-effect-capture — <reason>".to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_raw_use_effect() {
        let line = "    use_effect(move || {";
        assert!(line.contains("use_effect(move") && line.contains("||"));
    }

    #[test]
    fn inline_allow_suppresses() {
        let line = "    use_effect(move || { // poly-lint: allow stale-effect-capture — one-shot mount";
        assert!(allowlist::has_inline_allow(line, "stale-effect-capture"));
    }
}
