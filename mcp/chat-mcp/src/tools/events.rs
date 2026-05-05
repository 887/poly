//! Event subscription/poll handlers (Phase C) + typing simulation (Phase D)
//! + unread summary bundler (Phase F).

use crate::events::{Subscription, new_subscription_id, parse_opt_event_kinds, parse_opt_string_vec};
use crate::state::BackendPool;
use serde_json::{Value, json};

use super::{err_result, ok_result, str_arg};

// ─── Phase C — event subscription / poll ────────────────────────────────────

pub(super) async fn handle_subscribe_events(args: &Value, pool: &BackendPool) -> Value {
    let account_ids = parse_opt_string_vec(args, "account_ids");
    let chat_ids = parse_opt_string_vec(args, "chat_ids");
    let event_types = parse_opt_event_kinds(args, "event_types");

    let id = new_subscription_id();
    let sub = Subscription {
        id: id.clone(),
        account_ids,
        chat_ids,
        event_types,
    };

    pool.events.lock().await.add_subscription(sub);

    ok_result(serde_json::to_string_pretty(&json!({
        "subscription_id": id,
        "note": "Use poll_events with this subscription_id to retrieve matching events."
    })).unwrap_or_default())
}

pub(super) async fn handle_unsubscribe_events(args: &Value, pool: &BackendPool) -> Value {
    let sub_id = match args.get("subscription_id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_result("missing 'subscription_id'"),
    };
    pool.events.lock().await.remove_subscription(sub_id);
    ok_result(format!("subscription {sub_id} removed"))
}

/// Maximum events returned per poll call.
const MAX_POLL_LIMIT: usize = 500;
const DEFAULT_POLL_LIMIT: usize = 100;

pub(super) async fn handle_poll_events(args: &Value, pool: &BackendPool) -> Value {
    let since_ms = args
        .get("since_ms")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .map_or(DEFAULT_POLL_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX))
        .min(MAX_POLL_LIMIT);

    let store = pool.events.lock().await;

    let events = if let Some(sub_id) = args.get("subscription_id").and_then(|v| v.as_str()) {
        match store.poll(sub_id, since_ms, limit) {
            Ok(evs) => evs,
            Err(e) => return err_result(e),
        }
    } else {
        let account_ids = parse_opt_string_vec(args, "account_ids");
        let chat_ids = parse_opt_string_vec(args, "chat_ids");
        let event_types = parse_opt_event_kinds(args, "event_types");
        store.poll_adhoc(
            account_ids.as_deref(),
            chat_ids.as_deref(),
            event_types.as_deref(),
            since_ms,
            limit,
        )
    };

    let next_since_ms = events.iter().map(|e| e.seq_ms).max().unwrap_or(since_ms);

    ok_result(serde_json::to_string_pretty(&json!({
        "events": events,
        "count": events.len(),
        "next_since_ms": next_since_ms,
    })).unwrap_or_default())
}

// ─── Phase D — typing simulation ────────────────────────────────────────────

/// Phase D — Start a typing-simulation worker. Clones the backend Arc,
/// spawns the rhythm loop, and registers the sim in the pool's registry.
pub(super) async fn handle_start_typing_simulation(args: &Value, pool: &BackendPool) -> Value {
    let account_id = match str_arg(args, "account_id") {
        Some(v) => v,
        None => return err_result("missing 'account_id'"),
    };
    let chat_id = match str_arg(args, "chat_id") {
        Some(v) => v,
        None => return err_result("missing 'chat_id'"),
    };

    // Find the backend for this account. Cloning the Arc gives the worker
    // an independent handle for the lifetime of the simulation.
    let entry = match pool.find_by_account(account_id) {
        Some(e) => e,
        None => return err_result(format!("no backend for account '{account_id}'")),
    };
    if !entry.backend.backend_capabilities().supports_typing_indicators {
        return err_result("backend does not support typing indicators");
    }
    let backend_arc = entry.backend.clone();

    // poly-lint: probabilities are f64→f32 by API contract; truncation is acceptable in [0,1] range.
    #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
    let params = crate::typing_simulation::SimParams::clamped(
        u32::try_from(args.get("total_duration_ms").and_then(Value::as_u64).unwrap_or(8_000)).unwrap_or(u32::MAX),
        u16::try_from(args.get("avg_wpm").and_then(Value::as_u64).unwrap_or(60)).unwrap_or(u16::MAX),
        args.get("false_start_probability").and_then(Value::as_f64).unwrap_or(0.05_f64) as f32,
        args.get("pause_probability").and_then(Value::as_f64).unwrap_or(0.10_f64) as f32,
        args.get("stop_on_other_typing").and_then(Value::as_bool).unwrap_or(false),
    );

    // Seed the RNG from the current system clock so simulations feel fresh
    // between invocations. Unit tests use fixed seeds via
    // `next_tick_decision` directly, not this path.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0xCAFE_u64);

    let stop_on_other_typing = params.stop_on_other_typing;
    let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
    let handle = crate::typing_simulation::spawn_worker(
        backend_arc,
        chat_id.to_string(),
        params,
        seed,
        abort_rx,
    );

    let mut registry = pool.sim_registry.lock().await;
    let sim_id = match registry.start(account_id, chat_id, handle, abort_tx) {
        Ok(id) => id,
        Err(e) => return err_result(e),
    };
    drop(registry);

    // Phase D ↔ Phase C bridge — when stop_on_other_typing is true, watch the
    // event broadcast for a TypingStarted on this channel and abort the
    // simulation by removing it from the registry (which drops abort_tx +
    // aborts the JoinHandle).
    if stop_on_other_typing {
        let registry = pool.sim_registry.clone();
        let mut events_rx = pool.events.lock().await.subscribe_broadcast();
        let watch_chat_id = chat_id.to_string();
        let watch_sim_id = sim_id.clone();
        tokio::spawn(async move {
            use crate::events::EventKind;
            while let Ok(event) = events_rx.recv().await {
                if event.kind != EventKind::TypingStarted {
                    continue;
                }
                if event.channel_id.as_deref() != Some(watch_chat_id.as_str()) {
                    continue;
                }
                let mut reg = registry.lock().await;
                reg.stop(&watch_sim_id);
                break;
            }
        });
    }

    ok_result(
        serde_json::to_string_pretty(&json!({
            "simulation_id": sim_id,
            "account_id": account_id,
            "chat_id": chat_id,
        }))
        .unwrap_or_default(),
    )
}

/// Phase D — Stop an in-flight simulation. Returns `found: true` if the
/// id matched; `false` if the simulation had already expired naturally.
pub(super) async fn handle_stop_typing_simulation(args: &Value, pool: &BackendPool) -> Value {
    let sim_id = match str_arg(args, "simulation_id") {
        Some(v) => v,
        None => return err_result("missing 'simulation_id'"),
    };
    let mut registry = pool.sim_registry.lock().await;
    let found = registry.stop(sim_id);
    ok_result(
        serde_json::to_string_pretty(&json!({
            "simulation_id": sim_id,
            "found": found,
        }))
        .unwrap_or_default(),
    )
}

// ─── Phase F — unread summary bundler ────────────────────────────────────────

/// Phase F — Bundle recent activity across every chat for an account, ordered
/// by most-recent-first, so Claude Desktop can compose a "catch me up" digest
/// in one MCP round-trip. Stays LLM-free — the bundler just returns structured
/// context; the summary generation happens Claude-side.
pub(super) async fn handle_get_unread_summary(args: &Value, pool: &BackendPool) -> Value {
    let account_id = match str_arg(args, "account_id") {
        Some(v) => v,
        None => return err_result("missing 'account_id'"),
    };
    let message_limit = u32::try_from(args
        .get("message_limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(10)).unwrap_or(u32::MAX);

    let entry = match pool.find_by_account(account_id) {
        Some(e) => e,
        None => return err_result(format!("no backend for account '{account_id}'")),
    };

    // Gather servers + channels, pull recent messages from each channel with
    // unread_count > 0. Best-effort; skip channels that error.
    let servers = entry.backend.get_servers().await.unwrap_or_default();
    let mut per_chat_bundles: Vec<Value> = Vec::new();

    for server in &servers {
        let channels = entry
            .backend
            .get_channels(&server.id)
            .await
            .unwrap_or_default();
        for channel in channels {
            if channel.unread_count == 0 {
                continue;
            }
            let messages = entry
                .backend
                .get_messages(
                    &channel.id,
                    poly_client::MessageQuery {
                        limit: Some(message_limit),
                        ..Default::default()
                    },
                )
                .await
                .ok()
                .unwrap_or_default();
            per_chat_bundles.push(json!({
                "kind": "channel",
                "server": { "id": server.id, "name": server.name },
                "channel": { "id": channel.id, "name": channel.name, "unread_count": channel.unread_count },
                "recent_messages": messages,
            }));
        }
    }

    // DMs with unread messages.
    let dms = entry.backend.get_dm_channels().await.unwrap_or_default();
    for dm in dms {
        if dm.unread_count == 0 {
            continue;
        }
        let messages = entry
            .backend
            .get_messages(
                &dm.id,
                poly_client::MessageQuery {
                    limit: Some(message_limit),
                    ..Default::default()
                },
            )
            .await
            .ok()
            .unwrap_or_default();
        per_chat_bundles.push(json!({
            "kind": "dm",
            "contact": { "id": dm.user.id, "name": dm.user.display_name },
            "dm_channel_id": dm.id,
            "unread_count": dm.unread_count,
            "recent_messages": messages,
        }));
    }

    ok_result(
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "unread_chat_count": per_chat_bundles.len(),
            "chats": per_chat_bundles,
        }))
        .unwrap_or_default(),
    )
}
