//! Draft queue tool handlers (Phase B).

use crate::memory::MemoryDb;
use crate::state::BackendPool;
use poly_client::MessageContent;
use serde_json::Value;

use super::{err_result, ok_result, str_arg};

/// Helper: compute ISO-8601 UTC timestamp for `now + secs`.
// poly-lint: textbook Gregorian-calendar arithmetic on u64 timestamp.
#[allow(clippy::arithmetic_side_effects, clippy::integer_division)]
pub(super) fn future_iso8601(secs: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let total = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_add(secs);
    let s = total % 60;
    let m = (total / 60) % 60;
    let h = (total / 3600) % 24;
    let days = total / 86400;

    // Reuse the Gregorian calendar helper from memory.rs via a local copy.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

pub(super) async fn handle_draft_create(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let account_id   = match str_arg(args, "account_id")   { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id      = match str_arg(args, "chat_id")      { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let body         = match str_arg(args, "body")         { Some(v) => v, None => return err_result("missing 'body'") };
    let suggested_by = match str_arg(args, "suggested_by") { Some(v) => v, None => return err_result("missing 'suggested_by'") };

    // Sanitize body: trim leading/trailing whitespace; reject empty.
    let body = body.trim();
    if body.is_empty() {
        return err_result("draft body must not be empty");
    }

    // Per-chat auto-approve KV key: "agent.chat.{account_id}.{chat_id}.auto_approve_enabled"
    // We check a synthetic pool-level setting. Since pool has no KV store itself,
    // the auto-send feature is gated on the caller explicitly passing auto_send_in_secs
    // AND the backend being writable (as a safety proxy).
    let auto_send_in_secs = args.get("auto_send_in_secs").and_then(serde_json::Value::as_u64);

    // Only honour auto_send_in_secs when the backend is writable (sanity gate).
    let auto_send_at: Option<String> = if let Some(secs) = auto_send_in_secs {
        let is_writable = pool.find_by_account(account_id)
            .is_some_and(|e| {
                let caps = poly_client::capabilities_for_slug_static(
                    &format!("{:?}", e.session.backend)
                );
                caps.composer_writable()
            });
        if is_writable {
            Some(future_iso8601(secs))
        } else {
            None
        }
    } else {
        None
    };

    match mem.draft_insert(account_id, chat_id, body, suggested_by, auto_send_at.as_deref()) {
        Ok(id) => ok_result(format!("{{\"draft_id\":{id}}}")),
        Err(e) => err_result(format!("draft_create failed: {e}")),
    }
}

pub(super) fn handle_draft_list(args: &Value, mem: &MemoryDb) -> Value {
    let account_id = str_arg(args, "account_id");
    let chat_id    = str_arg(args, "chat_id");
    let status     = str_arg(args, "status");

    match mem.draft_list(account_id, chat_id, status) {
        Ok(drafts) => ok_result(serde_json::to_string_pretty(&drafts).unwrap_or_default()),
        Err(e)     => err_result(format!("draft_list failed: {e}")),
    }
}

pub(super) async fn handle_draft_approve(args: &Value, pool: &BackendPool, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };

    // Fetch the draft.
    let draft = match mem.draft_get(draft_id) {
        Ok(Some(d)) => d,
        Ok(None)    => return err_result(format!("draft {draft_id} not found")),
        Err(e)      => return err_result(format!("draft_approve failed: {e}")),
    };

    let status = draft.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status != "pending" {
        return err_result(format!("draft {draft_id} has status={status}, must be pending to approve"));
    }

    let account_id = draft.get("account_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let chat_id    = draft.get("chat_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let body       = draft.get("body").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Send via the active backend.
    // Verify the backend exists before attempting send.
    let entry = match pool.find_by_account(&account_id) {
        Some(e) => e,
        None    => return err_result(format!("no backend found for account_id={account_id}")),
    };

    match entry.backend.send_message(&chat_id, MessageContent::Text(body)).await {
        Ok(_) => {
            if let Err(e) = mem.draft_set_status(draft_id, "sent") {
                return err_result(format!("message sent but failed to update draft status: {e}"));
            }
            ok_result(format!("draft {draft_id} sent and status updated to sent"))
        }
        Err(e) => {
            drop(mem.draft_set_status(draft_id, "expired"));
            err_result(format!("send_message failed: {e}; draft marked expired"))
        }
    }
}

pub(super) fn handle_draft_edit(args: &Value, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };
    let new_body = match str_arg(args, "new_body") {
        Some(b) => b.trim(),
        None    => return err_result("missing 'new_body'"),
    };
    if new_body.is_empty() {
        return err_result("new_body must not be empty");
    }

    match mem.draft_edit(draft_id, new_body) {
        Ok(true)  => ok_result(format!("draft {draft_id} body updated")),
        Ok(false) => err_result(format!("draft {draft_id} not found or not in pending status")),
        Err(e)    => err_result(format!("draft_edit failed: {e}")),
    }
}

pub(super) fn handle_draft_discard(args: &Value, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };

    match mem.draft_set_status(draft_id, "discarded") {
        Ok(())  => ok_result(format!("draft {draft_id} discarded")),
        Err(e)  => err_result(format!("draft_discard failed: {e}")),
    }
}

pub(super) fn handle_draft_cancel_autosend(args: &Value, mem: &MemoryDb) -> Value {
    let draft_id = match args.get("draft_id").and_then(serde_json::Value::as_i64) {
        Some(id) => id,
        None     => return err_result("missing 'draft_id'"),
    };

    match mem.draft_clear_autosend(draft_id) {
        Ok(())  => ok_result(format!("auto-send cancelled for draft {draft_id}")),
        Err(e)  => err_result(format!("draft_cancel_autosend failed: {e}")),
    }
}
