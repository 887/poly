//! MCP JSON-RPC protocol handling.
//!
//! Provides the stdio main loop, tool list, and request/response helpers.
//! The main loop dispatches tool calls to a [`DevtoolsBackend`] implementation.

use crate::backend::{DevtoolsBackend, ScreenshotResult};
use serde_json::{Value, json};

// ─── Response Helpers ─────────────────────────────────────────────────────────

/// Build a text tool result.
pub fn text_result(text: impl ToString, is_err: bool) -> Value {
    json!({ "content": [{"type": "text", "text": text.to_string()}], "isError": is_err })
}

/// Build an image tool result from raw PNG bytes.
pub fn image_result(png: &[u8]) -> Value {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(png);
    json!({ "content": [{"type": "image", "data": b64, "mimeType": "image/png"}], "isError": false })
}

/// Wrap a value in a JSON-RPC response envelope.
pub fn mcp_response(id: Option<Value>, result: Value) -> String {
    let resp = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    // json!() always produces a serializable Value; unwrap_or_default is safe here.
    serde_json::to_string(&resp).unwrap_or_default()
}

/// Build a JSON-RPC error response.
pub fn mcp_error(id: Option<Value>, code: i64, msg: &str) -> String {
    let resp = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } });
    // json!() always produces a serializable Value; unwrap_or_default is safe here.
    serde_json::to_string(&resp).unwrap_or_default()
}

/// Parse a JSON-RPC request line into (id, method, params).
pub fn parse_request(line: &str) -> Option<(Option<Value>, String, Value)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id").cloned();
    let method = v.get("method")?.as_str()?.to_string();
    let params = v.get("params").cloned().unwrap_or(json!({}));
    Some((id, method, params))
}

// ─── Standard Tool List ───────────────────────────────────────────────────────

/// Return the base tool list (shared across all backends).
/// Backends can append extension tools via `extension_tools()`.
pub fn standard_tool_list() -> Vec<Value> {
    vec![
        json!({
            "name": "launch_app",
            "description": "Build (if needed) and launch the Poly devtools app. Wait ~2s then call connect_cdp.",
            "inputSchema": { "type": "object", "properties": {
                "workspace": { "type": "string", "description": "Path to workspace root" }
            }, "required": [] }
        }),
        json!({
            "name": "kill_app",
            "description": "Kill the running Poly devtools app.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "connect_cdp",
            "description": "Check that the devtools backend is connected and reachable.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "cdp_status",
            "description": "Same as connect_cdp — returns ok/error.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "screenshot",
            "description": "Capture the Poly UI as a PNG image. Returns an image.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "js_eval",
            "description": "Evaluate JavaScript inside the Poly webview and return the result.",
            "inputSchema": { "type": "object",
                "properties": { "expression": { "type": "string", "description": "JS expression to evaluate" } },
                "required": ["expression"] }
        }),
        json!({
            "name": "get_dom",
            "description": "Return the full document.documentElement.outerHTML.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "get_console",
            "description": "Return buffered console.log/warn/error messages (last 200).",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "click",
            "description": "Simulate a mouse click at screen coordinates (x, y) via JS dispatchEvent.",
            "inputSchema": { "type": "object",
                "properties": {
                    "x": { "type": "integer", "description": "X coordinate in CSS pixels" },
                    "y": { "type": "integer", "description": "Y coordinate in CSS pixels" }
                }, "required": ["x", "y"] }
        }),
        json!({
            "name": "type_text",
            "description": "Type text into the currently focused element via JS keyboard events.",
            "inputSchema": { "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"] }
        }),
        json!({
            "name": "rebuild_app",
            "description": "Trigger a Dioxus full rebuild (recompilation + app restart). Hot-reload handles RSX-only changes automatically — use this for structural code changes that need recompilation.",
            "inputSchema": { "type": "object", "properties": {
                "workspace": { "type": "string", "description": "Path to workspace root" }
            }, "required": [] }
        }),
        json!({
            "name": "hard_kill",
            "description": "Hard-kill the dx serve process and the running app with SIGKILL. Use when kill_app doesn't work (process is stuck). Call launch_app afterwards to restart.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "browser_reload",
            "description": "Reload the active page/webview (F5 equivalent). For desktop reloads the webview; for web reloads the browser tab.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "reset_app",
            "description": "Delete local database and restart the app at the setup wizard. Useful for testing first-launch flows.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] }
        }),
        json!({
            "name": "navigate",
            "description": "Navigate to a specific route/view within the running app.",
            "inputSchema": { "type": "object",
                "properties": { "route": { "type": "string", "description": "Route path to navigate to" } },
                "required": ["route"] }
        }),
    ]
}

// ─── Tool Dispatch ────────────────────────────────────────────────────────────

/// Dispatch a `tools/call` request to the appropriate backend method.
pub async fn dispatch_tool(backend: &dyn DevtoolsBackend, name: &str, args: &Value) -> Value {
    let ws = args
        .get("workspace")
        .and_then(|v| v.as_str())
        .unwrap_or("/home/laragana/workspcacemsg");

    match name {
        "launch_app" => match backend.launch_app(ws).await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("launch error: {e}"), true),
        },
        "kill_app" => match backend.kill_app().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("kill error: {e}"), true),
        },
        "connect_cdp" | "cdp_status" => match backend.connect().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(e.to_string(), true),
        },
        "screenshot" => match backend.screenshot().await {
            Ok(ScreenshotResult { png_bytes }) => image_result(&png_bytes),
            Err(e) => text_result(format!("screenshot error: {e}"), true),
        },
        "js_eval" => {
            let expr = args
                .get("expression")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match backend.js_eval(expr).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("eval error: {e}"), true),
            }
        }
        "get_dom" => match backend.get_dom().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("dom error: {e}"), true),
        },
        "get_console" => match backend.get_console().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("console error: {e}"), true),
        },
        "click" => {
            let x = args.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
            let y = args.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
            match backend.click(x, y).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("click error: {e}"), true),
            }
        }
        "type_text" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            match backend.type_text(text).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("type_text error: {e}"), true),
            }
        }
        "rebuild_app" => match backend.rebuild_app(ws).await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("rebuild error: {e}"), true),
        },
        "hard_kill" => match backend.hard_kill().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("hard_kill error: {e}"), true),
        },
        "browser_reload" => match backend.browser_reload().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("browser_reload error: {e}"), true),
        },
        "reset_app" => match backend.reset_app().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("reset error: {e}"), true),
        },
        "navigate" => {
            let route = args.get("route").and_then(|v| v.as_str()).unwrap_or("/");
            match backend.navigate(route).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("navigate error: {e}"), true),
            }
        }
        // Try backend-specific extension tools
        other => {
            if let Some(result) = backend.handle_extension_tool(other, args).await {
                match result {
                    Ok(r) => text_result(r, false),
                    Err(e) => text_result(format!("{other} error: {e}"), true),
                }
            } else {
                text_result(format!("Unknown tool: {other}"), true)
            }
        }
    }
}

// ─── MCP Main Loop ───────────────────────────────────────────────────────────

/// Run the MCP stdio main loop, dispatching to the given backend.
///
/// This function never returns under normal operation — it reads JSON-RPC
/// requests from stdin and writes responses to stdout.
pub async fn run_mcp_loop(backend: &dyn DevtoolsBackend, server_name: &str) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let Some((id, method, params)) = parse_request(&line) else {
            continue;
        };

        let result: Value = match method.as_str() {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": server_name, "version": "0.1.0" }
            }),

            "notifications/initialized" | "ping" => {
                // No response for notifications
                continue;
            }

            "tools/list" => {
                let mut tools = standard_tool_list();
                tools.extend(backend.extension_tools());
                json!({ "tools": tools })
            }

            "tools/call" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                dispatch_tool(backend, name, &args).await
            }

            _ => {
                let _ = stdout
                    .write_all(
                        (mcp_error(id, -32601, &format!("Method not found: {method}")) + "\n")
                            .as_bytes(),
                    )
                    .await;
                let _ = stdout.flush().await;
                continue;
            }
        };

        let response = mcp_response(id, result) + "\n";
        let _ = stdout.write_all(response.as_bytes()).await;
        let _ = stdout.flush().await;
    }
}
