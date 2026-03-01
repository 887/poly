//! # poly-web-devtools
//!
//! MCP server for the **web** devtools backend.
//!
//! Launches Chromium **with a visible window** by default (use `--headless` for
//! headless mode), connects to the Chrome DevTools Protocol (CDP) via WebSocket,
//! and uses real CDP commands for inspection and interaction.
//!
//! If Chrome crashes or exits while the MCP server is still running, it is
//! automatically restarted and the CDP connection is re-established.
//!
//! ## Usage
//! ```bash
//! # Visible Chrome (default)
//! cargo run --bin poly-web-devtools
//!
//! # Headless mode (CI, automated tests)
//! cargo run --bin poly-web-devtools -- --headless
//! ```

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use poly_devtools_protocol::backend::{DevtoolsBackend, ScreenshotResult};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

/// Default port for the Poly web server (dx serve).
const WEB_SERVER_PORT: u16 = 8080;
/// Chrome DevTools Protocol debugging port.
const CDP_PORT: u16 = 9222;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// CLI configuration parsed at startup.
struct CliConfig {
    /// Run Chrome in headless mode (no visible window).
    headless: bool,
}

impl CliConfig {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        Self {
            headless: args.iter().any(|a| a == "--headless"),
        }
    }
}

// ─── Chrome CDP Backend ───────────────────────────────────────────────────────

/// Web devtools backend — talks to Chrome via CDP.
struct ChromeCdpBackend {
    ws: Arc<Mutex<Option<WsStream>>>,
    msg_id: AtomicI64,
    client: reqwest::Client,
    /// Whether to run Chrome headless (no window).
    headless: bool,
    /// Set to `true` when the MCP server is shutting down — suppresses restart.
    shutting_down: Arc<AtomicBool>,
    /// Handle to the Chrome watchdog task (if running).
    watchdog_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl ChromeCdpBackend {
    fn new(headless: bool) -> Self {
        Self {
            ws: Arc::new(Mutex::new(None)),
            msg_id: AtomicI64::new(1),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            headless,
            shutting_down: Arc::new(AtomicBool::new(false)),
            watchdog_handle: Arc::new(Mutex::new(None)),
        }
    }

    /// Build the Chrome command-line arguments.
    fn chrome_args(&self) -> Vec<String> {
        // Use a dedicated profile directory so the user's main Chromium profile
        // (with all its extensions) is completely isolated from devtools sessions.
        let profile_dir = std::env::temp_dir()
            .join("poly-web-devtools-profile")
            .to_string_lossy()
            .into_owned();

        let mut args = vec![
            format!("--user-data-dir={profile_dir}"),
            "--disable-extensions".to_string(),
            format!("--remote-debugging-port={CDP_PORT}"),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            format!("http://127.0.0.1:{WEB_SERVER_PORT}"),
        ];
        if self.headless {
            args.insert(0, "--headless=new".to_string());
        }
        args
    }

    /// Spawn Chrome and start a watchdog that restarts it on crash.
    /// Returns immediately after Chrome is spawned.
    async fn spawn_chrome_with_watchdog(&self) -> anyhow::Result<()> {
        // Cancel any existing watchdog
        if let Some(handle) = self.watchdog_handle.lock().await.take() {
            handle.abort();
        }

        let chrome = find_chrome();
        let args = self.chrome_args();
        let shutting_down = self.shutting_down.clone();
        let ws = self.ws.clone();
        let headless = self.headless;

        let handle = tokio::spawn(async move {
            loop {
                if shutting_down.load(Ordering::Relaxed) {
                    tracing::info!("Shutdown flag set — not restarting Chrome");
                    break;
                }

                tracing::info!("Spawning Chrome: {chrome} {}", args.join(" "));
                let child = tokio::process::Command::new(&chrome)
                    .args(&args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();

                match child {
                    Ok(mut proc) => {
                        let status = proc.wait().await;
                        // Chrome exited — clear the WebSocket connection
                        *ws.lock().await = None;

                        if shutting_down.load(Ordering::Relaxed) {
                            tracing::info!("Chrome exited — watchdog stopped (shutdown flag set)");
                            break;
                        }

                        // If the user closed Chrome intentionally (code 0), do NOT restart
                        // automatically — that's what caused endless window loops.
                        // Only restart on a real crash (non-zero exit).
                        match &status {
                            Ok(s) if s.success() => {
                                tracing::info!(
                                    "Chrome exited cleanly (code 0) — not restarting (user closed it)"
                                );
                                break;
                            }
                            Ok(s) => {
                                tracing::warn!(
                                    "Chrome crashed (exit status: {s}) — restarting in 3s"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Error waiting on Chrome process: {e} — restarting in 3s"
                                );
                            }
                        }

                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        if shutting_down.load(Ordering::Relaxed) {
                            break;
                        }

                        tracing::info!(
                            "Restarting Chrome after crash ({})...",
                            if headless { "headless" } else { "visible" }
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to spawn Chrome: {e} — retrying in 5s");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        });

        *self.watchdog_handle.lock().await = Some(handle);
        Ok(())
    }

    /// Ensure the CDP WebSocket is connected, auto-reconnecting if the page was
    /// reloaded (which closes the old WebSocket and opens a new debugger target).
    async fn ensure_ws(&self) -> anyhow::Result<()> {
        if self.ws.lock().await.is_some() {
            return Ok(());
        }
        tracing::info!("CDP WebSocket dropped (page reloaded?) — auto-reconnecting...");
        for attempt in 1..=8u32 {
            match self.discover_ws_url().await {
                Ok(url) => match tokio_tungstenite::connect_async(&url).await {
                    Ok((stream, _)) => {
                        *self.ws.lock().await = Some(stream);
                        tracing::info!("Auto-reconnected to CDP at {url}");
                        return Ok(());
                    }
                    Err(e) => tracing::warn!("Auto-reconnect WS attempt {attempt}/8: {e}"),
                },
                Err(e) => tracing::warn!("Auto-reconnect discover attempt {attempt}/8: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        }
        anyhow::bail!(
            "CDP WebSocket disconnected and auto-reconnect failed. \
             The page may still be loading — call connect_cdp to retry."
        )
    }

    /// Send a CDP command and wait for the response.
    async fn cdp_send(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        // Transparently reconnect if ws was cleared (page reload / hot-patch)
        self.ensure_ws().await?;

        let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
        let msg = json!({ "id": id, "method": method, "params": params });

        let mut ws_guard = self.ws.lock().await;
        let ws = ws_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected to CDP. Call connect_cdp first."))?;

        ws.send(Message::Text(serde_json::to_string(&msg)?.into()))
            .await?;

        // Read messages until we get our response (matching id)
        loop {
            let Some(Ok(raw)) = ws.next().await else {
                // WebSocket died — clear it so reconnect works
                drop(ws_guard);
                *self.ws.lock().await = None;
                anyhow::bail!(
                    "CDP WebSocket closed unexpectedly — Chrome may have crashed. Call connect_cdp to reconnect."
                );
            };

            let text = match raw {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => {
                    drop(ws_guard);
                    *self.ws.lock().await = None;
                    anyhow::bail!(
                        "CDP WebSocket closed — Chrome may have crashed. Call connect_cdp to reconnect."
                    );
                }
                _ => continue,
            };

            let resp: Value = serde_json::from_str(&text)?;
            if resp.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = resp.get("error") {
                    anyhow::bail!("CDP error: {}", err);
                }
                return Ok(resp.get("result").cloned().unwrap_or(json!({})));
            }
            // Not our response — could be an event, skip it
        }
    }

    /// Discover the CDP WebSocket URL from the /json endpoint.
    ///
    /// Prefers a page target serving our app URL; falls back to any page target.
    async fn discover_ws_url(&self) -> anyhow::Result<String> {
        let url = format!("http://127.0.0.1:{CDP_PORT}/json");
        let resp = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Cannot reach Chrome CDP at {url}: {e}\n\
                     Make sure Chromium is running with --remote-debugging-port={CDP_PORT}"
                )
            })?;
        let targets: Vec<Value> = resp.json().await?;

        // 1st preference: page target serving our app URL
        let app_url = format!("http://127.0.0.1:{WEB_SERVER_PORT}");
        for target in &targets {
            if target.get("type").and_then(|v| v.as_str()) == Some("page") {
                let target_url = target.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if target_url.starts_with(&app_url) {
                    if let Some(ws_url) =
                        target.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                    {
                        return Ok(ws_url.to_string());
                    }
                }
            }
        }
        // 2nd preference: any page target
        for target in &targets {
            if target.get("type").and_then(|v| v.as_str()) == Some("page") {
                if let Some(ws_url) = target.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                    return Ok(ws_url.to_string());
                }
            }
        }

        anyhow::bail!(
            "No page target found in CDP /json response. \
             Targets: {}",
            serde_json::to_string_pretty(&targets)?
        )
    }
}

#[async_trait]
impl DevtoolsBackend for ChromeCdpBackend {
    fn name(&self) -> &str {
        "web-cdp"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        let app_dir = format!("{workspace}/apps/web");
        let mut messages = Vec::new();

        // ── Step 0: Kill any existing Chrome on the CDP port so we never stack windows ──
        // This is intentional: launch_app always produces exactly ONE fresh Chrome window.
        self.shutting_down.store(true, Ordering::Relaxed);
        if let Some(handle) = self.watchdog_handle.lock().await.take() {
            handle.abort();
        }
        {
            let mut ws_guard = self.ws.lock().await;
            *ws_guard = None;
        }
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", &format!("remote-debugging-port={CDP_PORT}")])
            .status()
            .await;
        // Brief wait for Chrome to fully exit and release the CDP port
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        // ── Step 1: Start dx serve (if not already running) ──
        let dx_check = self
            .client
            .get(format!("http://127.0.0.1:{WEB_SERVER_PORT}"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;
        if dx_check.is_err() {
            tokio::process::Command::new("dx")
                .args([
                    "serve",
                    "--platform",
                    "web",
                    "--port",
                    &WEB_SERVER_PORT.to_string(),
                ])
                .current_dir(&app_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            messages.push(format!(
                "Started `dx serve` on port {WEB_SERVER_PORT} (building...)"
            ));
        } else {
            messages.push(format!(
                "Web server already running on port {WEB_SERVER_PORT}"
            ));
        }

        // ── Step 2: Spawn one fresh Chrome ──
        self.shutting_down.store(false, Ordering::Relaxed);
        self.spawn_chrome_with_watchdog().await?;

        if self.headless {
            messages.push(format!(
                "Launched Chrome (headless) with CDP on port {CDP_PORT}"
            ));
        } else {
            messages.push(format!(
                "Launched Chrome (visible window) with CDP on port {CDP_PORT}"
            ));
        }
        messages.push("Wait ~3 seconds then call connect_cdp.".to_string());

        Ok(messages.join("\n"))
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        // Signal watchdog to stop restarting
        self.shutting_down.store(true, Ordering::Relaxed);

        // Close CDP connection
        {
            let mut ws_guard = self.ws.lock().await;
            if let Some(ws) = ws_guard.take() {
                drop(ws);
            }
        }

        // Cancel watchdog task
        if let Some(handle) = self.watchdog_handle.lock().await.take() {
            handle.abort();
        }

        // Kill Chrome and dx serve
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "remote-debugging-port=9222"])
            .status()
            .await;
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "dx serve"])
            .status()
            .await;
        Ok("Killed Chrome and dx serve processes. Watchdog stopped.".to_string())
    }

    async fn connect(&self) -> anyhow::Result<String> {
        // Clear any stale connection first
        *self.ws.lock().await = None;

        // Retry discovering the CDP WebSocket URL — Chrome may not have opened
        // the debugging port yet when called immediately after launch_app.
        let ws_url = {
            let mut last_err = anyhow::anyhow!("CDP not available");
            let mut url = None;
            for attempt in 1..=10 {
                match self.discover_ws_url().await {
                    Ok(u) => {
                        url = Some(u);
                        break;
                    }
                    Err(e) => {
                        last_err = e;
                        tracing::debug!("connect attempt {attempt}/10 failed — waiting 1s");
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
            url.ok_or(last_err)?
        };

        // Retry the WebSocket handshake (Chrome sometimes accepts TCP but hasn't
        // fully initialised the CDP endpoint when first seen in /json).
        let ws = {
            let mut last_err = anyhow::anyhow!("WebSocket connect failed");
            let mut ws = None;
            for attempt in 1..=5 {
                match tokio_tungstenite::connect_async(&ws_url).await {
                    Ok((stream, _)) => {
                        ws = Some(stream);
                        break;
                    }
                    Err(e) => {
                        last_err = anyhow::anyhow!("WS attempt {attempt}/5 at {ws_url}: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
            ws.ok_or(last_err)?
        };

        *self.ws.lock().await = Some(ws);

        // NOTE: We intentionally do NOT call Page.enable / Runtime.enable / DOM.enable
        // here. Those domains push unsolicited events into the WebSocket buffer which
        // can race with subsequent cdp_send reads. Individual commands (captureScreenshot,
        // Runtime.evaluate, etc.) work without domain-level enables.

        // Close any extra tabs — only keep the first page target so we don't
        // accumulate phantom tabs from previous launch_app calls.
        if let Ok(resp) = self
            .client
            .get(format!("http://127.0.0.1:{CDP_PORT}/json"))
            .send()
            .await
        {
            if let Ok(targets) = resp.json::<Vec<Value>>().await {
                let page_targets: Vec<&Value> = targets
                    .iter()
                    .filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
                    .collect();
                // Close all page targets except the first one
                for extra in page_targets.iter().skip(1) {
                    if let Some(id) = extra.get("id").and_then(|v| v.as_str()) {
                        let _ = self
                            .client
                            .get(format!("http://127.0.0.1:{CDP_PORT}/json/close/{id}"))
                            .send()
                            .await;
                    }
                }
            }
        }

        Ok(format!("Connected to Chrome CDP ✓  (ws: {ws_url})"))
    }

    async fn screenshot(&self) -> anyhow::Result<ScreenshotResult> {
        let result = self
            .cdp_send("Page.captureScreenshot", json!({ "format": "png" }))
            .await?;
        let b64 = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No data in screenshot response"))?;
        use base64::Engine as _;
        let png_bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
        // Save to devtools-screenshots/ so it appears as a workspace file.
        let dir = "devtools-screenshots";
        let _ = std::fs::create_dir_all(dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = std::fs::write(format!("{dir}/web-{ts}.png"), &png_bytes);
        Ok(ScreenshotResult { png_bytes })
    }

    async fn js_eval(&self, expression: &str) -> anyhow::Result<String> {
        let result = self
            .cdp_send(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
            )
            .await?;

        if let Some(exception) = result.get("exceptionDetails") {
            let text = exception
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("Unknown exception");
            anyhow::bail!("JS exception: {text}");
        }

        let value = result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null);

        Ok(match value {
            Value::String(s) => s,
            other => other.to_string(),
        })
    }

    async fn get_dom(&self) -> anyhow::Result<String> {
        self.js_eval("document.documentElement.outerHTML").await
    }

    async fn get_console(&self) -> anyhow::Result<String> {
        // Inject console capture if not already done, then retrieve
        self.js_eval(
            r#"(function(){
                if (!window.__polyLogs) {
                    window.__polyLogs = [];
                    var orig = {};
                    ['log','warn','error','info','debug'].forEach(function(lvl){
                        orig[lvl] = console[lvl];
                        console[lvl] = function(){
                            var args = Array.from(arguments).map(function(a){ try{return JSON.stringify(a);}catch(e){return String(a);} });
                            window.__polyLogs.push({level:lvl, text:args.join(' '), timestamp:Date.now()});
                            if (window.__polyLogs.length > 200) window.__polyLogs.shift();
                            orig[lvl].apply(console, arguments);
                        };
                    });
                }
                return JSON.stringify(window.__polyLogs);
            })()"#,
        )
        .await
    }

    async fn click(&self, x: i64, y: i64) -> anyhow::Result<String> {
        // Use CDP Input.dispatchMouseEvent for precise clicking
        self.cdp_send(
            "Input.dispatchMouseEvent",
            json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1 }),
        )
        .await?;
        self.cdp_send(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1 }),
        )
        .await?;

        // Also get info about what we clicked
        let info = self
            .js_eval(&format!(
                r#"(function(){{
                    var el = document.elementFromPoint({x},{y});
                    if (!el) return 'No element at ({x},{y})';
                    return 'Clicked: ' + el.tagName + (el.id ? '#'+el.id : '') + (el.className ? '.'+el.className.split(' ').join('.') : '');
                }})()"#
            ))
            .await?;
        Ok(info)
    }

    async fn type_text(&self, text: &str) -> anyhow::Result<String> {
        // Use CDP Input.insertText for reliable text input
        self.cdp_send("Input.insertText", json!({ "text": text }))
            .await?;
        Ok(format!("typed: {text}"))
    }

    async fn reset_app(&self) -> anyhow::Result<String> {
        // For web, clear all storage and reload
        self.js_eval(
            r#"(function(){
                localStorage.clear();
                sessionStorage.clear();
                indexedDB.databases().then(function(dbs){
                    dbs.forEach(function(db){ indexedDB.deleteDatabase(db.name); });
                });
                return 'Storage cleared';
            })()"#,
        )
        .await?;
        self.cdp_send("Page.reload", json!({ "ignoreCache": true }))
            .await?;
        Ok(
            "Cleared all web storage and reloaded page. App should restart at setup wizard."
                .to_string(),
        )
    }

    fn extension_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "page_reload",
                "description": "Reload the page (optionally ignoring cache).",
                "inputSchema": { "type": "object",
                    "properties": { "ignoreCache": { "type": "boolean", "description": "If true, bypass browser cache" } },
                    "required": [] }
            }),
            json!({
                "name": "set_viewport",
                "description": "Set the browser viewport size.",
                "inputSchema": { "type": "object",
                    "properties": {
                        "width": { "type": "integer" },
                        "height": { "type": "integer" }
                    },
                    "required": ["width", "height"] }
            }),
        ]
    }

    async fn handle_extension_tool(
        &self,
        name: &str,
        args: &Value,
    ) -> Option<anyhow::Result<String>> {
        match name {
            "page_reload" => {
                let ignore_cache = args
                    .get("ignoreCache")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Some(
                    self.cdp_send("Page.reload", json!({ "ignoreCache": ignore_cache }))
                        .await
                        .map(|_| "Page reloaded".to_string()),
                )
            }
            "set_viewport" => {
                let width = args.get("width").and_then(|v| v.as_i64()).unwrap_or(1440);
                let height = args.get("height").and_then(|v| v.as_i64()).unwrap_or(900);
                Some(
                    self.cdp_send(
                        "Emulation.setDeviceMetricsOverride",
                        json!({
                            "width": width,
                            "height": height,
                            "deviceScaleFactor": 1,
                            "mobile": false,
                        }),
                    )
                    .await
                    .map(|_| format!("Viewport set to {width}×{height}")),
                )
            }
            _ => None,
        }
    }
}

/// Find a Chrome/Chromium binary.
fn find_chrome() -> String {
    let candidates = [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/snap/bin/chromium",
    ];
    for c in &candidates {
        if std::process::Command::new("which")
            .arg(c)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return c.to_string();
        }
    }
    // Default fallback
    "chromium".to_string()
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = CliConfig::parse();

    if config.headless {
        tracing::info!("Starting poly-web-devtools (headless Chrome mode)");
    } else {
        tracing::info!("Starting poly-web-devtools (visible Chrome window)");
    }

    let backend = ChromeCdpBackend::new(config.headless);
    run_mcp_loop(&backend, "poly-devtools-web").await;
}
