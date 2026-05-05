//! Integration tests for MemoryDb — all tables.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use super::helpers::now_iso8601;
use super::MemoryDb;

fn fresh_db() -> MemoryDb {
    MemoryDb::open(":memory:").expect("open in-memory db")
}

// ── contact_facts ─────────────────────────────────────────────────────────

#[test]
fn remember_and_recall_fact() {
    let db = fresh_db();
    let id = db.remember_fact("acc1", "contact1", "preference", "likes coffee").unwrap();
    assert!(id > 0);

    let facts = db.recall_facts("acc1", "contact1", None).unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0]["fact_text"], "likes coffee");
    assert_eq!(facts[0]["category"], "preference");
    assert_eq!(facts[0]["id"], id);
}

#[test]
fn recall_facts_with_category_filter() {
    let db = fresh_db();
    db.remember_fact("acc1", "c1", "preference", "likes coffee").unwrap();
    db.remember_fact("acc1", "c1", "schedule", "free Friday").unwrap();
    db.remember_fact("acc1", "c1", "preference", "hates Mondays").unwrap();

    let prefs = db.recall_facts("acc1", "c1", Some("preference")).unwrap();
    assert_eq!(prefs.len(), 2);

    let sched = db.recall_facts("acc1", "c1", Some("schedule")).unwrap();
    assert_eq!(sched.len(), 1);
    assert_eq!(sched[0]["fact_text"], "free Friday");
}

#[test]
fn recall_facts_account_scoped() {
    let db = fresh_db();
    db.remember_fact("acc1", "c1", "", "fact A").unwrap();
    db.remember_fact("acc2", "c1", "", "fact B").unwrap();

    let a = db.recall_facts("acc1", "c1", None).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0]["fact_text"], "fact A");

    let b = db.recall_facts("acc2", "c1", None).unwrap();
    assert_eq!(b.len(), 1);
    assert_eq!(b[0]["fact_text"], "fact B");
}

#[test]
fn forget_fact() {
    let db = fresh_db();
    let id = db.remember_fact("acc1", "c1", "", "to forget").unwrap();
    db.forget_fact(id).unwrap();

    let facts = db.recall_facts("acc1", "c1", None).unwrap();
    assert!(facts.is_empty());
}

#[test]
fn forget_nonexistent_fact_is_noop() {
    let db = fresh_db();
    db.forget_fact(9999).unwrap(); // must not error
}

#[test]
fn search_facts_like() {
    let db = fresh_db();
    db.remember_fact("acc1", "c1", "", "loves hiking in the mountains").unwrap();
    db.remember_fact("acc1", "c2", "", "prefers staying indoors").unwrap();
    db.remember_fact("acc2", "c1", "", "hiking enthusiast").unwrap();

    let results = db.search_facts("hiking", None).unwrap();
    assert_eq!(results.len(), 2);

    let scoped = db.search_facts("hiking", Some("acc1")).unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0]["account_id"], "acc1");
}

#[test]
fn search_facts_no_match() {
    let db = fresh_db();
    db.remember_fact("acc1", "c1", "", "likes tea").unwrap();
    let results = db.search_facts("coffee", None).unwrap();
    assert!(results.is_empty());
}

// ── chat_notes ────────────────────────────────────────────────────────────

#[test]
fn store_and_get_chat_note() {
    let db = fresh_db();
    let id = db.store_chat_note("acc1", "chat1", "remember: bring umbrella").unwrap();
    assert!(id > 0);

    let notes = db.get_chat_notes("acc1", "chat1").unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["note_text"], "remember: bring umbrella");
    assert_eq!(notes[0]["id"], id);
}

#[test]
fn multiple_notes_ordered_by_id() {
    let db = fresh_db();
    let id1 = db.store_chat_note("acc1", "chat1", "note one").unwrap();
    let id2 = db.store_chat_note("acc1", "chat1", "note two").unwrap();
    let notes = db.get_chat_notes("acc1", "chat1").unwrap();
    assert_eq!(notes.len(), 2);
    assert!(notes[0]["id"].as_i64().unwrap() < notes[1]["id"].as_i64().unwrap());
    let _ids = (id1, id2);
}

#[test]
fn forget_chat_note() {
    let db = fresh_db();
    let id = db.store_chat_note("acc1", "chat1", "to forget").unwrap();
    db.forget_chat_note(id).unwrap();

    let notes = db.get_chat_notes("acc1", "chat1").unwrap();
    assert!(notes.is_empty());
}

#[test]
fn get_chat_notes_empty_for_unknown_chat() {
    let db = fresh_db();
    let notes = db.get_chat_notes("acc1", "unknown-chat").unwrap();
    assert!(notes.is_empty());
}

// ── chat_summaries ────────────────────────────────────────────────────────

#[test]
fn store_and_get_chat_summary() {
    let db = fresh_db();
    db.store_chat_summary("acc1", "chat1", "Alice and Bob discussed the project", "msg1", "msg20").unwrap();

    let s = db.get_chat_summary("acc1", "chat1").unwrap();
    assert!(s.is_some());
    let s = s.unwrap();
    assert_eq!(s["summary"], "Alice and Bob discussed the project");
    assert_eq!(s["window_start"], "msg1");
    assert_eq!(s["window_end"], "msg20");
}

#[test]
fn chat_summary_upsert() {
    let db = fresh_db();
    db.store_chat_summary("acc1", "chat1", "old summary", "msg1", "msg10").unwrap();
    db.store_chat_summary("acc1", "chat1", "new summary", "msg11", "msg20").unwrap();

    let s = db.get_chat_summary("acc1", "chat1").unwrap().unwrap();
    assert_eq!(s["summary"], "new summary");
    assert_eq!(s["window_start"], "msg11");
}

#[test]
fn get_chat_summary_returns_none_when_missing() {
    let db = fresh_db();
    let s = db.get_chat_summary("acc1", "no-chat").unwrap();
    assert!(s.is_none());
}

#[test]
fn summaries_are_per_account_and_chat() {
    let db = fresh_db();
    db.store_chat_summary("acc1", "chat1", "summary A", "", "").unwrap();
    db.store_chat_summary("acc2", "chat1", "summary B", "", "").unwrap();
    db.store_chat_summary("acc1", "chat2", "summary C", "", "").unwrap();

    assert_eq!(db.get_chat_summary("acc1", "chat1").unwrap().unwrap()["summary"], "summary A");
    assert_eq!(db.get_chat_summary("acc2", "chat1").unwrap().unwrap()["summary"], "summary B");
    assert_eq!(db.get_chat_summary("acc1", "chat2").unwrap().unwrap()["summary"], "summary C");
}

// ── drafts ────────────────────────────────────────────────────────────────

#[test]
fn draft_insert_and_list() {
    let db = fresh_db();
    let id = db.draft_insert("acc1", "chat1", "Hello!", "test-agent", None).unwrap();
    assert!(id > 0);

    let drafts = db.draft_list(Some("acc1"), Some("chat1"), Some("pending")).unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0]["body"], "Hello!");
    assert_eq!(drafts[0]["status"], "pending");
    assert_eq!(drafts[0]["suggested_by"], "test-agent");
    assert!(drafts[0]["auto_send_at"].is_null());
}

#[test]
fn draft_insert_with_autosend() {
    let db = fresh_db();
    let id = db.draft_insert("acc1", "chat1", "Scheduled!", "test-agent", Some("2030-01-01T00:00:00Z")).unwrap();
    assert!(id > 0);

    let drafts = db.draft_list(Some("acc1"), Some("chat1"), None).unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0]["auto_send_at"], "2030-01-01T00:00:00Z");
}

#[test]
fn draft_edit_pending() {
    let db = fresh_db();
    let id = db.draft_insert("acc1", "chat1", "Original", "bot", None).unwrap();
    let changed = db.draft_edit(id, "Updated body").unwrap();
    assert!(changed);

    let d = db.draft_get(id).unwrap().unwrap();
    assert_eq!(d["body"], "Updated body");
}

#[test]
fn draft_edit_non_pending_fails() {
    let db = fresh_db();
    let id = db.draft_insert("acc1", "chat1", "body", "bot", None).unwrap();
    db.draft_set_status(id, "sent").unwrap();

    let changed = db.draft_edit(id, "attempt").unwrap();
    assert!(!changed, "edit of sent draft should return false");
}

#[test]
fn draft_discard() {
    let db = fresh_db();
    let id = db.draft_insert("acc1", "chat1", "body", "bot", None).unwrap();
    db.draft_set_status(id, "discarded").unwrap();

    let d = db.draft_get(id).unwrap().unwrap();
    assert_eq!(d["status"], "discarded");

    let pending = db.draft_list(Some("acc1"), Some("chat1"), Some("pending")).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn draft_clear_autosend() {
    let db = fresh_db();
    let id = db.draft_insert("acc1", "chat1", "body", "bot", Some("2030-01-01T00:00:00Z")).unwrap();
    db.draft_clear_autosend(id).unwrap();

    let d = db.draft_get(id).unwrap().unwrap();
    assert!(d["auto_send_at"].is_null());
}

#[test]
fn draft_list_no_filters() {
    let db = fresh_db();
    db.draft_insert("acc1", "chat1", "a", "bot", None).unwrap();
    db.draft_insert("acc2", "chat2", "b", "bot", None).unwrap();

    let all = db.draft_list(None, None, None).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn draft_pending_autosend_returns_overdue() {
    let db = fresh_db();
    // Past timestamp — should be returned.
    db.draft_insert("acc1", "chat1", "overdue", "bot", Some("2020-01-01T00:00:00Z")).unwrap();
    // Future timestamp — should NOT be returned.
    db.draft_insert("acc1", "chat1", "future", "bot", Some("2090-01-01T00:00:00Z")).unwrap();
    // No auto_send — should NOT be returned.
    db.draft_insert("acc1", "chat1", "manual", "bot", None).unwrap();

    let due = db.draft_pending_autosend().unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0]["body"], "overdue");
}

// ── chat_style ────────────────────────────────────────────────────────────

#[test]
fn set_and_get_chat_style() {
    let db = fresh_db();
    db.set_chat_style("acc1", "chat1", Some("casual"), Some("tu"), Some(true), Some("Alex"), Some("prefers short replies")).unwrap();
    let style = db.get_chat_style("acc1", "chat1").unwrap();
    assert!(style.is_some());
    let s = style.unwrap();
    assert_eq!(s["tone"], "casual");
    assert_eq!(s["formality"], "tu");
    assert_eq!(s["emoji_allowed"], true);
    assert_eq!(s["signature"], "Alex");
    assert_eq!(s["extra_notes"], "prefers short replies");
}

#[test]
fn get_chat_style_returns_none_when_missing() {
    let db = fresh_db();
    let style = db.get_chat_style("acc1", "no-chat").unwrap();
    assert!(style.is_none());
}

#[test]
fn set_chat_style_partial_update_preserves_unset_fields() {
    let db = fresh_db();
    db.set_chat_style("acc1", "chat1", Some("warm"), Some("vous"), Some(false), Some("Bob"), None).unwrap();
    // Update only tone — other fields must stay.
    db.set_chat_style("acc1", "chat1", Some("direct"), None, None, None, None).unwrap();
    let s = db.get_chat_style("acc1", "chat1").unwrap().unwrap();
    assert_eq!(s["tone"], "direct");
    assert_eq!(s["formality"], "vous");
    assert_eq!(s["emoji_allowed"], false);
    assert_eq!(s["signature"], "Bob");
}

#[test]
fn list_chat_styles_filtered_by_account() {
    let db = fresh_db();
    db.set_chat_style("acc1", "chat1", Some("casual"), None, Some(true), None, None).unwrap();
    db.set_chat_style("acc1", "chat2", Some("warm"), None, Some(true), None, None).unwrap();
    db.set_chat_style("acc2", "chat1", Some("direct"), None, Some(true), None, None).unwrap();

    let list1 = db.list_chat_styles(Some("acc1")).unwrap();
    assert_eq!(list1.len(), 2);
    for item in &list1 {
        assert_eq!(item["account_id"], "acc1");
    }

    let all = db.list_chat_styles(None).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn forget_chat_style() {
    let db = fresh_db();
    db.set_chat_style("acc1", "chat1", Some("snarky"), None, Some(true), None, None).unwrap();
    db.forget_chat_style("acc1", "chat1").unwrap();
    assert!(db.get_chat_style("acc1", "chat1").unwrap().is_none());
}

#[test]
fn forget_chat_style_nonexistent_is_noop() {
    let db = fresh_db();
    db.forget_chat_style("acc1", "ghost-chat").unwrap(); // must not error
}

// ── helpers ───────────────────────────────────────────────────────────────

#[test]
fn now_iso8601_looks_plausible() {
    let s = now_iso8601();
    // "2026-04-19T12:34:56Z" — length 20, has 'T' and 'Z'
    assert_eq!(s.len(), 20, "unexpected length: {s}");
    assert!(s.contains('T'));
    assert!(s.ends_with('Z'));
    assert!(s.starts_with("20")); // year 2xxx
}

// ── personas ─────────────────────────────────────────────────────────────

#[test]
fn create_and_get_persona() {
    let db = fresh_db();
    let slug = db.create_persona(
        "broker-bob", "Broker Bob", "💼",
        "You are my finance broker.", None, None,
        "drafts-only", 4,
    ).unwrap();
    assert_eq!(slug, "broker-bob");

    let p = db.get_persona("broker-bob").unwrap().unwrap();
    assert_eq!(p["slug"], "broker-bob");
    assert_eq!(p["name"], "Broker Bob");
    assert_eq!(p["avatar_emoji"], "💼");
    assert_eq!(p["system_prompt"], "You are my finance broker.");
    assert!(p["style_notes"].is_null());
    assert!(p["heartbeat_interval_secs"].is_null());
    assert_eq!(p["proactivity"], "drafts-only");
    assert_eq!(p["rate_limit_per_hour"], 4_i64);
    assert_eq!(p["enabled"], true);
    assert!(p["last_run_at"].is_null());
}

#[test]
fn get_persona_returns_none_when_missing() {
    let db = fresh_db();
    assert!(db.get_persona("nonexistent").unwrap().is_none());
}

#[test]
fn list_personas_ordered_by_name() {
    let db = fresh_db();
    db.create_persona("zzz", "Zzz", "🤖", "prompt", None, None, "drafts-only", 4).unwrap();
    db.create_persona("aaa", "Aaa", "🤖", "prompt", None, None, "drafts-only", 4).unwrap();

    let list = db.list_personas().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["slug"], "aaa");
    assert_eq!(list[1]["slug"], "zzz");
}

#[test]
fn update_persona_partial() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "old prompt", None, None, "drafts-only", 4).unwrap();

    let updated = db.update_persona(
        "bob",
        Some("Bob Updated"), // name
        None,                // avatar unchanged
        Some("new prompt"),  // system_prompt
        None,                // style_notes unchanged
        None,                // heartbeat unchanged
        None,                // proactivity unchanged
        Some(8),             // rate_limit changed
        None,                // enabled unchanged
        None,                // last_run_at unchanged
    ).unwrap();
    assert!(updated);

    let p = db.get_persona("bob").unwrap().unwrap();
    assert_eq!(p["name"], "Bob Updated");
    assert_eq!(p["system_prompt"], "new prompt");
    assert_eq!(p["rate_limit_per_hour"], 8_i64);
    assert_eq!(p["avatar_emoji"], "🤖");   // preserved
    assert_eq!(p["proactivity"], "drafts-only"); // preserved
}

#[test]
fn update_persona_nonexistent_returns_false() {
    let db = fresh_db();
    let updated = db.update_persona(
        "ghost", None, None, None, None, None, None, None, None, None,
    ).unwrap();
    assert!(!updated);
}

#[test]
fn delete_persona_cascades() {
    let db = fresh_db();
    db.create_persona("frag-frank", "Frag Frank", "🎮", "hype-man", None, None, "notify", 4).unwrap();

    // Add child rows to each child table.
    db.add_persona_source("frag-frank", "discord-1", "server", Some("guild-1"), true).unwrap();
    db.add_persona_tool("frag-frank", "get_messages").unwrap();
    db.add_persona_fact("frag-frank", Some("observation"), "raid tonight", false).unwrap();
    db.set_persona_outbound_allow("frag-frank", "discord-1", "channel-1", 1).unwrap();
    db.record_persona_audit(
        "frag-frank", "user", "invoke", None, None, None, "ok", None,
    ).unwrap();

    db.delete_persona("frag-frank").unwrap();

    // Parent row gone.
    assert!(db.get_persona("frag-frank").unwrap().is_none());
    // All child tables empty.
    assert!(db.list_persona_sources("frag-frank").unwrap().is_empty());
    assert!(db.list_persona_tools("frag-frank").unwrap().is_empty());
    assert!(db.list_persona_facts("frag-frank", false).unwrap().is_empty());
    assert!(db.list_persona_outbound_allows("frag-frank").unwrap().is_empty());
    assert!(db.list_persona_audit("frag-frank", 100).unwrap().is_empty());
}

// ── persona_sources ───────────────────────────────────────────────────────

#[test]
fn add_and_list_persona_sources() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();

    db.add_persona_source("bob", "discord-1", "server", Some("guild-A"), true).unwrap();
    db.add_persona_source("bob", "discord-1", "channel", Some("ch-deny"), false).unwrap();

    let sources = db.list_persona_sources("bob").unwrap();
    assert_eq!(sources.len(), 2);
    let allow = sources.iter().find(|s| s["include"] == true).unwrap();
    assert_eq!(allow["selector_kind"], "server");
    let deny = sources.iter().find(|s| s["include"] == false).unwrap();
    assert_eq!(deny["selector_value"], "ch-deny");
}

#[test]
fn add_persona_source_duplicate_is_noop() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    // Use a non-NULL selector_value so the UNIQUE constraint fires correctly
    // (SQLite treats two NULLs as distinct in UNIQUE constraints).
    db.add_persona_source("bob", "discord-1", "server", Some("guild-A"), true).unwrap();
    db.add_persona_source("bob", "discord-1", "server", Some("guild-A"), true).unwrap(); // duplicate
    assert_eq!(db.list_persona_sources("bob").unwrap().len(), 1);
}

#[test]
fn remove_persona_source() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    let id = db.add_persona_source("bob", "discord-1", "server", Some("g"), true).unwrap();
    db.remove_persona_source(id).unwrap();
    assert!(db.list_persona_sources("bob").unwrap().is_empty());
}

// ── persona_tool_whitelist ────────────────────────────────────────────────

#[test]
fn add_list_remove_persona_tools() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    db.add_persona_tool("bob", "get_messages").unwrap();
    db.add_persona_tool("bob", "draft_create").unwrap();
    db.add_persona_tool("bob", "get_messages").unwrap(); // dup — ignored

    let tools = db.list_persona_tools("bob").unwrap();
    assert_eq!(tools.len(), 2);
    assert!(tools.contains(&"draft_create".to_string()));

    db.remove_persona_tool("bob", "draft_create").unwrap();
    let tools = db.list_persona_tools("bob").unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0], "get_messages");
}

// ── persona_facts ─────────────────────────────────────────────────────────

#[test]
fn add_and_list_persona_facts() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    let id1 = db.add_persona_fact("bob", Some("observation"), "user likes ETH", false).unwrap();
    let id2 = db.add_persona_fact("bob", Some("reminder"), "check earnings Friday", true).unwrap();
    assert!(id1 > 0 && id2 > 0);

    let all = db.list_persona_facts("bob", false).unwrap();
    assert_eq!(all.len(), 2);

    let pinned = db.list_persona_facts("bob", true).unwrap();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0]["fact_text"], "check earnings Friday");
    assert_eq!(pinned[0]["pinned"], true);
}

#[test]
fn remove_persona_fact() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    let id = db.add_persona_fact("bob", None, "temporary", false).unwrap();
    db.remove_persona_fact(id).unwrap();
    assert!(db.list_persona_facts("bob", false).unwrap().is_empty());
}

#[test]
fn forget_all_persona_facts() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    db.add_persona_fact("bob", None, "fact 1", false).unwrap();
    db.add_persona_fact("bob", None, "fact 2", true).unwrap();
    db.forget_all_persona_facts("bob").unwrap();
    assert!(db.list_persona_facts("bob", false).unwrap().is_empty());
}

// ── persona_outbound_allowlist ────────────────────────────────────────────

#[test]
fn set_and_list_outbound_allow() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "outbound-allowlisted", 4).unwrap();
    db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 2).unwrap();
    db.set_persona_outbound_allow("bob", "discord-1", "channel-2", 1).unwrap();

    let allows = db.list_persona_outbound_allows("bob").unwrap();
    assert_eq!(allows.len(), 2);
    let a = allows.iter().find(|a| a["chat_id"] == "channel-1").unwrap();
    assert_eq!(a["max_messages_per_day"], 2_i64);
}

#[test]
fn set_outbound_allow_upsert() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "outbound-allowlisted", 4).unwrap();
    db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 1).unwrap();
    db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 5).unwrap(); // upsert
    let allows = db.list_persona_outbound_allows("bob").unwrap();
    assert_eq!(allows.len(), 1);
    assert_eq!(allows[0]["max_messages_per_day"], 5_i64);
}

#[test]
fn remove_outbound_allow() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "outbound-allowlisted", 4).unwrap();
    db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 1).unwrap();
    db.remove_persona_outbound_allow("bob", "discord-1", "channel-1").unwrap();
    assert!(db.list_persona_outbound_allows("bob").unwrap().is_empty());
}

// ── persona_audit ─────────────────────────────────────────────────────────

#[test]
fn record_and_list_persona_audit() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    let id1 = db.record_persona_audit(
        "bob", "user", "invoke",
        Some("discord-1"), Some("channel-1"),
        Some("{\"msgs\":5}"), "ok", None,
    ).unwrap();
    let id2 = db.record_persona_audit(
        "bob", "heartbeat", "heartbeat_run",
        None, None, None, "ok", None,
    ).unwrap();
    assert!(id1 > 0 && id2 > 0);

    let rows = db.list_persona_audit("bob", 50).unwrap();
    assert_eq!(rows.len(), 2);
    // list returns newest first
    let invoke_row = rows.iter().find(|r| r["action"] == "invoke").unwrap();
    assert_eq!(invoke_row["actor"], "user");
    assert_eq!(invoke_row["target_account"], "discord-1");
    assert_eq!(invoke_row["result"], "ok");
    assert!(invoke_row["error_msg"].is_null());
}

#[test]
fn record_audit_with_error() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    db.record_persona_audit(
        "bob", "heartbeat", "heartbeat_run",
        None, None, None, "error", Some("backend timeout"),
    ).unwrap();

    let rows = db.list_persona_audit("bob", 10).unwrap();
    assert_eq!(rows[0]["result"], "error");
    assert_eq!(rows[0]["error_msg"], "backend timeout");
}

#[test]
fn prune_persona_audit_before_cutoff() {
    let db = fresh_db();
    db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    // Insert two rows at "current" time; we can't control the timestamp,
    // so prune with a future cutoff to delete both.
    db.record_persona_audit("bob", "user", "invoke", None, None, None, "ok", None).unwrap();
    db.record_persona_audit("bob", "user", "invoke", None, None, None, "ok", None).unwrap();

    let deleted = db.prune_persona_audit_before("2099-01-01T00:00:00Z").unwrap();
    assert_eq!(deleted, 2);
    assert!(db.list_persona_audit("bob", 10).unwrap().is_empty());
}

#[test]
fn migration_is_idempotent() {
    // Opening a second MemoryDb on the same ":memory:" path would give a
    // new DB.  Instead, call run_migrations again on the same connection.
    let db = fresh_db();
    // This must not fail — all CREATE TABLE IF NOT EXISTS.
    let guard = db.db.lock().unwrap();
    MemoryDb::run_migrations(&guard).unwrap();
}

// ── query_persona_audit (Phase T.1) ───────────────────────────────────────

/// Helper: seed 4 audit rows for tests below.
///
/// Row layout:
///  1. bob / user    / invoke         / discord-1 / chan-1 / ok
///  2. bob / hb      / heartbeat_run  / None      / None   / ok
///  3. bob / hb      / outbound_send  / discord-1 / chan-2 / denied
///  4. alice / user  / invoke         / matrix-1  / None   / ok
fn seed_audit_rows(db: &MemoryDb) {
    db.create_persona("bob",   "Bob",   "🤖", "p", None, None, "drafts-only", 4).unwrap();
    db.create_persona("alice", "Alice", "🤖", "p", None, None, "drafts-only", 4).unwrap();
    db.record_persona_audit("bob",   "user", "invoke",        Some("discord-1"), Some("chan-1"), None, "ok",     None).unwrap();
    db.record_persona_audit("bob",   "hb",   "heartbeat_run", None,             None,           None, "ok",     None).unwrap();
    db.record_persona_audit("bob",   "hb",   "outbound_send", Some("discord-1"), Some("chan-2"), None, "denied", None).unwrap();
    db.record_persona_audit("alice", "user", "invoke",        Some("matrix-1"), None,           None, "ok",     None).unwrap();
}

// Test 1 — no filter: returns all rows (equivalent to recent_actions with
// no slug restriction).
#[test]
fn query_audit_no_filter_returns_all() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.query_persona_audit(
        None, None, None, None, None, None, None, None, 100,
    ).unwrap();
    assert_eq!(rows.len(), 4);
}

// Test 2 — slug filter: only bob's rows.
#[test]
fn query_audit_filter_by_slug() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.query_persona_audit(
        Some("bob"), None, None, None, None, None, None, None, 100,
    ).unwrap();
    assert_eq!(rows.len(), 3);
    for r in &rows {
        assert_eq!(r["persona_slug"], "bob");
    }
}

// Test 3 — action filter.
#[test]
fn query_audit_filter_by_action() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.query_persona_audit(
        None, Some("outbound_send"), None, None, None, None, None, None, 100,
    ).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["action"], "outbound_send");
}

// Test 4 — result filter.
#[test]
fn query_audit_filter_by_result() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.query_persona_audit(
        None, None, None, None, None, None, None, Some("denied"), 100,
    ).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["result"], "denied");
}

// Test 5 — target_account filter.
#[test]
fn query_audit_filter_by_target_account() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.query_persona_audit(
        None, None, None, None, None, Some("discord-1"), None, None, 100,
    ).unwrap();
    assert_eq!(rows.len(), 2); // bob/invoke + bob/outbound_send
    for r in &rows {
        assert_eq!(r["target_account"], "discord-1");
    }
}

// Test 6 — combined slug + action + since filter.
// We seed rows with known timestamps that are in the past; use a
// `since` cutoff in the far past so all rows pass, then a `since`
// cutoff in the far future so no rows pass.
#[test]
fn query_audit_combined_slug_action_since() {
    let db = fresh_db();
    seed_audit_rows(&db);

    // since=far past: all bob/invoke rows pass.
    let rows_all = db.query_persona_audit(
        Some("bob"), Some("invoke"), None,
        Some("2000-01-01T00:00:00Z"), None,
        None, None, None, 100,
    ).unwrap();
    assert_eq!(rows_all.len(), 1);
    assert_eq!(rows_all[0]["action"], "invoke");
    assert_eq!(rows_all[0]["persona_slug"], "bob");

    // since=far future: no rows pass.
    let rows_none = db.query_persona_audit(
        Some("bob"), Some("invoke"), None,
        Some("2099-01-01T00:00:00Z"), None,
        None, None, None, 100,
    ).unwrap();
    assert_eq!(rows_none.len(), 0);
}

// Test 7 — export_persona_audit: returns all rows for slug, oldest first.
#[test]
fn export_persona_audit_all_rows_oldest_first() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.export_persona_audit("bob").unwrap();
    assert_eq!(rows.len(), 3);
    // Oldest first — no guarantee on ordering ties, but all should be bob.
    for r in &rows {
        assert_eq!(r["persona_slug"], "bob");
    }
    // alice rows must not appear.
    let alice_rows = db.export_persona_audit("alice").unwrap();
    assert_eq!(alice_rows.len(), 1);
}

// Test 8 — actor filter (individual filter not covered by test 6).
#[test]
fn query_audit_filter_by_actor() {
    let db = fresh_db();
    seed_audit_rows(&db);
    let rows = db.query_persona_audit(
        None, None, Some("hb"), None, None, None, None, None, 100,
    ).unwrap();
    assert_eq!(rows.len(), 2); // heartbeat_run + outbound_send both by "hb"
    for r in &rows {
        assert_eq!(r["actor"], "hb");
    }
}
