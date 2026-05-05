//! Client-settings tool handlers (Phase D — client-version plan).
//!
//! These handlers use `ClientConfigStore` from `poly_host_bridge`, which wraps
//! the `/host/kv/*` bridge routes. In tests (no live bridge server), the store's
//! `kv_*` calls will fail — tests that exercise these handlers spin up a full
//! host-bridge mock or use `MemoryDb`-only assertions for the audit path.
//!
//! Backend IDs are hardcoded (10 slugs) in `client_settings_list` to avoid
//! taking a live `BackendPool` dependency in the schema layer. New backends must
//! be added here when they are added to `state.rs::create_backend`.

use crate::memory::MemoryDb;
use crate::state::BackendPool;
use serde_json::Value;

use super::{err_result, ok_result, str_arg};

/// The 10 known backend slugs for `client_settings_list` enumeration.
pub(super) const CLIENT_SETTINGS_BACKENDS: &[&str] = &[
    "stoat", "matrix", "lemmy", "hackernews", "discord",
    "teams", "poly", "github", "forgejo", "demo",
];

/// Emit a client-settings audit row; swallows errors so failures don't break
/// the primary return path.
pub(super) fn audit_client_settings(
    mem: &MemoryDb,
    backend_id: &str,
    action: &str,
    payload: Option<&str>,
    status: &str,
    error_msg: Option<&str>,
) {
    drop(mem.record_client_settings_audit(backend_id, action, payload, status, error_msg));
}

pub(super) async fn handle_client_settings_list(
    args: &Value,
    pool: &BackendPool,
    _mem: &MemoryDb,
) -> Value {
    // poly-lint: allow unaudited-client-settings-tool — read-only; no audit needed.
    let store = &pool.config_store;

    if let Some(bid) = str_arg(args, "backend_id") {
        // Single backend snapshot.
        match store.list_overrides(bid).await {
            Ok(snap) => ok_result(serde_json::to_string_pretty(&snap).unwrap_or_default()),
            Err(e)   => {
                // If host bridge is not reachable, return a zero-state snapshot
                // so callers can still reason about the backend.
                let snap = serde_json::json!({
                    "backend_id": bid,
                    "version_override": null,
                    "mechanisms": [],
                    "_error": format!("host bridge unavailable: {e}")
                });
                ok_result(serde_json::to_string_pretty(&snap).unwrap_or_default())
            }
        }
    } else {
        // All 10 known backends.
        let mut results = Vec::with_capacity(CLIENT_SETTINGS_BACKENDS.len());
        for bid in CLIENT_SETTINGS_BACKENDS {
            let snap = match store.list_overrides(bid).await {
                Ok(s) => serde_json::to_value(s).unwrap_or_default(),
                Err(e) => serde_json::json!({
                    "backend_id": bid,
                    "version_override": null,
                    "mechanisms": [],
                    "_error": format!("host bridge unavailable: {e}")
                }),
            };
            results.push(snap);
        }
        ok_result(serde_json::to_string_pretty(&results).unwrap_or_default())
    }
}

pub(super) async fn handle_client_settings_get_version(
    args: &Value,
    pool: &BackendPool,
    _mem: &MemoryDb,
) -> Value {
    // poly-lint: allow unaudited-client-settings-tool — read-only; no audit needed.
    let bid = match str_arg(args, "backend_id") {
        Some(v) => v,
        None => return err_result("missing 'backend_id'"),
    };
    let store = &pool.config_store;
    match store.get_version_override(bid).await {
        Ok(Some(ov)) => ok_result(serde_json::json!({
            "backend_id": bid,
            "effective_version": &ov,
            "source": "override",
            "override": &ov,
        }).to_string()),
        Ok(None) => ok_result(serde_json::json!({
            "backend_id": bid,
            "effective_version": null,
            "source": "default",
            "override": null,
        }).to_string()),
        Err(e) => err_result(format!("client_settings_get_version failed: {e}")),
    }
}

pub(super) async fn handle_client_settings_set_version_override(
    args: &Value,
    pool: &BackendPool,
    mem: &MemoryDb,
) -> Value {
    let bid = match str_arg(args, "backend_id") {
        Some(v) => v,
        None => return err_result("missing 'backend_id'"),
    };
    // `override` may be absent (clear), null JSON (clear), or a string (set).
    let override_val: Option<String> = match args.get("override") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => match v.as_str() {
            Some(s) => Some(s.to_owned()),
            None => return err_result("'override' must be a string or null"),
        },
    };

    let payload = serde_json::json!({
        "backend_id": bid,
        "override": override_val,
    }).to_string();

    let store = &pool.config_store;
    match store.set_version_override(bid, override_val.clone()).await {
        Ok(()) => {
            audit_client_settings(mem, bid, "set_version_override", Some(&payload), "ok", None);
            let msg = match &override_val {
                Some(s) => format!("version override for '{bid}' set to '{s}'"),
                None    => format!("version override for '{bid}' cleared"),
            };
            ok_result(msg)
        }
        Err(e) => {
            let err_msg = format!("client_settings_set_version_override failed: {e}");
            audit_client_settings(mem, bid, "set_version_override", Some(&payload), "error", Some(&err_msg));
            err_result(err_msg)
        }
    }
}

pub(super) async fn handle_client_settings_list_mechanisms(
    args: &Value,
    pool: &BackendPool,
    _mem: &MemoryDb,
) -> Value {
    // poly-lint: allow unaudited-client-settings-tool — read-only; no audit needed.
    let bid = match str_arg(args, "backend_id") {
        Some(v) => v,
        None => return err_result("missing 'backend_id'"),
    };
    let store = &pool.config_store;
    match store.list_overrides(bid).await {
        Ok(snap) => {
            let mechs: Vec<serde_json::Value> = snap
                .mechanisms
                .into_iter()
                .map(|(id, enabled)| serde_json::json!({ "mechanism_id": id, "enabled": enabled }))
                .collect();
            ok_result(serde_json::to_string_pretty(&serde_json::json!({
                "backend_id": bid,
                "mechanisms": mechs,
            })).unwrap_or_default())
        }
        Err(e) => err_result(format!("client_settings_list_mechanisms failed: {e}")),
    }
}

pub(super) async fn handle_client_settings_set_mechanism(
    args: &Value,
    pool: &BackendPool,
    mem: &MemoryDb,
) -> Value {
    let bid   = match str_arg(args, "backend_id")   { Some(v) => v, None => return err_result("missing 'backend_id'") };
    let mech  = match str_arg(args, "mechanism_id")  { Some(v) => v, None => return err_result("missing 'mechanism_id'") };
    let enabled = match args.get("enabled").and_then(serde_json::Value::as_bool) {
        Some(b) => b,
        None => return err_result("missing or invalid 'enabled' (must be boolean)"),
    };

    let payload = serde_json::json!({
        "backend_id": bid,
        "mechanism_id": mech,
        "enabled": enabled,
    }).to_string();

    let store = &pool.config_store;
    match store.set_mechanism_state(bid, mech, enabled).await {
        Ok(()) => {
            audit_client_settings(mem, bid, "set_mechanism", Some(&payload), "ok", None);
            ok_result(format!("mechanism '{mech}' on '{bid}' set to {enabled}"))
        }
        Err(e) => {
            let err_msg = format!("client_settings_set_mechanism failed: {e}");
            audit_client_settings(mem, bid, "set_mechanism", Some(&payload), "error", Some(&err_msg));
            err_result(err_msg)
        }
    }
}
