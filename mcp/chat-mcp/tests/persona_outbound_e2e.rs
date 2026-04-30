//! G.7 — Integration test: outbound allowlist + daily-cap enforcement.
//!
//! Tests:
//! 1. Enable a persona with outbound-allowlisted proactivity.
//! 2. Add chat to allowlist with max_messages_per_day = 2.
//! 3. Simulate 2 outbound_send audit rows for today.
//! 4. Check check_persona_outbound_cap returns (2, 2) → cap exceeded.
//! 5. Verify a fresh chat (not in allowlist) returns None.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_chat_mcp::memory::MemoryDb;

fn setup_db() -> MemoryDb {
    MemoryDb::open(":memory:").expect("in-memory db")
}

/// Insert a today-dated `outbound_send` audit row manually for testing.
fn insert_outbound_send(
    mem: &MemoryDb,
    persona_slug: &str,
    account_id: &str,
    chat_id: &str,
) {
    mem.record_persona_audit(
        persona_slug,
        "heartbeat",
        "outbound_send",
        Some(account_id),
        Some(chat_id),
        None,
        "ok",
        None,
    )
    .expect("record audit");
}

#[test]
fn outbound_cap_not_exceeded_initially() {
    let mem = setup_db();

    // Create persona with outbound-allowlisted proactivity.
    mem.create_persona(
        "test-outbound",
        "Test Outbound",
        "🤖",
        "You are a test agent.",
        None,
        None,
        "outbound-allowlisted",
        4,
    )
    .expect("create persona");

    // Add chat to allowlist with cap = 2.
    mem.set_persona_outbound_allow("test-outbound", "acc1", "chat1", 2)
        .expect("set allow");

    // No sends yet — cap check should return Some((2, 0)).
    let result = mem
        .check_persona_outbound_cap("test-outbound", "acc1", "chat1")
        .expect("cap check");
    let (max_per_day, sends_today) = result.expect("entry exists");
    assert_eq!(max_per_day, 2);
    assert_eq!(sends_today, 0);
    assert!(sends_today < max_per_day, "cap not exceeded yet");
}

#[test]
fn outbound_cap_exceeded_after_limit_sends() {
    let mem = setup_db();

    mem.create_persona(
        "cap-test",
        "Cap Test",
        "🤖",
        "system prompt",
        None,
        None,
        "outbound-allowlisted",
        4,
    )
    .expect("create persona");

    // max_messages_per_day = 2.
    mem.set_persona_outbound_allow("cap-test", "acc1", "chat1", 2)
        .expect("set allow");

    // Simulate 2 sends today.
    insert_outbound_send(&mem, "cap-test", "acc1", "chat1");
    insert_outbound_send(&mem, "cap-test", "acc1", "chat1");

    let (max_per_day, sends_today) = mem
        .check_persona_outbound_cap("cap-test", "acc1", "chat1")
        .expect("cap check")
        .expect("entry exists");

    assert_eq!(max_per_day, 2);
    assert_eq!(sends_today, 2);
    // G.7: cap reached → deny send.
    assert!(
        sends_today >= max_per_day,
        "daily cap must be reached: sends_today={sends_today} max={max_per_day}"
    );

    // Verify the audit rows were written.
    let audit = mem.list_persona_audit("cap-test", 50).expect("list audit");
    let outbound_rows: Vec<_> = audit
        .iter()
        .filter(|r| r.get("action").and_then(|a| a.as_str()) == Some("outbound_send"))
        .collect();
    assert_eq!(outbound_rows.len(), 2, "expected 2 outbound_send rows");
}

#[test]
fn outbound_blocked_when_chat_not_in_allowlist() {
    let mem = setup_db();

    mem.create_persona(
        "allowlist-test",
        "Allowlist Test",
        "🤖",
        "prompt",
        None,
        None,
        "outbound-allowlisted",
        4,
        true,
    )
    .expect("create persona");

    // Chat not added to allowlist.
    let result = mem
        .check_persona_outbound_cap("allowlist-test", "acc1", "not-allowed-chat")
        .expect("cap check");

    assert!(result.is_none(), "chat not in allowlist → must return None (blocked)");
}

#[test]
fn outbound_prune_audit_before_cutoff() {
    let mem = setup_db();

    mem.create_persona(
        "prune-test",
        "Prune Test",
        "🤖",
        "prompt",
        None,
        None,
        "drafts-only",
        4,
        true,
    )
    .expect("create persona");

    // Write 3 audit rows.
    for _ in 0..3 {
        mem.record_persona_audit("prune-test", "system", "invoke", None, None, None, "ok", None)
            .expect("record audit");
    }
    assert_eq!(mem.list_persona_audit("prune-test", 50).unwrap().len(), 3);

    // Prune with a far-future cutoff → all deleted.
    let deleted = mem
        .prune_persona_audit_before("2099-01-01T00:00:00Z")
        .expect("prune");
    assert_eq!(deleted, 3, "expected 3 rows pruned");
    assert!(mem.list_persona_audit("prune-test", 50).unwrap().is_empty());
}

#[test]
fn quiet_hours_disabled_persisted() {
    let mem = setup_db();

    mem.create_persona(
        "qh-test",
        "QH Test",
        "🤖",
        "prompt",
        None,
        None,
        "outbound-allowlisted",
        4,
        true,
    )
    .expect("create persona");

    // Default is false (quiet hours active).
    let p = mem.get_persona("qh-test").unwrap().unwrap();
    assert!(!p["quiet_hours_disabled"].as_bool().unwrap_or(true));

    // Disable quiet hours.
    mem.set_persona_quiet_hours_disabled("qh-test", true)
        .expect("set quiet hours disabled");

    let p2 = mem.get_persona("qh-test").unwrap().unwrap();
    assert!(p2["quiet_hours_disabled"].as_bool().unwrap_or(false));
}
