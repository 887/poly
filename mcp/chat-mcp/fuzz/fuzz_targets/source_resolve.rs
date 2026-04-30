#![no_main]
//! Fuzz target: `persona::context::is_chat_included`.
//!
//! Tests the deny-wins source-resolution algorithm against an independent
//! reference implementation.  Any panic in `is_chat_included` OR any
//! divergence between the fast path and the reference oracle is a finding.
//!
//! # What this tests
//!
//! `is_chat_included` encodes the deny-wins rule: if ANY matching
//! `PersonaSourceRow` has `include=false`, the candidate (account_id,
//! chat_id) pair is excluded from the persona bundle — regardless of how
//! many allow rows also match.  This is the highest-blast-radius helper in
//! the persona subsystem: a wrong answer leaks private chat data into a
//! persona bundle or silently drops chats the user expected to include.
//!
//! # How to run
//!
//! ```bash
//! cd mcp/chat-mcp/fuzz
//! cargo +nightly fuzz run source_resolve
//! # Time-bounded (CI):
//! cargo +nightly fuzz run source_resolve -- -max_total_time=300
//! # With seed corpus:
//! cargo +nightly fuzz run source_resolve corpus/source_resolve
//! ```
//!
//! See `mcp/chat-mcp/fuzz/README.md` for full usage guide.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

use poly_chat_mcp::persona::{PersonaSourceRow, is_chat_included};

// ─── Arbitrary mirror type ────────────────────────────────────────────────────

/// `Arbitrary`-derived mirror of `PersonaSourceRow`.
///
/// `PersonaSourceRow` cannot `#[derive(Arbitrary)]` in the stable workspace
/// crate (no `arbitrary` dependency there by design).  This wrapper derives
/// `Arbitrary` and converts to the real type via `From`.
#[derive(Debug, Clone, Arbitrary)]
struct FuzzSourceRow {
    account_id: String,
    selector_kind: String,
    selector_value: Option<String>,
    include: bool,
}

impl From<FuzzSourceRow> for PersonaSourceRow {
    fn from(f: FuzzSourceRow) -> Self {
        Self {
            account_id: f.account_id,
            selector_kind: f.selector_kind,
            selector_value: f.selector_value,
            include: f.include,
        }
    }
}

/// Fuzz input: a list of source rows and a candidate (account_id, chat_id).
#[derive(Debug, Arbitrary)]
struct FuzzInput {
    rows: Vec<FuzzSourceRow>,
    account_id: String,
    chat_id: String,
}

// ─── Reference oracle ─────────────────────────────────────────────────────────

/// Slow reference implementation of the deny-wins check.
///
/// Intentionally written differently from `is_chat_included` to act as an
/// independent oracle.  Both must agree on every input — divergence indicates
/// a bug in the fast path.
///
/// Algorithm (naïve, but obviously correct):
/// 1. Collect all rows for the matching account.
/// 2. If ANY matching row has `include=false` → denied (deny-wins).
/// 3. If ANY matching row has `include=true` → allowed.
/// 4. Default: denied.
fn reference_is_chat_included(
    rows: &[PersonaSourceRow],
    account_id: &str,
    chat_id: &str,
) -> bool {
    let account_rows: Vec<&PersonaSourceRow> = rows
        .iter()
        .filter(|r| r.account_id == account_id)
        .collect();

    // Deny check (deny-wins — evaluated unconditionally first).
    for row in &account_rows {
        if !row.include && ref_selector_matches(row, chat_id) {
            return false;
        }
    }

    // Allow check.
    for row in &account_rows {
        if row.include && ref_selector_matches(row, chat_id) {
            return true;
        }
    }

    false // Default-deny.
}

fn ref_selector_matches(row: &PersonaSourceRow, chat_id: &str) -> bool {
    match row.selector_kind.as_str() {
        "all" => true,
        "channel" | "dm" => row.selector_value.as_deref() == Some(chat_id),
        "server" => row.selector_value.as_deref() == Some(chat_id),
        _ => false, // "tag" and unknown kinds are non-matching
    }
}

// ─── Fuzz entry point ─────────────────────────────────────────────────────────

fuzz_target!(|input: FuzzInput| {
    let rows: Vec<PersonaSourceRow> = input.rows
        .into_iter()
        .map(PersonaSourceRow::from)
        .collect();

    // Fast path under test.
    let fast = is_chat_included(&rows, &input.account_id, &input.chat_id);

    // Reference oracle (independently written, same spec).
    let slow = reference_is_chat_included(&rows, &input.account_id, &input.chat_id);

    // Any divergence is a bug.
    assert_eq!(
        fast, slow,
        "is_chat_included diverged from reference: \
         fast={fast} slow={slow} \
         account_id={:?} chat_id={:?} rows={:?}",
        input.account_id, input.chat_id, rows,
    );
});
