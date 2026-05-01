//! Client-settings transport — direct host-bridge calls (no chat-mcp hop).
//!
//! These are *storage* operations: read/write of `client.config.<backend_id>.*`
//! KV keys via `poly_host_bridge::client_config::ClientConfigStore`. There is
//! no persona / chat reasoning involved, so the UI talks to the host bridge
//! directly instead of POSTing through chat-mcp on :3010 (which would in turn
//! call the host bridge anyway). On WASM the bridge URL resolves to
//! `window.location.origin` — same port as the fullstack server hosting this
//! UI, so the request is same-origin with no extra daemon required.
//!
//! The function names and JSON shape are kept identical to the previous
//! chat-mcp wrappers so callers (`backend_card.rs`, `version_override.rs`,
//! `mod.rs`) need no changes.

use poly_host_bridge::client_config::ClientConfigStore;
use serde_json::Value;

const KNOWN_BACKENDS: &[&str] = &[
    "stoat", "matrix", "lemmy", "hackernews", "discord",
    "teams", "poly", "github", "forgejo", "demo",
];

fn store() -> ClientConfigStore {
    ClientConfigStore::new()
}

/// Snapshot for all known backends, shape:
/// `[{ backend_id, version_override, mechanisms: [{mechanism_id, enabled}] }, …]`.
pub async fn client_settings_list() -> Result<Value, String> {
    let store = store();
    let mut out = Vec::with_capacity(KNOWN_BACKENDS.len());
    for bid in KNOWN_BACKENDS {
        match store.list_overrides(bid).await {
            Ok(snap) => {
                let mechs: Vec<Value> = snap
                    .mechanisms
                    .into_iter()
                    .map(|(id, enabled)| serde_json::json!({
                        "mechanism_id": id,
                        "enabled": enabled,
                    }))
                    .collect();
                out.push(serde_json::json!({
                    "backend_id": snap.backend_id,
                    "version_override": snap.version_override,
                    "mechanisms": mechs,
                }));
            }
            Err(e) => out.push(serde_json::json!({
                "backend_id": bid,
                "version_override": null,
                "mechanisms": [],
                "_error": format!("{e}"),
            })),
        }
    }
    Ok(Value::Array(out))
}

/// Effective version for one backend, shape:
/// `{ backend_id, effective_version, source, override }`.
pub async fn client_settings_get_version(backend_id: &str) -> Result<Value, String> {
    let store = store();
    match store.get_version_override(backend_id).await {
        Ok(Some(ov)) => Ok(serde_json::json!({
            "backend_id": backend_id,
            "effective_version": &ov,
            "source": "override",
            "override": &ov,
        })),
        Ok(None) => Ok(serde_json::json!({
            "backend_id": backend_id,
            "effective_version": null,
            "source": "default",
            "override": null,
        })),
        Err(e) => Err(format!("client_settings_get_version failed: {e}")),
    }
}

/// Set or clear (`None` → delete) the version override for one backend.
pub async fn client_settings_set_version_override(
    backend_id: &str,
    override_val: Option<&str>,
) -> Result<Value, String> {
    let store = store();
    let owned = override_val.map(str::to_owned);
    match store.set_version_override(backend_id, owned).await {
        Ok(()) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Err(format!("client_settings_set_version_override failed: {e}")),
    }
}

/// List mechanisms for one backend, shape:
/// `{ backend_id, mechanisms: [{mechanism_id, enabled}] }`.
pub async fn client_settings_list_mechanisms(backend_id: &str) -> Result<Value, String> {
    let store = store();
    match store.list_overrides(backend_id).await {
        Ok(snap) => {
            let mechs: Vec<Value> = snap
                .mechanisms
                .into_iter()
                .map(|(id, enabled)| serde_json::json!({
                    "mechanism_id": id,
                    "enabled": enabled,
                }))
                .collect();
            Ok(serde_json::json!({
                "backend_id": backend_id,
                "mechanisms": mechs,
            }))
        }
        Err(e) => Err(format!("client_settings_list_mechanisms failed: {e}")),
    }
}

/// Enable or disable one mechanism on one backend.
pub async fn client_settings_set_mechanism(
    backend_id: &str,
    mechanism_id: &str,
    enabled: bool,
) -> Result<Value, String> {
    let store = store();
    match store.set_mechanism_state(backend_id, mechanism_id, enabled).await {
        Ok(()) => Ok(serde_json::json!({ "ok": true })),
        Err(e) => Err(format!("client_settings_set_mechanism failed: {e}")),
    }
}
