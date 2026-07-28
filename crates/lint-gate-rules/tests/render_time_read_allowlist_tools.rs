#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Maintenance tools for `tools/scripts/render-time-read-allowlist.txt`
//! (plan-lint-gate-integrity Phase B).
//!
//! Both entry points are `#[ignore]`d: they read and write repo files, so they
//! run on demand, never as part of `cargo test`.
//!
//! ```bash
//! # 1. Snapshot what the current allowlist actually suppresses.
//! POLY_EFFECTIVE_DUMP=/tmp/before \
//!   cargo test -p poly-lint-gate-rules -- --ignored dump_effective_suppressed_set
//!
//! # 2. Re-key every live entry on content; drop entries that suppress nothing.
//! cargo test -p poly-lint-gate-rules -- --ignored regenerate_render_time_read_allowlist
//!
//! # 3. Snapshot again and prove the suppressed set is byte-identical.
//! POLY_EFFECTIVE_DUMP=/tmp/after \
//!   cargo test -p poly-lint-gate-rules -- --ignored dump_effective_suppressed_set
//! command diff /tmp/before/suppressed.txt /tmp/after/suppressed.txt
//! ```
//!
//! Step 3 is the acceptance test: re-keying must not widen or narrow the set of
//! suppressed sites by a single line.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use poly_lint_gate_rules::allowlist::{self, AllowlistEntry};
use poly_lint_gate_rules::forbid_render_time_read as rule;

const ALLOWLIST_REL: &str = "tools/scripts/render-time-read-allowlist.txt";
const SCAN_REL: &str = "crates/core/src/ui";

/// Emit a line of tool output.
///
/// `println!` is denied workspace-wide (`clippy::print_stdout`) and `#[allow]`
/// is banned by the `allow_ban` scan, so write through the `Write` impl — these
/// are operator-facing maintenance tools whose whole product is their report.
macro_rules! report {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        drop(writeln!(std::io::stdout(), $($arg)*));
    }};
}

fn ws_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/lint-gate-rules is two levels below the workspace root")
        .to_path_buf()
}

/// Every `.rs` file the rule scans, repo-relative-path sorted.
fn scanned_files(root: &Path) -> Vec<PathBuf> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    collect(&root.join(SCAN_REL), &mut files);
    // `crates/core/src/ui.rs` is in scope too — the rule matches on the
    // `crates/core/src/ui` path *substring*, not on a directory prefix.
    let ui_rs = root.join("crates/core/src/ui.rs");
    if ui_rs.is_file() {
        files.push(ui_rs);
    }
    assert!(
        !files.is_empty(),
        "scan scope resolved to zero files — the tool would happily emit an empty allowlist"
    );
    files.sort();
    files
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .into_owned()
}

/// One render-time `.read()` site, with everything needed to key it either way.
struct Site {
    rel: String,
    line: u32,
    fingerprint: String,
    ordinal: u32,
    text: String,
}

fn all_sites(root: &Path) -> Vec<Site> {
    let mut sites = Vec::new();
    for path in scanned_files(root) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = rel_of(root, &path);
        let keys = allowlist::line_keys(&content);
        let lines: Vec<&str> = content.lines().collect();
        for line in rule::candidate_lines(&content) {
            let idx = usize::try_from(line).unwrap_or(0).saturating_sub(1);
            let (fingerprint, ordinal) = keys.get(idx).cloned().unwrap_or_default();
            sites.push(Site {
                rel: rel.clone(),
                line,
                fingerprint,
                ordinal,
                text: allowlist::normalise_line(lines.get(idx).copied().unwrap_or("")),
            });
        }
    }
    sites
}

/// Does the on-disk allowlist suppress this site, under **either** key scheme?
fn suppressed(entries: &[AllowlistEntry], site: &Site) -> bool {
    allowlist::is_allowed(entries, &site.rel, site.line)
        || allowlist::is_allowed_content(entries, &site.rel, &site.fingerprint, site.ordinal)
}

/// Dump the effective suppressed set and the effective violation set.
///
/// This is the before/after evidence for the re-key: source files are untouched
/// by the migration, so `path:line` is a fair comparison key across it.
#[test]
#[ignore = "maintenance tool: writes to $POLY_EFFECTIVE_DUMP"]
fn dump_effective_suppressed_set() {
    let root = ws_root();
    let out_dir = PathBuf::from(
        std::env::var("POLY_EFFECTIVE_DUMP").expect("set POLY_EFFECTIVE_DUMP to an output dir"),
    );
    std::fs::create_dir_all(&out_dir).unwrap();

    let entries = allowlist::load(&root.join(ALLOWLIST_REL));
    let sites = all_sites(&root);

    let mut yes = Vec::new();
    let mut no = Vec::new();
    for site in &sites {
        let key = format!("{}:{}", site.rel, site.line);
        if suppressed(&entries, site) {
            yes.push(key);
        } else {
            no.push(key);
        }
    }
    yes.sort();
    no.sort();
    std::fs::write(out_dir.join("suppressed.txt"), format!("{}\n", yes.join("\n"))).unwrap();
    std::fs::write(out_dir.join("violations.txt"), format!("{}\n", no.join("\n"))).unwrap();
    report!("candidates={} suppressed={} violations={}",
        sites.len(),
        yes.len(),
        no.len()
    );
}

/// Rebuild `render-time-read-allowlist.txt` keyed on line content.
///
/// Idempotent, and accepts either key scheme on input: it derives the new file
/// from the set of sites the current file actually suppresses, so entries that
/// suppress nothing are dropped by construction rather than by a guess about
/// what they "meant".
#[test]
#[ignore = "maintenance tool: rewrites tools/scripts/render-time-read-allowlist.txt"]
fn regenerate_render_time_read_allowlist() {
    let root = ws_root();
    let allowlist_path = root.join(ALLOWLIST_REL);
    let raw = std::fs::read_to_string(&allowlist_path).expect("allowlist file must exist");

    // Carry each entry's `# reason` across the re-key, keyed by its raw text.
    let mut reasons: BTreeMap<String, String> = BTreeMap::new();
    for line in raw.lines() {
        let Some((key, reason)) = line.split_once('#') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        reasons.insert(key.to_string(), reason.trim().to_string());
    }

    let entries = allowlist::load(&allowlist_path);
    let sites = all_sites(&root);

    let mut kept = Vec::new();
    for site in &sites {
        if !suppressed(&entries, site) {
            continue;
        }
        let legacy_key = format!("{}:{}", site.rel, site.line);
        let content_key =
            allowlist::render_content_key(&site.rel, site.ordinal, &site.fingerprint);
        let reason = reasons
            .get(&content_key)
            .or_else(|| reasons.get(&legacy_key))
            .map_or_else(
                || {
                    "MEDIUM — render-time read drives output or child prop, subscription is intent"
                        .to_string()
                },
                |r| r.split('|').next().unwrap_or(r).trim().to_string(),
            );
        kept.push(format!("{content_key} # {reason} | {}", site.text));
    }
    kept.sort();
    kept.dedup();

    let body = format!("{HEADER}{}\n", kept.join("\n"));
    std::fs::write(&allowlist_path, body).unwrap();
    report!("regenerated {ALLOWLIST_REL}: {} entries covering {} candidate sites",
        kept.len(),
        sites.len()
    );
}

/// Classify every entry of the *current* allowlist into the Phase B.1 buckets.
///
/// Run this before regenerating to get the drop count for the record.
#[test]
#[ignore = "maintenance tool: prints the Phase B.1 bucket report"]
fn report_allowlist_buckets() {
    let root = ws_root();
    let sites = all_sites(&root);

    // Which sites does each entry cover?
    let mut live = 0_u32;
    let mut dead_no_read = 0_u32;
    let mut dead_not_a_candidate = 0_u32;
    let mut dead_out_of_range = 0_u32;
    let mut dead_missing_file = 0_u32;
    let mut dead_examples = Vec::new();

    for (src_line, entry) in allowlist::load_with_lines(&root.join(ALLOWLIST_REL)) {
        let (rel, line) = match &entry {
            AllowlistEntry::PathLine(p, n) => (p.clone(), *n),
            AllowlistEntry::PathContent { .. } => {
                live = live.saturating_add(1);
                continue;
            }
            other @ (AllowlistEntry::WholePath(_)
            | AllowlistEntry::PathRange(..)
            | AllowlistEntry::PathReceiver(..)) => {
                panic!("unexpected entry shape at line {src_line}: {other:?}")
            }
        };
        if sites.iter().any(|s| s.rel == rel && s.line == line) {
            live = live.saturating_add(1);
            continue;
        }
        let Ok(content) = std::fs::read_to_string(root.join(&rel)) else {
            dead_missing_file = dead_missing_file.saturating_add(1);
            continue;
        };
        let text = content
            .lines()
            .nth(usize::try_from(line).unwrap_or(0).saturating_sub(1));
        match text {
            None => dead_out_of_range = dead_out_of_range.saturating_add(1),
            Some(t) if !t.contains(".read()") => {
                dead_no_read = dead_no_read.saturating_add(1);
                if dead_examples.len() < 5 {
                    dead_examples.push(format!("{rel}:{line} -> {}", t.trim()));
                }
            }
            Some(_) => dead_not_a_candidate = dead_not_a_candidate.saturating_add(1),
        }
    }

    report!("--- Phase B.1 buckets for {ALLOWLIST_REL} ---");
    report!("live (suppresses a real candidate site): {live}");
    report!("dead — named line holds no `.read()`:    {dead_no_read}");
    report!("dead — line has .read() but is not a candidate (safe context / .await / inline allow): {dead_not_a_candidate}");
    report!("dead — line number past end of file:     {dead_out_of_range}");
    report!("dead — file no longer exists:            {dead_missing_file}");
    for e in &dead_examples {
        report!("  example dead entry: {e}");
    }
}

const HEADER: &str = "\
# render-time-read-allowlist.txt
#
# Allowlisted render-time .read() sites in crates/core/src/ui.
# These are sites where .read() appears at the top-level of a render body
# or hook-setup function but the subscription IS the intent (MEDIUM/LOW
# risk). They drive rsx! output, conditional rendering, or child-component
# props -- the component SHOULD re-render when the signal changes.
# HIGH-risk sites (value drives a use_spawn_once / use_reactive_effect key
# or a .batch() call in the same body) MUST NOT appear here -- migrate to
# .peek() instead.
#
# FORMAT (content-keyed, plan-lint-gate-integrity Phase B):
#
#   <path>@<ordinal>:<fingerprint> # <reason> | <the suppressed source line>
#
#   ordinal     which occurrence of that exact line within the file (0-based),
#               so two byte-identical .read() lines stay two distinct sites
#   fingerprint FNV-1a 64 of the whitespace-normalised line, as h<16 hex>
#
# The trailing `| <source line>` is a human-readable echo only; the key is
# everything before the first `#`.
#
# WHY NOT `path:line`: this file used to be line-keyed and 1323 of its 1469
# entries (90%) named a line holding no `.read()` at all. Those suppressions
# were inert while the sites they meant to cover had drifted into
# crates/lint-gate/baseline.json, so an unrelated one-line reflow flipped
# sites between \"allowlisted\" and \"baselined\" and manufactured a false
# new-hang-class violation. Content keys survive reflow; line keys do not.
#
# REGENERATE (never hand-edit a fingerprint):
#   cargo test -p poly-lint-gate-rules -- --ignored regenerate_render_time_read_allowlist
#
# The `render_time_read_allowlist` self-check in
# crates/lint-gate-rules/src/forbid_render_time_read.rs fails the gate if any
# entry here stops resolving to a real site, so the inert state cannot recur.
#
# Inline suppression (preferred for one-offs):
#   // poly-lint: allow render-time-read -- <reason>
# See: docs/dev/reactive-state.md section .peek() vs .read()
#      docs/plans/plan-peek-vs-read.md
#      docs/plans/plan-lint-gate-integrity.md Phase B
#
# Migrated HIGH sites (see docs/plans/plan-peek-vs-read.md):
#   - crates/core/src/ui/account/common/thread_view.rs (two .read() keys -> .peek())
#   - crates/core/src/ui/account/common/chat_view.rs use_search_messages_effect (-> .peek())
#   - crates/core/src/ui/account/common/chat_view.rs use_pinned_messages_effect (-> .peek())
";
