#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Regression guard for CLAUDE.md hang class #4 — unbounded `RwLock::read().await`.
//!
//! # Why this test exists
//!
//! The workspace lint `forbid_raw_backend_read`
//! (`crates/lint-gate-rules/src/forbid_raw_backend_read.rs`) is supposed to be the
//! countermeasure for hang class #4, but it matches the **literal** receiver name:
//!
//! ```text
//! const NEEDLE: &str = "backend.read().await";
//! ```
//!
//! checked with a plain `line.contains(NEEDLE)`. Every real call site in this crate
//! binds the handle under a different name — `handle`, `backend_handle`, `bh`,
//! `backend_arc`, `cat_handle`, … — so the rule matched **nothing**, the baseline
//! recorded zero rows, and 14 genuinely unguarded reads sat in WASM-reachable UI
//! code while the gate reported clean.
//!
//! This test enforces the invariant the scanner is *meant* to enforce, but does it
//! receiver-agnostically: **no `.read().await` on any receiver anywhere under the
//! hang-class-#4 scan scope** (`crates/core/src/ui/` and `crates/core/src/ui.rs`).
//! Use [`crate::client_manager::BackendHandleExt::read_with_timeout`] instead — it
//! is cfg-gated (`tokio::time::timeout` on native, a `gloo_timers` race on wasm32)
//! precisely because bare `tokio::time::timeout` panics on `Instant::now()` under
//! `wasm32-unknown-unknown`.
//!
//! Being single-threaded is what makes an unbounded read **fatal** on WASM, not
//! safe: a backend task holding the write half (gateway reconnect, sync writer)
//! wedges the reader forever, the spawned task never returns, and CDP stops
//! responding.
//!
//! This guard lives in `crates/core` because the scanner itself is owned by a
//! different crate; fixing the scanner is tracked separately.

use std::path::{Path, PathBuf};

/// The forbidden call shape, receiver-agnostic.
const FORBIDDEN: &str = ".read().await";

/// Per-file opt-out, mirroring the lint's inline token. A file that genuinely
/// needs a raw read documents it with this marker and a reason.
const INLINE_ALLOW: &str = "poly-lint: allow raw backend.read().await";

/// Recursively collect `.rs` files under `dir`.
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Drop `//`-comments so a doc-comment mentioning the pattern is not a hit.
///
/// Deliberately naive: it does not understand `//` inside a string literal.
/// That only ever makes the check *stricter* on a line that is already
/// suspicious, which is the safe direction for a lint guard.
fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("//") {
            Some(idx) => line.get(..idx).unwrap_or(""),
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Remove all whitespace so a call split across lines still matches, e.g.
/// `handle\n    .read()\n    .await` — a form the line-based scanner misses.
fn despace(src: &str) -> String {
    src.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn no_unbounded_backend_read_in_ui_scope() {
    let core_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let mut files = Vec::new();
    collect_rs(&core_src.join("ui"), &mut files);
    let ui_rs = core_src.join("ui.rs");
    if ui_rs.is_file() {
        files.push(ui_rs);
    }

    assert!(
        !files.is_empty(),
        "scan scope resolved to zero files — the test is not actually checking anything",
    );

    let mut offenders = Vec::new();
    for path in &files {
        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };
        // Documented per-file escape hatch.
        if src.contains(INLINE_ALLOW) {
            continue;
        }
        if despace(&strip_line_comments(&src)).contains(&despace(FORBIDDEN)) {
            offenders.push(path.display().to_string());
        }
    }
    offenders.sort();

    assert!(
        offenders.is_empty(),
        "CLAUDE.md hang class #4: unbounded `{FORBIDDEN}` found in the UI scan scope.\n\
         On the single-threaded WASM executor a writer wedges these reads forever.\n\
         Use `BackendHandleExt::read_with_timeout(Duration::from_secs(5))` and handle\n\
         the timeout branch (reset `loading`, bail the effect).\n\
         Offending files:\n  {}",
        offenders.join("\n  "),
    );
}

/// The guard above is only meaningful if it can actually fail — a scan scope that
/// silently resolves to nothing, or a matcher that never matches, would make it a
/// tautology. (The shipped `forbid_raw_backend_read` unit tests have exactly that
/// defect: they re-implement the match instead of calling `scan`.)
#[test]
fn guard_detects_the_pattern_it_forbids() {
    // Single-line form.
    assert!(despace(&strip_line_comments("let g = handle.read().await;")).contains(&despace(FORBIDDEN)));

    // Multi-line form — the shape the line-based scanner cannot see.
    let split = "let g = handle\n    .read()\n    .await;";
    assert!(despace(&strip_line_comments(split)).contains(&despace(FORBIDDEN)));

    // Any receiver name, not just `backend` — the scanner's blind spot.
    for recv in ["backend", "bh", "backend_arc", "cat_handle", "set"] {
        let line = format!("let g = {recv}.read().await;");
        assert!(
            despace(&strip_line_comments(&line)).contains(&despace(FORBIDDEN)),
            "receiver `{recv}` should be caught",
        );
    }

    // The approved replacement must NOT trip the guard.
    let ok = "let Ok(g) = handle.read_with_timeout(Duration::from_secs(5)).await else { return; };";
    assert!(!despace(&strip_line_comments(ok)).contains(&despace(FORBIDDEN)));

    // A doc-comment mentioning the pattern must NOT trip the guard.
    let doc = "//! Call sites switch from `backend.read().await` to read_with_timeout.";
    assert!(!despace(&strip_line_comments(doc)).contains(&despace(FORBIDDEN)));
}
