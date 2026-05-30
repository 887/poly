//! Meta-persona tool handlers (Phase B — persona subsystem).
//!
//! All `handle_meta_persona_*` functions live here. The persona-quality-control
//! lint `forbid_unaudited_persona_tool` scans this file for audit calls.

use crate::memory::MemoryDb;
use crate::state::BackendPool;
use serde_json::Value;

use super::{err_result, ok_result, str_arg};

// ─── Audit helper ─────────────────────────────────────────────────────────────

/// Emit an audit row; swallows errors so failures don't break the primary
/// return path. The tool already returns its result — audit is best-effort.
pub(super) fn audit(
    mem: &MemoryDb,
    slug: &str,
    action: &str,
    payload: Option<&str>,
    result: &str,
    error_msg: Option<&str>,
) {
    drop(mem.record_persona_audit(slug, "claude-desktop", action, None, None, payload, result, error_msg));
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub(super) fn handle_meta_persona_list(mem: &MemoryDb) -> Value {
    match mem.list_personas() {
        Ok(list) => ok_result(serde_json::to_string_pretty(&list).unwrap_or_default()),
        Err(e)   => err_result(format!("meta_persona_list failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_get(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    match mem.get_persona(slug) {
        Ok(Some(p)) => {
            audit(mem, slug, "invoke", Some("{\"action\":\"get\"}"), "ok", None);
            ok_result(serde_json::to_string_pretty(&p).unwrap_or_default())
        }
        Ok(None)    => err_result(format!("persona '{slug}' not found")),
        Err(e)      => err_result(format!("meta_persona_get failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_create(args: &Value, mem: &MemoryDb) -> Value {
    let slug          = match str_arg(args, "slug")          { Some(v) => v, None => return err_result("missing 'slug'") };
    let name          = match str_arg(args, "name")          { Some(v) => v, None => return err_result("missing 'name'") };
    let system_prompt = match str_arg(args, "system_prompt") { Some(v) => v, None => return err_result("missing 'system_prompt'") };

    let avatar_emoji  = str_arg(args, "avatar_emoji").unwrap_or("🤖");
    let style_notes   = str_arg(args, "style_notes");
    let heartbeat     = args.get("heartbeat_interval_secs").and_then(serde_json::Value::as_i64);
    let proactivity   = str_arg(args, "proactivity").unwrap_or("drafts-only");
    let rate_limit    = args.get("rate_limit_per_hour").and_then(serde_json::Value::as_i64).unwrap_or(4);

    match mem.create_persona(slug, name, avatar_emoji, system_prompt, style_notes, heartbeat, proactivity, rate_limit) {
        Ok(s) => {
            audit(mem, &s, "invoke", Some("{\"action\":\"create\"}"), "ok", None);
            ok_result(format!("{{\"slug\":\"{s}\"}}"))
        }
        Err(e) => err_result(format!("meta_persona_create failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_update(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };

    let name          = str_arg(args, "name");
    let avatar_emoji  = str_arg(args, "avatar_emoji");
    let system_prompt = str_arg(args, "system_prompt");

    // style_notes: absent = preserve; null JSON = clear; string = set.
    let style_notes: Option<Option<&str>> = match args.get("style_notes") {
        None => None,
        Some(v) if v.is_null() => Some(None),
        Some(v) => Some(v.as_str()),
    };

    // heartbeat_interval_secs: absent = preserve; null/0 JSON = clear.
    let heartbeat: Option<Option<i64>> = match args.get("heartbeat_interval_secs") {
        None => None,
        Some(v) if v.is_null() => Some(None),
        Some(v) => match v.as_i64() {
            Some(0) | None => Some(None),
            Some(n) => Some(Some(n)),
        },
    };

    let proactivity   = str_arg(args, "proactivity");
    let rate_limit    = args.get("rate_limit_per_hour").and_then(serde_json::Value::as_i64);
    let enabled       = args.get("enabled").and_then(serde_json::Value::as_bool);

    match mem.update_persona(slug, name, avatar_emoji, system_prompt, style_notes, heartbeat, proactivity, rate_limit, enabled, None) {
        Ok(true)  => {
            audit(mem, slug, "invoke", Some("{\"action\":\"update\"}"), "ok", None);
            ok_result(format!("persona '{slug}' updated"))
        }
        Ok(false) => err_result(format!("persona '{slug}' not found")),
        Err(e)    => err_result(format!("meta_persona_update failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_delete(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    // Write the audit row BEFORE deleting (cascade will wipe it otherwise).
    drop(mem.record_persona_audit(slug, "claude-desktop", "invoke", None, None,
        Some("{\"action\":\"delete\"}"), "ok", None));
    match mem.delete_persona(slug) {
        Ok(()) => ok_result(format!("persona '{slug}' deleted")),
        Err(e) => err_result(format!("meta_persona_delete failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_set_sources(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let sources = match args.get("sources").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return err_result("missing or invalid 'sources' (must be array)"),
    };

    // Atomic replace: remove all existing sources, then insert new ones.
    if let Err(e) = mem.list_persona_sources(slug).and_then(|existing| {
        for s in &existing {
            if let Some(id) = s.get("id").and_then(serde_json::Value::as_i64) {
                mem.remove_persona_source(id)?;
            }
        }
        Ok(())
    }) {
        return err_result(format!("meta_persona_set_sources failed clearing old sources: {e}"));
    }

    let mut added = 0usize;
    for src in sources {
        let account_id = match src.get("account_id").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return err_result("source missing 'account_id'"),
        };
        let selector_kind = match src.get("selector_kind").and_then(|v| v.as_str()) {
            Some(k) => k,
            None => return err_result("source missing 'selector_kind'"),
        };
        let selector_value = src.get("selector_value").and_then(|v| v.as_str());
        let include = src.get("include").and_then(serde_json::Value::as_bool).unwrap_or(true);
        if let Err(e) = mem.add_persona_source(slug, account_id, selector_kind, selector_value, include) {
            return err_result(format!("meta_persona_set_sources failed adding source: {e}"));
        }
        added += 1;
    }
    audit(mem, slug, "invoke", Some(&format!("{{\"action\":\"set_sources\",\"count\":{added}}}")), "ok", None);
    ok_result(format!("{added} sources set for persona '{slug}'"))
}

pub(super) fn handle_meta_persona_set_tool_whitelist(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let tools = match args.get("tools").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return err_result("missing or invalid 'tools' (must be array)"),
    };

    // Atomic replace: remove all existing whitelist entries, then add new ones.
    if let Err(e) = mem.list_persona_tools(slug).and_then(|existing| {
        for t in &existing {
            mem.remove_persona_tool(slug, t)?;
        }
        Ok(())
    }) {
        return err_result(format!("meta_persona_set_tool_whitelist failed clearing old tools: {e}"));
    }

    let mut added = 0usize;
    for tool in tools {
        let name = match tool.as_str() {
            Some(n) => n,
            None => return err_result("tool name must be a string"),
        };
        if let Err(e) = mem.add_persona_tool(slug, name) {
            return err_result(format!("meta_persona_set_tool_whitelist failed adding tool: {e}"));
        }
        added += 1;
    }
    audit(mem, slug, "invoke", Some(&format!("{{\"action\":\"set_tool_whitelist\",\"count\":{added}}}")), "ok", None);
    ok_result(format!("{added} tools whitelisted for persona '{slug}'"))
}

/// `meta_persona_invoke` — build the full persona context bundle and return it.
///
/// The invoke audit row fires unconditionally — even in dry_run mode.
/// regardless; suppressing it would remove the only record that the user
/// asked the persona to run, which is always useful for audit purposes.
///
/// Use `dry_run=true` when you want to inspect the bundle shape (e.g. from
/// the e2e harness or a future "preview bundle" UI button) without polluting
/// the persona's audit history with phantom memory reads.
pub(super) async fn handle_meta_persona_invoke(
    args: &Value,
    pool: &BackendPool,
    mem: &MemoryDb,
) -> Value {
    use crate::persona::{PersonaContextRequest, build};
    use crate::persona::context::BackendPoolProvider;

    let slug = match str_arg(args, "slug") {
        Some(v) => v,
        None    => return err_result("missing 'slug'"),
    };
    let user_prompt = str_arg(args, "user_prompt").map(std::string::ToString::to_string);

    // Parse the dry_run flag (default: false).
    let dry_run = args.get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Verify the persona exists and is enabled.
    let persona = match mem.get_persona(slug) {
        Ok(Some(p)) => p,
        Ok(None)    => {
            drop(mem.record_persona_audit(slug, "claude-desktop", "invoke", None, None,
                None, "error", Some("persona not found")));
            return err_result(format!("persona '{slug}' not found"));
        }
        Err(e) => return err_result(format!("meta_persona_invoke failed: {e}")),
    };

    if persona.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
        drop(mem.record_persona_audit(slug, "claude-desktop", "invoke", None, None,
            None, "denied", Some("persona disabled")));
        return err_result(format!("persona '{slug}' is disabled"));
    }

    // Parse optional tuning parameters.
    let max_messages_per_chat = args.get("max_messages_per_chat")
        .and_then(serde_json::Value::as_u64)
        .map_or(30, |v| usize::try_from(v.clamp(1, 200)).unwrap_or(200));
    let max_chats = args.get("max_chats")
        .and_then(serde_json::Value::as_u64)
        .map_or(25, |v| usize::try_from(v.clamp(1, 100)).unwrap_or(100));
    let include_summaries = args.get("include_summaries")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);

    let req = PersonaContextRequest {
        slug: slug.to_string(),
        user_prompt: user_prompt.clone(),
        max_messages_per_chat,
        max_chats,
        include_summaries,
        dry_run,
    };

    let provider = BackendPoolProvider { pool };

    match build(req, mem, &provider).await {
        Ok(bundle) => {
            let payload_str = format!(
                "{{\"action\":\"invoke\",\"user_prompt\":{},\"dry_run\":{dry_run}}}",
                user_prompt
                    .as_deref().map_or_else(|| "null".to_string(), |p| format!("{p:?}")),
            );
            // The invoke audit row fires unconditionally — even in dry_run mode.
            // This is intentional: the invoke row records that the user asked the
            // persona to run, which is always relevant.  Only the per-chat
            // memory_read rows (written by PersonaContextBuilder::build()) are
            // suppressed when dry_run=true.
            audit(mem, slug, "invoke", Some(&payload_str), "ok", None);
            ok_result(serde_json::to_string_pretty(&bundle).unwrap_or_default())
        }
        Err(e) => {
            let msg = e.to_string();
            drop(mem.record_persona_audit(
                slug, "claude-desktop", "invoke", None, None, None, "error", Some(&msg),
            ));
            err_result(format!("meta_persona_invoke build failed: {msg}"))
        }
    }
}

pub(super) fn handle_meta_persona_set_heartbeat(args: &Value, mem: &MemoryDb) -> Value {
    let slug = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };

    // interval_secs: null or 0 → disable (None); positive integer → set.
    let interval: Option<Option<i64>> = match args.get("interval_secs") {
        None => None,   // absent — don't change
        Some(v) if v.is_null() => Some(None),
        Some(v) => {
            let n = v.as_i64().unwrap_or(0);
            Some(if n == 0 { None } else { Some(n) })
        }
    };

    match mem.update_persona(slug, None, None, None, None, interval, None, None, None, None) {
        Ok(true) => {
            let payload = match interval {
                Some(Some(n)) => format!("{{\"action\":\"set_heartbeat\",\"interval_secs\":{n}}}"),
                _             => "{\"action\":\"set_heartbeat\",\"interval_secs\":null}".to_string(),
            };
            audit(mem, slug, "invoke", Some(&payload), "ok", None);
            ok_result("heartbeat updated")
        }
        Ok(false) => err_result(format!("persona '{slug}' not found")),
        Err(e)    => err_result(format!("meta_persona_set_heartbeat failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_get_memory(args: &Value, mem: &MemoryDb) -> Value {
    let slug        = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let pinned_only = args.get("pinned_only").and_then(serde_json::Value::as_bool).unwrap_or(false);

    match mem.list_persona_facts(slug, pinned_only) {
        Ok(facts) => {
            audit(mem, slug, "memory_read", Some("{\"action\":\"get_memory\"}"), "ok", None);
            ok_result(serde_json::to_string_pretty(&facts).unwrap_or_default())
        }
        Err(e) => err_result(format!("meta_persona_get_memory failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_set_memory(args: &Value, mem: &MemoryDb) -> Value {
    let slug      = match str_arg(args, "slug")      { Some(v) => v, None => return err_result("missing 'slug'") };
    let fact_text = match str_arg(args, "fact_text") { Some(v) => v, None => return err_result("missing 'fact_text'") };

    let category = str_arg(args, "category");
    let pinned   = args.get("pinned").and_then(serde_json::Value::as_bool).unwrap_or(false);

    match mem.add_persona_fact(slug, category, fact_text, pinned) {
        Ok(id) => {
            audit(mem, slug, "memory_write", Some(&format!("{{\"action\":\"set_memory\",\"fact_id\":{id}}}")), "ok", None);
            ok_result(format!("{{\"fact_id\":{id}}}"))
        }
        Err(e) => err_result(format!("meta_persona_set_memory failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_forget_memory(args: &Value, mem: &MemoryDb) -> Value {
    let slug       = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let forget_all = args.get("forget_all").and_then(serde_json::Value::as_bool).unwrap_or(false);

    if forget_all {
        match mem.forget_all_persona_facts(slug) {
            Ok(()) => {
                audit(mem, slug, "memory_write", Some("{\"action\":\"forget_all_memory\"}"), "ok", None);
                ok_result(format!("all facts for persona '{slug}' deleted"))
            }
            Err(e) => err_result(format!("meta_persona_forget_memory failed: {e}")),
        }
    } else {
        let fact_id = match args.get("fact_id").and_then(serde_json::Value::as_i64) {
            Some(id) => id,
            None => return err_result("must provide 'fact_id' or set 'forget_all': true"),
        };
        match mem.remove_persona_fact(fact_id) {
            Ok(()) => {
                audit(mem, slug, "memory_write",
                    Some(&format!("{{\"action\":\"forget_memory\",\"fact_id\":{fact_id}}}")), "ok", None);
                ok_result(format!("fact {fact_id} deleted"))
            }
            Err(e) => err_result(format!("meta_persona_forget_memory failed: {e}")),
        }
    }
}

pub(super) fn handle_meta_persona_recent_actions(args: &Value, mem: &MemoryDb) -> Value {
    // poly-lint: allow unaudited-persona-tool — read-only query of own audit log; no audit row needed.
    let slug  = match str_arg(args, "slug") { Some(v) => v, None => return err_result("missing 'slug'") };
    let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(50).clamp(1, 500);

    match mem.list_persona_audit(slug, limit) {
        Ok(rows) => ok_result(serde_json::to_string_pretty(&rows).unwrap_or_default()),
        Err(e)   => err_result(format!("meta_persona_recent_actions failed: {e}")),
    }
}

pub(super) fn handle_meta_persona_set_outbound_allow(args: &Value, mem: &MemoryDb) -> Value {
    let slug       = match str_arg(args, "slug")       { Some(v) => v, None => return err_result("missing 'slug'") };
    let account_id = match str_arg(args, "account_id") { Some(v) => v, None => return err_result("missing 'account_id'") };
    let chat_id    = match str_arg(args, "chat_id")    { Some(v) => v, None => return err_result("missing 'chat_id'") };
    let remove     = args.get("remove").and_then(serde_json::Value::as_bool).unwrap_or(false);

    if remove {
        match mem.remove_persona_outbound_allow(slug, account_id, chat_id) {
            Ok(()) => {
                audit(mem, slug, "invoke", Some("{\"action\":\"remove_outbound_allow\"}"), "ok", None);
                ok_result(format!("outbound allow entry removed for {account_id}/{chat_id}"))
            }
            Err(e) => err_result(format!("meta_persona_set_outbound_allow remove failed: {e}")),
        }
    } else {
        let max_per_day = args.get("max_messages_per_day")
            .and_then(serde_json::Value::as_i64).unwrap_or(1).clamp(1, 100);
        match mem.set_persona_outbound_allow(slug, account_id, chat_id, max_per_day) {
            Ok(()) => {
                let payload = format!(
                    "{{\"action\":\"set_outbound_allow\",\"account_id\":\"{account_id}\",\"chat_id\":\"{chat_id}\",\"max_per_day\":{max_per_day}}}",
                );
                audit(mem, slug, "invoke", Some(&payload), "ok", None);
                ok_result(format!("outbound allow set for {account_id}/{chat_id} max={max_per_day}/day"))
            }
            Err(e) => err_result(format!("meta_persona_set_outbound_allow failed: {e}")),
        }
    }
}

// ─── Phase T — audit surface handlers ────────────────────────────────────────

/// `meta_persona_audit_query` — filtered query over persona_audit.
///
/// Read-only; no audit row emitted (auditing a read of the audit log would be
/// circular).  Listed in `unaudited-persona-tool-allowlist.txt`.
pub(super) fn handle_meta_persona_audit_query(args: &Value, mem: &MemoryDb) -> Value {
    // poly-lint: allow unaudited-persona-tool — read-only audit query; circular to audit an audit read.
    let slug           = str_arg(args, "slug");
    let action         = str_arg(args, "action");
    let actor          = str_arg(args, "actor");
    let since          = str_arg(args, "since");
    let until          = str_arg(args, "until");
    let target_account = str_arg(args, "target_account");
    let target_chat    = str_arg(args, "target_chat");
    let result         = str_arg(args, "result");
    let limit          = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(100).clamp(1, 500);

    match mem.query_persona_audit(
        slug,
        action,
        actor,
        since,
        until,
        target_account,
        target_chat,
        result,
        limit,
    ) {
        Ok(rows) => ok_result(serde_json::to_string_pretty(&rows).unwrap_or_default()),
        Err(e)   => err_result(format!("meta_persona_audit_query failed: {e}")),
    }
}

/// `meta_persona_audit_export` — full audit history for a persona as JSONL.
///
/// Read-only; no audit row emitted.  Listed in `unaudited-persona-tool-allowlist.txt`.
pub(super) fn handle_meta_persona_audit_export(args: &Value, mem: &MemoryDb) -> Value {
    // poly-lint: allow unaudited-persona-tool — read-only export; no audit row needed for a read.
    let slug = match str_arg(args, "slug") {
        Some(v) => v,
        None    => return err_result("missing 'slug'"),
    };

    match mem.export_persona_audit(slug) {
        Ok(rows) => {
            // Serialise as JSONL (one JSON object per line).
            let jsonl: String = rows
                .iter()
                .filter_map(|r| serde_json::to_string(r).ok())
                .collect::<Vec<_>>()
                .join("\n");
            ok_result(jsonl)
        }
        Err(e) => err_result(format!("meta_persona_audit_export failed: {e}")),
    }
}
