//! Memory tool handlers: contact facts, chat notes, summaries, context bundler.

use crate::memory::MemoryDb;
use crate::state::BackendPool;
use serde_json::Value;

use super::{err_result, ok_result, str_arg};
use super::chat::find_backend;

// ─── Phase A — contact facts ──────────────────────────────────────────────────

pub(super) fn handle_remember_fact(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let contact_id = match str_arg(args, "contact_id") { Some(v) => v, None => return err_result("missing 'contact_id'") };
    let category   = str_arg(args, "category").unwrap_or("");
    let fact       = match str_arg(args, "fact") { Some(v) => v, None => return err_result("missing 'fact'") };
    match mem.remember_fact(account_id, contact_id, category, fact) {
        Ok(id) => ok_result(serde_json::json!({ "fact_id": id }).to_string()),
        Err(e) => err_result(format!("remember_fact failed: {e}")),
    }
}

pub(super) fn handle_recall_facts(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let contact_id = match str_arg(args, "contact_id") { Some(v) => v, None => return err_result("missing 'contact_id'") };
    let category   = str_arg(args, "category");
    match mem.recall_facts(account_id, contact_id, category) {
        Ok(facts) => ok_result(serde_json::to_string_pretty(&facts).unwrap_or_default()),
        Err(e) => err_result(format!("recall_facts failed: {e}")),
    }
}

pub(super) fn handle_forget_fact(args: &Value, mem: &MemoryDb) -> Value {
    let fact_id = match args.get("fact_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None => return err_result("missing or invalid 'fact_id' (must be integer)"),
    };
    match mem.forget_fact(fact_id) {
        Ok(()) => ok_result("fact deleted"),
        Err(e) => err_result(format!("forget_fact failed: {e}")),
    }
}

pub(super) fn handle_search_facts(args: &Value, mem: &MemoryDb) -> Value {
    let query      = match str_arg(args, "query") { Some(v) => v, None => return err_result("missing 'query'") };
    let account_id = str_arg(args, "account_id");
    match mem.search_facts(query, account_id) {
        Ok(facts) => ok_result(serde_json::to_string_pretty(&facts).unwrap_or_default()),
        Err(e) => err_result(format!("search_facts failed: {e}")),
    }
}

// ─── Chat notes ───────────────────────────────────────────────────────────────

pub(super) fn handle_store_chat_note(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let note       = match str_arg(args, "note")       { Some(v) => v, None => return err_result("missing 'note'") };
    match mem.store_chat_note(account_id, chat_id, note) {
        Ok(id) => ok_result(serde_json::json!({ "note_id": id }).to_string()),
        Err(e) => err_result(format!("store_chat_note failed: {e}")),
    }
}

pub(super) fn handle_get_chat_notes(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.get_chat_notes(account_id, chat_id) {
        Ok(notes) => ok_result(serde_json::to_string_pretty(&notes).unwrap_or_default()),
        Err(e) => err_result(format!("get_chat_notes failed: {e}")),
    }
}

pub(super) fn handle_forget_chat_note(args: &Value, mem: &MemoryDb) -> Value {
    let note_id = match args.get("note_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None => return err_result("missing or invalid 'note_id' (must be integer)"),
    };
    match mem.forget_chat_note(note_id) {
        Ok(()) => ok_result("note deleted"),
        Err(e) => err_result(format!("forget_chat_note failed: {e}")),
    }
}

// ─── Chat summaries ───────────────────────────────────────────────────────────

pub(super) fn handle_store_chat_summary(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let summary    = match str_arg(args, "summary")    { Some(v) => v, None => return err_result("missing 'summary'") };
    let window_start = str_arg(args, "window_start_msg_id").unwrap_or("");
    let window_end   = str_arg(args, "window_end_msg_id").unwrap_or("");
    match mem.store_chat_summary(account_id, chat_id, summary, window_start, window_end) {
        Ok(()) => ok_result("summary stored"),
        Err(e) => err_result(format!("store_chat_summary failed: {e}")),
    }
}

pub(super) fn handle_get_chat_summary(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    match mem.get_chat_summary(account_id, chat_id) {
        Ok(Some(s)) => ok_result(serde_json::to_string_pretty(&s).unwrap_or_default()),
        Ok(None)    => ok_result("null"),
        Err(e)      => err_result(format!("get_chat_summary failed: {e}")),
    }
}

// ─── Phase A.3 — Context bundler ──────────────────────────────────────────────

/// Build the fat reply-context bundle that gives Claude Desktop everything it
/// needs to draft a contextually-aware reply in a single MCP call.
pub(super) async fn handle_get_reply_context(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let message_limit = u32::try_from(args.get("message_limit").and_then(serde_json::Value::as_u64).unwrap_or(20)).unwrap_or(u32::MAX);
    let contact_id = str_arg(args, "contact_id");

    // Find the backend for this account.
    let entry = match find_backend(args, pool) {
        Ok(e) => e,
        Err(v) => return v,
    };

    // Section: account info.
    let account_section = serde_json::json!({
        "id":           entry.session.user.id,
        "backend":      format!("{:?}", entry.session.backend),
        "display_name": entry.session.user.display_name,
    });

    // Section: recent messages (best-effort; null on error).
    let recent_messages: Value = match entry
        .backend
        .get_messages(
            chat_id,
            poly_client::MessageQuery {
                limit: Some(message_limit),
                ..Default::default()
            },
        )
        .await
    {
        Ok(msgs) => serde_json::to_value(&msgs).unwrap_or(serde_json::json!([])),
        Err(_) => serde_json::json!([]),
    };

    // Section: contact info + facts (null if no contact_id supplied or lookup fails).
    let contact_section: Value = if let Some(cid) = contact_id {
        let user_info: Option<Value> = match entry.backend.get_user(cid).await {
            Ok(u) => serde_json::to_value(&u).ok(),
            Err(_) => None,
        };
        let facts = mem.recall_facts(account_id, cid, None).unwrap_or_default();
        let mut obj = serde_json::Map::new();
        obj.insert("id".to_string(), serde_json::json!(cid));
        if let Some(u) = user_info {
            obj.insert("display_name".to_string(), u.get("display_name").cloned().unwrap_or(serde_json::json!(null)));
            obj.insert("presence".to_string(), u.get("presence").cloned().unwrap_or(serde_json::json!(null)));
            obj.insert("last_seen".to_string(), u.get("last_seen").cloned().unwrap_or(serde_json::json!(null)));
        }
        obj.insert("facts".to_string(), serde_json::json!(facts));
        serde_json::json!(obj)
    } else {
        serde_json::json!(null)
    };

    // Section: chat notes.
    let chat_notes: Value = mem
        .get_chat_notes(account_id, chat_id)
        .map(|n| serde_json::json!(n))
        .unwrap_or(serde_json::json!([]));

    // Section: chat summary.
    let chat_summary: Value = mem
        .get_chat_summary(account_id, chat_id)
        .ok()
        .flatten()
        .unwrap_or(serde_json::json!(null));

    // Section: per-chat style (Phase E).
    let chat_style: Value = mem
        .get_chat_style(account_id, chat_id)
        .ok()
        .flatten()
        .unwrap_or(serde_json::json!(null));

    let bundle = serde_json::json!({
        "account":         account_section,
        "chat":            { "id": chat_id },
        "recent_messages": recent_messages,
        "contact":         contact_section,
        "chat_notes":      chat_notes,
        "chat_summary":    chat_summary,
        "style":           chat_style,
    });

    ok_result(serde_json::to_string_pretty(&bundle).unwrap_or_default())
}
