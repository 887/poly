//! Thin async wrapper for calling `poly-chat-mcp` persona tools from the WASM client.
//!
//! The pattern mirrors `draft_banner::call_draft_mcp`: POST JSON-RPC 2.0 to
//! `http://127.0.0.1:{port}/mcp`.  On WASM the `reqwest` crate is not available;
//! we use `web_sys::fetch` via `wasm_bindgen_futures`.  On native we use `reqwest`.
//!
//! The MCP port is read from the KV key `agent.mcp.port`; defaults to 3010.

use serde_json::Value;

const DEFAULT_MCP_PORT: u16 = 3010;

/// Resolve the MCP port: KV `agent.mcp.port` → env `POLY_CHAT_MCP_PORT` → 3010.
async fn resolve_port() -> u16 {
    // Try KV first.
    if let Some(storage) = crate::STORAGE.get()
        && let Ok(Some(v)) = storage.get("agent.mcp.port").await
            && let Some(p) = v.as_u64().and_then(|n| u16::try_from(n).ok())
                && p >= 1024 {
                    return p;
                }
    // Env fallback (native only).
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(p) = std::env::var("POLY_CHAT_MCP_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .filter(|p| *p >= 1024)
            .ok_or(())
        {
            return p;
        }
    }
    DEFAULT_MCP_PORT
}

/// Call a `meta_persona_*` tool on the local `poly-chat-mcp` process.
///
/// Returns `Ok(Value)` with the `result.content[0].text` JSON on success.
/// Returns `Err(String)` on network error or MCP-level error response.
pub async fn call_persona_mcp(tool: &str, args: Value) -> Result<Value, String> {
    let port = resolve_port().await;
    let url = format!("http://127.0.0.1:{port}/mcp");
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1_i32,
        "method": "tools/call",
        "params": {
            "name": tool,
            "arguments": args
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: Value = resp.json().await.map_err(|e| e.to_string())?;
        extract_mcp_result(json)
    }

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::JsValue;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{Request, RequestInit, RequestMode, Response};

        let body_str = serde_json::to_string(&body).map_err(|e| e.to_string())?;

        let opts = RequestInit::new();
        opts.set_method("POST");
        // poly-chat-mcp listens on its own port (3010 by default), so the
        // request is cross-origin from poly-web (:3000). The MCP server's
        // CorsLayer::very_permissive() handles the preflight.
        opts.set_mode(RequestMode::Cors);
        opts.set_body(&JsValue::from_str(&body_str));

        let request = Request::new_with_str_and_init(&url, &opts)
            .map_err(|e| format!("fetch init: {e:?}"))?;
        request
            .headers()
            .set("content-type", "application/json")
            .map_err(|e| format!("header: {e:?}"))?;

        let window = web_sys::window().ok_or("no window")?;
        let resp: Response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| format!("fetch: {e:?}"))?
            .dyn_into()
            .map_err(|_| "not a Response".to_string())?;

        let text_promise = resp.text().map_err(|e| format!("resp.text(): {e:?}"))?;
        let text = JsFuture::from(text_promise)
            .await
            .map_err(|e| format!("text await: {e:?}"))?
            .as_string()
            .ok_or_else(|| "non-string body".to_string())?;

        let json: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        extract_mcp_result(json)
    }
}

/// Extract the payload from a JSON-RPC 2.0 MCP response.
///
/// MCP success: `{"result": {"content": [{"type": "text", "text": "..."}]}}`
/// MCP error:   `{"result": {"isError": true, "content": [...]}}`
fn extract_mcp_result(json: Value) -> Result<Value, String> {
    if let Some(err) = json.get("error") {
        return Err(format!("mcp error: {err}"));
    }
    let result = json
        .get("result")
        .ok_or_else(|| "no result field".to_string())?;

    if result
        .get("isError")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        let msg = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|e| e.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("unknown MCP error");
        return Err(msg.to_string());
    }

    // Parse the text payload as JSON (tools return JSON strings in `text`).
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|e| e.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("{}");

    serde_json::from_str(text).map_err(|e| format!("parse result: {e}"))
}
