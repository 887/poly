//! MCP JSON-RPC protocol handling.
//!
//! Provides the stdio main loop, tool list, and request/response helpers.
//! The main loop dispatches tool calls to a [`DevtoolsBackend`] implementation.
//!
//! Tool definitions mirror those from
//! [chrome-devtools-mcp](https://github.com/nicobailey/chrome-devtools-mcp)
//! as closely as practical for our Dioxus app context.

use crate::backend::{DevtoolsBackend, NavigateParams, ScreenshotParams, ScreenshotResult};
use serde_json::{Value, json};

// ─── Response Helpers ─────────────────────────────────────────────────────────

/// Build a text tool result.
pub fn text_result(text: impl ToString, is_err: bool) -> Value {
    json!({ "content": [{"type": "text", "text": text.to_string()}], "isError": is_err })
}

/// Build a multi-text tool result (e.g. action result + snapshot).
pub fn multi_text_result(texts: &[String], is_err: bool) -> Value {
    let content: Vec<Value> = texts
        .iter()
        .map(|t| json!({"type": "text", "text": t}))
        .collect();
    json!({ "content": content, "isError": is_err })
}

/// Build an image tool result from raw image bytes.
pub fn image_result(image_bytes: &[u8], mime_type: &str) -> Value {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    json!({ "content": [{"type": "image", "data": b64, "mimeType": mime_type}], "isError": false })
}

/// Build a mixed result: text message + screenshot image.
pub fn text_and_image_result(text: &str, image_bytes: &[u8], mime_type: &str) -> Value {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    json!({ "content": [
        {"type": "text", "text": text},
        {"type": "image", "data": b64, "mimeType": mime_type}
    ], "isError": false })
}

/// Wrap a value in a JSON-RPC response envelope.
pub fn mcp_response(id: Option<Value>, result: Value) -> String {
    let resp = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    serde_json::to_string(&resp).unwrap_or_default()
}

/// Build a JSON-RPC error response.
pub fn mcp_error(id: Option<Value>, code: i64, msg: &str) -> String {
    let resp = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } });
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
// Descriptions and schemas mirror chrome-devtools-mcp as closely as possible.

/// Return the base tool list (shared across all backends).
///
/// The tool names, descriptions, and schemas follow the conventions from
/// chrome-devtools-mcp. Annotations include category and readOnlyHint.
pub fn standard_tool_list() -> Vec<Value> {
    vec![
        // ── Lifecycle (Poly-specific) ────────────────────────────────
        json!({
            "name": "launch_app",
            "description": "Build and launch the Poly app under dx serve with hot-reload. If an instance is already running, reuses it. Wait ~2 seconds then call connect_cdp to interact with the app.",
            "annotations": { "title": "Launch App", "category": "lifecycle", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {
                "workspace": { "type": "string", "description": "Path to workspace root. Auto-detected if omitted." }
            }}
        }),
        json!({
            "name": "kill_app",
            "description": "Gracefully stop the running Poly app and dx serve process.",
            "annotations": { "title": "Kill App", "category": "lifecycle", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "connect_cdp",
            "description": "Verify that the devtools backend is connected and reachable. Returns a status message. Call this after launch_app to confirm the app is ready.",
            "annotations": { "title": "Connect", "category": "lifecycle", "readOnlyHint": true },
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "cdp_status",
            "description": "Check the connection status. Same as connect_cdp.",
            "annotations": { "title": "Status", "category": "lifecycle", "readOnlyHint": true },
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "hard_kill",
            "description": "Hard-kill the dx serve process and the running app with SIGKILL. Use when kill_app doesn't work (process is stuck). Call launch_app afterwards to restart.",
            "annotations": { "title": "Hard Kill", "category": "lifecycle", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "rebuild_app",
            "description": "Trigger a Dioxus full rebuild (recompilation + app restart). Hot-reload handles RSX-only changes automatically — use this for structural code changes that need recompilation. The app will restart after building.",
            "annotations": { "title": "Rebuild App", "category": "lifecycle", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {
                "workspace": { "type": "string", "description": "Path to workspace root. Auto-detected if omitted." }
            }}
        }),
        json!({
            "name": "reset_app",
            "description": "Delete local database and restart the app at the setup wizard. Useful for testing first-launch flows.",
            "annotations": { "title": "Reset App", "category": "lifecycle", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {} }
        }),
        // ── Screenshot & Snapshot (cf. chrome-devtools-mcp) ──────────
        json!({
            "name": "take_screenshot",
            "description": "Take a screenshot of the page or element. Returns the screenshot as an image.",
            "annotations": { "title": "Take Screenshot", "category": "debugging", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {
                "format": {
                    "type": "string", "enum": ["png", "jpeg", "webp"],
                    "default": "png",
                    "description": "Type of format to save the screenshot as. Default is \"png\"."
                },
                "quality": {
                    "type": "integer", "minimum": 0, "maximum": 100,
                    "description": "Compression quality for JPEG and WebP formats (0-100). Higher values mean better quality but larger file sizes. Ignored for PNG format."
                },
                "fullPage": {
                    "type": "boolean",
                    "description": "If set to true takes a screenshot of the full page instead of the currently visible viewport."
                },
                "filePath": {
                    "type": "string",
                    "description": "The absolute path, or a path relative to the current working directory, to save the screenshot to instead of attaching it to the response."
                }
            }}
        }),
        json!({
            "name": "take_snapshot",
            "description": "Take a text snapshot of the currently selected page based on the DOM tree. The snapshot lists page elements in a tree format showing tags, IDs, roles, aria-labels, and text content. Use CSS selectors from the snapshot to target elements with click, fill, or hover tools. Always use the latest snapshot. Prefer taking a snapshot over taking a screenshot for understanding page structure.",
            "annotations": { "title": "Take Snapshot", "category": "debugging", "readOnlyHint": true },
            "inputSchema": { "type": "object", "properties": {
                "verbose": {
                    "type": "boolean",
                    "description": "Whether to include all possible information (CSS classes, data attributes, image sources). Default is false."
                }
            }}
        }),
        // ── Script (cf. chrome-devtools-mcp evaluate_script) ─────────
        json!({
            "name": "evaluate_script",
            "description": "Evaluate a JavaScript function inside the currently selected page. Returns the response as a string, so returned values should be JSON-serializable or simple strings.",
            "annotations": { "title": "Evaluate Script", "category": "debugging", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "function": {
                        "type": "string",
                        "description": "A JavaScript function declaration to be executed in the page.\nExample without arguments: `() => { return document.title }`\nExample async: `async () => { const r = await fetch('/api'); return await r.text() }`"
                    }
                },
                "required": ["function"]
            }
        }),
        // ── Console (cf. chrome-devtools-mcp list_console_messages) ──
        json!({
            "name": "list_console_messages",
            "description": "List all console messages for the currently selected page since the last navigation. Returns JSON array of {level, text, timestamp} objects.",
            "annotations": { "title": "Console Messages", "category": "debugging", "readOnlyHint": true },
            "inputSchema": { "type": "object", "properties": {} }
        }),
        // ── Navigation (cf. chrome-devtools-mcp navigate_page, wait_for)
        json!({
            "name": "navigate_page",
            "description": "Navigates the currently selected page to a URL, or navigates back/forward in history, or reloads the page.",
            "annotations": { "title": "Navigate Page", "category": "navigation", "readOnlyHint": false },
            "inputSchema": { "type": "object", "properties": {
                "type": {
                    "type": "string", "enum": ["url", "back", "forward", "reload"],
                    "description": "Navigate the page by URL, back or forward in history, or reload."
                },
                "url": {
                    "type": "string",
                    "description": "Target URL (only for type=url)."
                },
                "ignoreCache": {
                    "type": "boolean",
                    "description": "Whether to ignore cache on reload."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Navigation timeout in milliseconds. Default is 30000."
                }
            }}
        }),
        json!({
            "name": "wait_for",
            "description": "Wait for the specified text to appear on the selected page. Polls the page content until any of the specified texts is found or the timeout is reached.",
            "annotations": { "title": "Wait For", "category": "navigation", "readOnlyHint": true },
            "inputSchema": { "type": "object",
                "properties": {
                    "text": {
                        "type": "array", "items": { "type": "string" }, "minItems": 1,
                        "description": "Non-empty list of texts. Resolves when any value appears on the page."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Maximum time to wait in milliseconds. Default is 10000."
                    }
                },
                "required": ["text"]
            }
        }),
        // ── Input (cf. chrome-devtools-mcp click, click_at, hover,
        //    fill, type_text, handle_dialog) ──────────────────────────
        json!({
            "name": "click",
            "description": "Click on an element matching the given CSS selector. The element is scrolled into view before clicking. Use take_snapshot to find the right selector.",
            "annotations": { "title": "Click", "category": "input", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector of the element to click (e.g. '#submit-btn', '.nav-item', 'button[aria-label=\"Save\"]')"
                    },
                    "includeSnapshot": {
                        "type": "boolean",
                        "description": "Whether to include a snapshot in the response. Default is false."
                    }
                },
                "required": ["selector"]
            }
        }),
        json!({
            "name": "click_at",
            "description": "Click at the provided coordinates on the page. Dispatches proper pointer and mouse events.",
            "annotations": { "title": "Click At", "category": "input", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "x": { "type": "number", "description": "The x coordinate in CSS pixels" },
                    "y": { "type": "number", "description": "The y coordinate in CSS pixels" },
                    "dblClick": { "type": "boolean", "description": "Set to true for double clicks. Default is false." },
                    "includeSnapshot": {
                        "type": "boolean",
                        "description": "Whether to include a snapshot in the response. Default is false."
                    }
                },
                "required": ["x", "y"]
            }
        }),
        json!({
            "name": "hover",
            "description": "Hover over an element matching the given CSS selector. Dispatches mouseenter, mouseover, and mousemove events.",
            "annotations": { "title": "Hover", "category": "input", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector of the element to hover over"
                    },
                    "includeSnapshot": {
                        "type": "boolean",
                        "description": "Whether to include a snapshot in the response. Default is false."
                    }
                },
                "required": ["selector"]
            }
        }),
        json!({
            "name": "fill",
            "description": "Type text into an input, textarea, or select an option from a <select> element. Uses the native value setter to trigger framework change handlers (React, Dioxus, etc.).",
            "annotations": { "title": "Fill", "category": "input", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector of the input element to fill"
                    },
                    "value": {
                        "type": "string",
                        "description": "The value to fill in"
                    },
                    "includeSnapshot": {
                        "type": "boolean",
                        "description": "Whether to include a snapshot in the response. Default is false."
                    }
                },
                "required": ["selector", "value"]
            }
        }),
        json!({
            "name": "type_text",
            "description": "Type text using keyboard into a previously focused element. Use click or fill to focus an element first.",
            "annotations": { "title": "Type Text", "category": "input", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "The text to type" },
                    "submitKey": {
                        "type": "string",
                        "description": "Optional key to press after typing. E.g., \"Enter\", \"Tab\", \"Escape\""
                    }
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "handle_dialog",
            "description": "If a browser dialog was opened (alert, confirm, prompt), use this command to handle it.",
            "annotations": { "title": "Handle Dialog", "category": "input", "readOnlyHint": false },
            "inputSchema": { "type": "object",
                "properties": {
                    "action": {
                        "type": "string", "enum": ["accept", "dismiss"],
                        "description": "Whether to dismiss or accept the dialog"
                    },
                    "promptText": {
                        "type": "string",
                        "description": "Optional prompt text to enter into the dialog."
                    }
                },
                "required": ["action"]
            }
        }),
    ]
}

// ─── Tool Dispatch ────────────────────────────────────────────────────────────

/// Dispatch a `tools/call` request to the appropriate backend method.
pub async fn dispatch_tool(backend: &dyn DevtoolsBackend, name: &str, args: &Value) -> Value {
    let ws = args
        .get("workspace")
        .and_then(|v| v.as_str())
        .or_else(|| std::env::var("POLY_WORKSPACE").ok().as_deref().map(|_| ""))
        .unwrap_or(default_workspace());

    match name {
        // ── Lifecycle ────────────────────────────────────────────────
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
        "hard_kill" => match backend.hard_kill().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("hard_kill error: {e}"), true),
        },
        "rebuild_app" => match backend.rebuild_app(ws).await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("rebuild error: {e}"), true),
        },
        "reset_app" => match backend.reset_app().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("reset error: {e}"), true),
        },

        // ── Screenshot & Snapshot ────────────────────────────────────
        "take_screenshot" => {
            let params = ScreenshotParams {
                format: args
                    .get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("png")
                    .to_string(),
                quality: args
                    .get("quality")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32),
                full_page: args
                    .get("fullPage")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                file_path: args
                    .get("filePath")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            };

            match backend.take_screenshot(&params).await {
                Ok(ScreenshotResult {
                    image_bytes,
                    mime_type,
                }) => {
                    // Save to file if requested
                    if let Some(ref path) = params.file_path {
                        match std::fs::write(path, &image_bytes) {
                            Ok(()) => text_result(format!("Screenshot saved to {path}"), false),
                            Err(e) => text_result(format!("Failed to save screenshot: {e}"), true),
                        }
                    } else {
                        // Also save to devtools-screenshots/ for reference
                        let dir = "devtools-screenshots";
                        let _ = std::fs::create_dir_all(dir);
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        let ext = &params.format;
                        let _ =
                            std::fs::write(format!("{dir}/screenshot-{ts}.{ext}"), &image_bytes);

                        // Include CSS viewport dimensions so caller knows the coordinate
                        // space.  The screenshot image pixels = CSS pixels when DPR=1.
                        // click_at(x, y) takes CSS pixel coordinates, NOT display-scaled
                        // pixel coordinates from the rendered image.  Use
                        // evaluate_script with getBoundingClientRect() to get exact
                        // element centres rather than eyeballing the image.
                        let viewport_info = backend
                            .js_eval(
                                "(function(){\
                                    var iw=window.innerWidth,ih=window.innerHeight;\
                                    var dpr=window.devicePixelRatio||1;\
                                    return 'CSS viewport: '+iw+'x'+ih+' (DPR='+dpr+').\\n\
                                            Screenshot pixels = CSS pixels when DPR=1.\\n\
                                            IMPORTANT: click_at(x,y) uses CSS pixel coordinates.\\n\
                                            Do NOT read coordinates from the image display pixels.\\n\
                                            Instead use evaluate_script with getBoundingClientRect()\\n\
                                            to get element centres, e.g.:\\n\
                                            () => { var r=document.querySelector(\"button\").getBoundingClientRect(); return JSON.stringify({cx:r.left+r.width/2,cy:r.top+r.height/2}) }';\
                                })()",
                            )
                            .await
                            .unwrap_or_else(|_| "(viewport info unavailable)".to_string());

                        use base64::Engine as _;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
                        json!({ "content": [
                            {"type": "text", "text": viewport_info},
                            {"type": "image", "data": b64, "mimeType": mime_type}
                        ], "isError": false })
                    }
                }
                Err(e) => text_result(format!("screenshot error: {e}"), true),
            }
        }
        "take_snapshot" => {
            let verbose = args
                .get("verbose")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match backend.take_snapshot(verbose).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("snapshot error: {e}"), true),
            }
        }

        // ── Script ───────────────────────────────────────────────────
        "evaluate_script" => {
            let function = args
                .get("function")
                .and_then(|v| v.as_str())
                .unwrap_or("() => null");
            match backend.evaluate_script(function).await {
                Ok(r) => {
                    let mut result = String::from("Script ran on page and returned:\n```json\n");
                    result.push_str(&r);
                    result.push_str("\n```");
                    text_result(result, false)
                }
                Err(e) => text_result(format!("evaluate error: {e}"), true),
            }
        }

        // ── Console ──────────────────────────────────────────────────
        "list_console_messages" => match backend.list_console_messages().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("console error: {e}"), true),
        },

        // ── Navigation ───────────────────────────────────────────────
        "navigate_page" => {
            let params = NavigateParams {
                nav_type: args
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or(if args.get("url").is_some() { "url" } else { "" })
                    .to_string(),
                url: args
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                ignore_cache: args
                    .get("ignoreCache")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                timeout_ms: args
                    .get("timeout")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(30_000),
            };

            if params.nav_type.is_empty() && params.url.is_none() {
                text_result("Either URL or a type is required.", true)
            } else {
                match backend.navigate_page(&params).await {
                    Ok(r) => text_result(r, false),
                    Err(e) => text_result(format!("navigate error: {e}"), true),
                }
            }
        }
        "wait_for" => {
            let texts: Vec<String> = args
                .get("text")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let timeout = args
                .get("timeout")
                .and_then(|v| v.as_u64())
                .unwrap_or(10_000);

            if texts.is_empty() {
                text_result("At least one text string is required.", true)
            } else {
                match backend.wait_for_text(&texts, timeout).await {
                    Ok(r) => {
                        // After finding the text, include a snapshot
                        let mut parts = vec![r];
                        if let Ok(snapshot) = backend.take_snapshot(false).await {
                            parts.push(snapshot);
                        }
                        multi_text_result(&parts, false)
                    }
                    Err(e) => text_result(format!("wait_for error: {e}"), true),
                }
            }
        }

        // ── Input ────────────────────────────────────────────────────
        "click" => {
            let selector = args.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let include_snapshot = args
                .get("includeSnapshot")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            match backend.click_element(selector).await {
                Ok(r) => {
                    if include_snapshot {
                        let mut parts = vec![r];
                        if let Ok(snapshot) = backend.take_snapshot(false).await {
                            parts.push(snapshot);
                        }
                        multi_text_result(&parts, false)
                    } else {
                        text_result(r, false)
                    }
                }
                Err(e) => text_result(format!("click error: {e}"), true),
            }
        }
        "click_at" => {
            let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let dbl = args
                .get("dblClick")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let include_snapshot = args
                .get("includeSnapshot")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            match backend.click_at(x, y, dbl).await {
                Ok(r) => {
                    if include_snapshot {
                        let mut parts = vec![r];
                        if let Ok(snapshot) = backend.take_snapshot(false).await {
                            parts.push(snapshot);
                        }
                        multi_text_result(&parts, false)
                    } else {
                        text_result(r, false)
                    }
                }
                Err(e) => text_result(format!("click_at error: {e}"), true),
            }
        }
        "hover" => {
            let selector = args.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let include_snapshot = args
                .get("includeSnapshot")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            match backend.hover_element(selector).await {
                Ok(r) => {
                    if include_snapshot {
                        let mut parts = vec![r];
                        if let Ok(snapshot) = backend.take_snapshot(false).await {
                            parts.push(snapshot);
                        }
                        multi_text_result(&parts, false)
                    } else {
                        text_result(r, false)
                    }
                }
                Err(e) => text_result(format!("hover error: {e}"), true),
            }
        }
        "fill" => {
            let selector = args.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let include_snapshot = args
                .get("includeSnapshot")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            match backend.fill_element(selector, value).await {
                Ok(r) => {
                    if include_snapshot {
                        let mut parts = vec![r];
                        if let Ok(snapshot) = backend.take_snapshot(false).await {
                            parts.push(snapshot);
                        }
                        multi_text_result(&parts, false)
                    } else {
                        text_result(r, false)
                    }
                }
                Err(e) => text_result(format!("fill error: {e}"), true),
            }
        }
        "type_text" => {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let submit_key = args.get("submitKey").and_then(|v| v.as_str());

            match backend.type_text(text, submit_key).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("type_text error: {e}"), true),
            }
        }
        "handle_dialog" => {
            let action = args
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("accept");
            let prompt_text = args.get("promptText").and_then(|v| v.as_str());

            match backend.handle_dialog(action, prompt_text).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("handle_dialog error: {e}"), true),
            }
        }

        // ── Legacy aliases (backwards compatibility) ─────────────────
        "screenshot" => {
            // Old name → redirect to take_screenshot
            match backend.take_screenshot(&ScreenshotParams::default()).await {
                Ok(ScreenshotResult {
                    image_bytes,
                    mime_type,
                }) => {
                    let dir = "devtools-screenshots";
                    let _ = std::fs::create_dir_all(dir);
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let _ = std::fs::write(format!("{dir}/screenshot-{ts}.png"), &image_bytes);
                    image_result(&image_bytes, &mime_type)
                }
                Err(e) => text_result(format!("screenshot error: {e}"), true),
            }
        }
        "js_eval" => {
            // Old name → redirect to evaluate_script
            let expr = args
                .get("expression")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match backend.js_eval(expr).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("eval error: {e}"), true),
            }
        }
        "get_dom" => {
            // Legacy alias — returns full raw HTML (document.documentElement.outerHTML),
            // same behaviour as the old per-backend get_dom methods.
            // For structured page inspection use take_snapshot instead.
            match backend.js_eval("document.documentElement.outerHTML").await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("dom error: {e}"), true),
            }
        }
        "get_console" => match backend.list_console_messages().await {
            Ok(r) => text_result(r, false),
            Err(e) => text_result(format!("console error: {e}"), true),
        },
        "navigate" => {
            let route = args.get("route").and_then(|v| v.as_str()).unwrap_or("/");
            let params = NavigateParams {
                nav_type: "url".to_string(),
                url: Some(route.to_string()),
                ..Default::default()
            };
            match backend.navigate_page(&params).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("navigate error: {e}"), true),
            }
        }
        "browser_reload" => {
            let params = NavigateParams {
                nav_type: "reload".to_string(),
                ..Default::default()
            };
            match backend.navigate_page(&params).await {
                Ok(r) => text_result(r, false),
                Err(e) => text_result(format!("browser_reload error: {e}"), true),
            }
        }

        // ── Extension / unknown ──────────────────────────────────────
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

/// Auto-detect the workspace path.
fn default_workspace() -> &'static str {
    // Best effort: use the POLY_WORKSPACE env var or fall back to cwd detection
    static WORKSPACE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    WORKSPACE.get_or_init(|| {
        if let Ok(ws) = std::env::var("POLY_WORKSPACE") {
            return ws;
        }
        // Walk up from cwd looking for Cargo.toml with [workspace]
        if let Ok(cwd) = std::env::current_dir() {
            let mut dir = cwd.as_path();
            loop {
                let cargo = dir.join("Cargo.toml");
                if cargo.exists()
                    && let Ok(content) = std::fs::read_to_string(&cargo)
                    && content.contains("[workspace")
                {
                    return dir.to_string_lossy().into_owned();
                }
                match dir.parent() {
                    Some(parent) => dir = parent,
                    None => break,
                }
            }
        }
        "/home/laragana/workspcacemsg".to_string()
    })
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
                "serverInfo": { "name": server_name, "version": "0.2.0" }
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
