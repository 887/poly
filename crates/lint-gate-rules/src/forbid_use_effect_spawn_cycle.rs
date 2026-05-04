//! Forbid `use_effect + spawn(async move { signal.batch/write/set })` — hang class #3.
//!
//! Ported from `tools/scripts/forbid-use-effect-spawn-cycle.sh` (Phase 5 Track A of
//! plan-use-spawn-once.md).
//!
//! Scans `crates/core/src/ui/**/*.rs` for the hang-class-#3 triple:
//!   `use_effect(move || { ... spawn(async move { ... signal.batch|write|set|pending_update ... }) ... })`
//!
//! Allowlist file: `tools/scripts/use-effect-spawn-cycle-allowlist.txt`
//! Inline allowlist: not used — allowlist the whole effect by line number.

use std::path::Path;

use crate::allowlist;
use crate::violation::Violation;
use crate::walk::WorkspaceWalker;

const SCAN_SUBDIR: &str = "crates/core/src/ui";
const RULE: &str = "forbid_use_effect_spawn_cycle";
const ALLOWLIST_FILE: &str = "tools/scripts/use-effect-spawn-cycle-allowlist.txt";

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
    let n = lines.len();
    let mut i = 0;

    while i < n {
        let line = lines[i];
        // Look for `use_effect(move ||` start.
        if !line.contains("use_effect(move") || !line.contains("||") {
            i += 1;
            continue;
        }
        let effect_start_line = (i as u32) + 1;

        // Collect the effect body by brace counting.
        let mut effect_depth: i32 = 0;
        let mut spawn_depth_start: i32 = -1;
        let mut in_spawn = false;
        let mut found_spawn = false;
        let mut found_write = false;
        let mut j = i;
        let mut effect_ended_at = n;

        while j < n {
            let l = lines[j];
            // Strip line comments.
            let stripped = match l.find("//") {
                Some(pos) => &l[..pos],
                None => l,
            };

            // Look for spawn start inside effect.
            if effect_depth > 0 && !in_spawn {
                if stripped.contains("spawn(async move") || stripped.contains("spawn(async move {") {
                    found_spawn = true;
                    in_spawn = true;
                    spawn_depth_start = effect_depth;
                }
            }

            // Look for write-family calls inside spawn.
            if in_spawn {
                for method in &[".batch(", ".write()", ".write(", ".set(", ".pending_update("] {
                    if stripped.contains(method) {
                        found_write = true;
                        break;
                    }
                }
            }

            // Count braces.
            for ch in stripped.chars() {
                match ch {
                    '{' => {
                        effect_depth += 1;
                    }
                    '}' => {
                        effect_depth -= 1;
                        if in_spawn && effect_depth < spawn_depth_start {
                            in_spawn = false;
                        }
                        if effect_depth <= 0 && j > i {
                            // Effect ended.
                            effect_ended_at = j;
                            j = n; // signal break
                            break;
                        }
                    }
                    _ => {}
                }
            }
            j += 1;
        }

        // If we found the triple, emit a violation.
        if found_spawn && found_write && effect_ended_at < n {
            if !allowlist::is_allowed(allowlist_entries, rel, effect_start_line) {
                violations.push(Violation {
                    rule: RULE.to_string(),
                    path: rel.to_string(),
                    line: effect_start_line,
                    detail: format!(
                        "use_effect+spawn+signal-write triple — hang class #3. \
                         Use use_spawn_once<K>(key, async_fn) instead. \
                         See: crates/core/src/state/use_spawn_once.rs, \
                         docs/plans/plan-use-spawn-once.md. \
                         Allowlist: tools/scripts/use-effect-spawn-cycle-allowlist.txt"
                    ),
                });
            }
        }

        // Advance past the effect body.
        i = effect_ended_at + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_spawn_signal_write_triple() {
        let src = r#"
fn foo() {
    use_effect(move || {
        spawn(async move {
            data.batch(|v| { v.loaded = true; });
        });
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(!violations.is_empty(), "should detect triple");
    }

    #[test]
    fn allows_effect_without_spawn() {
        let src = r#"
fn foo() {
    use_effect(move || {
        let x = data.read();
        drop(x);
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "no spawn — should not flag");
    }

    #[test]
    fn allows_spawn_without_write() {
        let src = r#"
fn foo() {
    use_effect(move || {
        spawn(async move {
            let val = data.read().clone();
            log::info!("{:?}", val);
        });
    });
}
"#;
        let mut violations = Vec::new();
        scan_file_content(src, "test.rs", &[], &mut violations);
        assert!(violations.is_empty(), "spawn without write — should not flag");
    }
}
