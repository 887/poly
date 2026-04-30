//! Thin async wrappers for `client_settings_*` MCP tools.
//!
//! Delegates to the shared `call_persona_mcp` transport (same JSON-RPC 2.0
//! POST to `http://127.0.0.1:{port}/mcp`) — only the tool name differs.
//! Re-exports from `crate::ui::agent::persona::mcp` so both persona tools
//! and client-settings tools share one network path without duplication.

use serde_json::Value;

use crate::ui::agent::persona::mcp::call_persona_mcp;

/// Fetch a snapshot for all known backends (no `backend_id` argument).
/// Returns an array of `ClientSettingsSnapshot` JSON objects.
pub async fn client_settings_list() -> Result<Value, String> {
    call_persona_mcp("client_settings_list", serde_json::json!({})).await
}

/// Fetch the effective version for one backend.
pub async fn client_settings_get_version(backend_id: &str) -> Result<Value, String> {
    call_persona_mcp(
        "client_settings_get_version",
        serde_json::json!({ "backend_id": backend_id }),
    )
    .await
}

/// Set (or clear) the version override for one backend.
/// Pass `None` to clear the override.
pub async fn client_settings_set_version_override(
    backend_id: &str,
    override_val: Option<&str>,
) -> Result<Value, String> {
    let args = match override_val {
        Some(v) => serde_json::json!({ "backend_id": backend_id, "override": v }),
        None => serde_json::json!({ "backend_id": backend_id, "override": null }),
    };
    call_persona_mcp("client_settings_set_version_override", args).await
}

/// List mechanisms for one backend.
pub async fn client_settings_list_mechanisms(backend_id: &str) -> Result<Value, String> {
    call_persona_mcp(
        "client_settings_list_mechanisms",
        serde_json::json!({ "backend_id": backend_id }),
    )
    .await
}

/// Enable or disable a mechanism for one backend.
pub async fn client_settings_set_mechanism(
    backend_id: &str,
    mechanism_id: &str,
    enabled: bool,
) -> Result<Value, String> {
    call_persona_mcp(
        "client_settings_set_mechanism",
        serde_json::json!({
            "backend_id": backend_id,
            "mechanism_id": mechanism_id,
            "enabled": enabled,
        }),
    )
    .await
}
