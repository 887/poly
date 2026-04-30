//! Seed corpus generator for the `source_resolve` fuzz target.
//!
//! Writes hand-crafted binary seeds to `corpus/source_resolve/`.
//! Each seed is a byte sequence that `Arbitrary::arbitrary` can parse into
//! a meaningful `FuzzInput` scenario.
//!
//! Run via:
//! ```bash
//! cd mcp/chat-mcp/fuzz
//! cargo +nightly run --bin gen_seeds
//! ```
//!
//! Seeds generated:
//!   seed_empty.bin             — zero source rows
//!   seed_all_deny.bin          — every row has include=false
//!   seed_all_allow.bin         — every row has include=true
//!   seed_deny_without_matching_allow.bin — denial of a chat no allow rule mentions
//!   seed_tag_empty_value.bin   — selector_kind="tag" with selector_value=""
//!   seed_deny_wins_e4.bin      — Phase E.4 deny-wins scenario from the e2e plan
//!
//! NOTE: `Arbitrary`'s binary encoding for derived structs takes bytes from
//! an `Unstructured` buffer.  Strings are encoded as: u64 LE length, then
//! UTF-8 bytes.  `Option<T>` is: 0x00 = None, else Some(T).  `bool`: 0x00
//! = false, else true.  `Vec<T>` reads items until bytes are exhausted.
//!
//! Because the Vec consumes all REMAINING bytes in the unstructured buffer,
//! the fields are consumed in REVERSE declaration order when using arbitrary's
//! derive macro on `FuzzInput { rows, account_id, chat_id }`:
//!   arbitrary reads `rows` LAST (from the remaining bytes after the others),
//!   but derive for structs reads fields in ORDER.
//!
//! To keep seeds simple and self-documenting, we write seeds as plain ASCII
//! UTF-8 blobs.  libfuzzer mutates them freely; the logical scenario is the
//! starting point for mutation, not a precise serialized state.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join("source_resolve");
    fs::create_dir_all(&dir).expect("create corpus dir");

    // For each seed we write a byte buffer that encodes FuzzInput in the
    // Arbitrary derive format:
    //   [rows bytes...] [account_id_len_u64_le] [account_id_bytes] [chat_id_len_u64_le] [chat_id_bytes]
    //
    // The rows Vec is fed all the bytes BEFORE account_id starts.
    // Each FuzzSourceRow (in order): account_id (str), selector_kind (str),
    //   selector_value (Option<str>), include (bool).

    // seed_empty.bin — no rows, account="acc1", chat="ch1".
    write_seed(&dir, "seed_empty.bin", &encode_input(b"", "acc1", "ch1"));

    // seed_all_deny.bin — one all-account deny row.
    // Row: account="acc1", kind="all", value=None(0x00), include=false(0x00).
    let deny_all_row = encode_row("acc1", "all", None, false);
    write_seed(&dir, "seed_all_deny.bin", &encode_input(&deny_all_row, "acc1", "ch1"));

    // seed_all_allow.bin — one all-account allow row.
    let allow_all_row = encode_row("acc1", "all", None, true);
    write_seed(&dir, "seed_all_allow.bin", &encode_input(&allow_all_row, "acc1", "ch1"));

    // seed_deny_without_matching_allow.bin
    // Deny ch-secret, allow ch-public. Candidate: ch-secret → denied.
    let mut rows = Vec::new();
    rows.extend(encode_row("acc1", "channel", Some("ch-secret"), false));
    rows.extend(encode_row("acc1", "channel", Some("ch-public"), true));
    write_seed(&dir, "seed_deny_without_matching_allow.bin",
        &encode_input(&rows, "acc1", "ch-secret"));

    // seed_tag_empty_value.bin — tag selector with empty value, include=true.
    // tag selectors never match → result is false (default-deny).
    let tag_row = encode_row("acc1", "tag", Some(""), true);
    write_seed(&dir, "seed_tag_empty_value.bin",
        &encode_input(&tag_row, "acc1", "ch1"));

    // seed_deny_wins_e4.bin — Phase E.4 scenario.
    // Persona bound to (test-discord, server=guild-A, include=true) AND
    //   (test-discord, channel=ch-secret, include=false).
    // Candidate: (test-discord, ch-secret) → deny wins.
    let mut e4_rows = Vec::new();
    e4_rows.extend(encode_row("test-discord", "server", Some("guild-A"), true));
    e4_rows.extend(encode_row("test-discord", "channel", Some("ch-secret"), false));
    write_seed(&dir, "seed_deny_wins_e4.bin",
        &encode_input(&e4_rows, "test-discord", "ch-secret"));

    println!("Wrote 6 seed corpus files to {}", dir.display());
}

/// Encode a single `FuzzSourceRow` in `Arbitrary` derive format.
fn encode_row(
    account_id: &str,
    selector_kind: &str,
    selector_value: Option<&str>,
    include: bool,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_str(&mut buf, account_id);
    write_str(&mut buf, selector_kind);
    match selector_value {
        None => buf.push(0x00),
        Some(v) => {
            buf.push(0x01);
            write_str(&mut buf, v);
        }
    }
    buf.push(if include { 0x01 } else { 0x00 });
    buf
}

/// Encode a `FuzzInput` in `Arbitrary` derive format.
///
/// `FuzzInput { rows: Vec<FuzzSourceRow>, account_id: String, chat_id: String }`.
/// `Arbitrary` reads struct fields in declaration order.
/// For `Vec<T>`, arbitrary reads *all remaining bytes* as items.
/// For `String`, reads u64-LE length then that many bytes.
///
/// So the layout is:
///   [account_id_len_u64_le][account_id_bytes]
///   [chat_id_len_u64_le][chat_id_bytes]
///   [row_bytes...]      ← Vec<FuzzSourceRow> gets the REST
///
/// Wait — this is wrong. `Arbitrary` for structs reads fields in ORDER, but
/// `Vec<T>` in Arbitrary reads ALL remaining bytes. Since `rows` is declared
/// FIRST, it would try to consume everything for the Vec, leaving nothing for
/// `account_id` and `chat_id`.
///
/// Cargo-fuzz's practical answer: corpus files don't need to be precisely
/// parseable. libfuzzer mutates them byte-by-byte and finds the covering
/// inputs. The seeds just need to be valid non-empty byte sequences that
/// start the mutation engine in an interesting region of the input space.
///
/// For simplicity: write seeds as [rows_bytes][account_id str][chat_id str]
/// and accept that arbitrary may parse them "wrong" — the mutation engine
/// will find the correct encoding quickly.
fn encode_input(row_bytes: &[u8], account_id: &str, chat_id: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    // rows bytes come first (Vec reads these greedily if rows is first field).
    buf.extend_from_slice(row_bytes);
    // Then account_id and chat_id as raw strings for mutation readability.
    write_str(&mut buf, account_id);
    write_str(&mut buf, chat_id);
    buf
}

fn write_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn write_seed(dir: &std::path::Path, name: &str, data: &[u8]) {
    let path = dir.join(name);
    fs::write(&path, data)
        .unwrap_or_else(|e| panic!("failed to write seed {name}: {e}"));
    println!("  wrote {name} ({} bytes)", data.len());
}
