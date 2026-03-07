//! # poly-web-devtools-mcp
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
//! cargo run --bin poly-web-devtools-mcp
//!
//! # Headless mode (CI, automated tests)
//! cargo run --bin poly-web-devtools-mcp -- --headless
//! ```

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use poly_devtools_protocol::backend::{
    DevtoolsBackend, NavigateParams, ScreenshotParams, ScreenshotResult,
};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

/// Default port for the Poly web server (dx serve).
/// NOTE: Port 8080 is used by `dx serve --platform desktop` for its hot-reload
/// asset server, so we use 3000 here to avoid the conflict when both MCPs run
/// simultaneously.
const WEB_SERVER_PORT: u16 = 3000;
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

/// Chrome CDP Backend — talks to Chrome via CDP, app served by `dx serve`.
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
    /// Piped stdin of the managed `dx serve` process (for rebuild commands).
    dx_serve_stdin: Arc<Mutex<Option<tokio::process::ChildStdin>>>,
    /// PID of the managed `dx serve` process (for hard-kill via SIGKILL).
    dx_serve_pid: Arc<Mutex<Option<u32>>>,
    /// Workspace path — set during `launch_app`, used by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
    /// Generation counter — increments on each successful `connect()` call.
    /// Tracks how many CDP sessions have been opened (reloads, rebuilds, etc.).
    generation: AtomicU64,
}

impl ChromeCdpBackend {
    fn new(headless: bool) -> Self {
        Self {
            ws: Arc::new(Mutex::new(None)),
            msg_id: AtomicI64::new(1),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            headless,
            shutting_down: Arc::new(AtomicBool::new(false)),
            watchdog_handle: Arc::new(Mutex::new(None)),
            dx_serve_stdin: Arc::new(Mutex::new(None)),
            dx_serve_pid: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
            generation: AtomicU64::new(0),
        }
    }

    /// Build the Chrome command-line arguments.
    fn chrome_args(&self) -> Vec<String> {
        // Use a dedicated profile directory so the user's main Chromium profile
        // (with all its extensions) is completely isolated from devtools sessions.
        let profile_dir = std::env::temp_dir()
            .join("poly-web-devtools-mcp-profile")
            .to_string_lossy()
            .into_owned();

        let mut args = vec![
            format!("--user-data-dir={profile_dir}"),
            "--disable-extensions".to_string(),
            format!("--remote-debugging-port={CDP_PORT}"),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            // Show the "Chrome is being controlled by automated test software"
            // info bar under the address bar — same behaviour as Puppeteer/
            // ChromeDriver so the user always knows this window is MCP-managed.
            "--enable-automation".to_string(),
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
                if target_url.starts_with(&app_url)
                    && let Some(ws_url) =
                        target.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                {
                    return Ok(ws_url.to_string());
                }
            }
        }
        // 2nd preference: any page target
        for target in &targets {
            if target.get("type").and_then(|v| v.as_str()) == Some("page")
                && let Some(ws_url) = target.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
            {
                return Ok(ws_url.to_string());
            }
        }

        anyhow::bail!(
            "No page target found in CDP /json response. \
             Targets: {}",
            serde_json::to_string_pretty(&targets)?
        )
    }

    /// Increment `/tmp/poly-devtools-web-rebuild-counter` atomically.
    ///
    /// This file is read by `get_generation` to populate the `build_id` field,
    /// giving callers a reliable way to detect that a rebuild was triggered
    /// without waiting for `connect_cdp` to be called first.
    fn increment_rebuild_counter() {
        let path = std::path::Path::new("/tmp/poly-devtools-web-rebuild-counter");
        let current: u64 = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let _ = std::fs::write(path, (current + 1).to_string());
    }

    /// Read the current rebuild counter from `/tmp/poly-devtools-web-rebuild-counter`.
    fn read_rebuild_counter() -> u64 {
        std::fs::read_to_string("/tmp/poly-devtools-web-rebuild-counter")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
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

        // Remember workspace for rebuild_app.
        *self.workspace.lock().await = Some(workspace.to_string());

        // ── Step 0: Kill any existing Chrome on the CDP port so we never stack windows ──
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

        // ── Step 1: Clean up any stale dx serve (wrong port or hotpatch mode) ──
        // Kill any dx serve on port 8080 (wrong port) — we need 3000
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "dx serve.*port.*8080"])
            .status()
            .await;
        // Kill any dx serve with hotpatch (wrong mode) — breaks WASM
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "dx serve.*hotpatch"])
            .status()
            .await;

        messages.push("Ensuring no stale/broken dx serve is running...".to_string());

        // ── Step 2: Start dx serve (if not already running) ──
        // NOTE: --hotpatch is intentionally NOT used for web/WASM — it is
        // experimental and causes WASM to get stuck in an infinite rebuild
        // loop (initial build finishes, then dx serve triggers a second
        // "non-hot-reloadable" rebuild that never resolves in the browser).
        // Standard hot-reload (file-watcher → full WASM recompile → page
        // refresh) works correctly without --hotpatch.
        let dx_check = self
            .client
            .get(format!("http://127.0.0.1:{WEB_SERVER_PORT}"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;
        if dx_check.is_err() {
            let mut child = tokio::process::Command::new("dx")
                .args([
                    "serve",
                    "--platform",
                    "web",
                    "--port",
                    &WEB_SERVER_PORT.to_string(),
                ])
                .current_dir(&app_dir)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()?;

            // Capture stdin for rebuild commands.
            if let Some(stdin) = child.stdin.take() {
                *self.dx_serve_stdin.lock().await = Some(stdin);
            }
            // Capture PID for hard-kill.
            if let Some(pid) = child.id() {
                *self.dx_serve_pid.lock().await = Some(pid);
            }

            // Background task: reap the child and clean up stdin on exit.
            let stdin_ref = self.dx_serve_stdin.clone();
            let pid_ref = self.dx_serve_pid.clone();
            tokio::spawn(async move {
                let _ = child.wait().await;
                *stdin_ref.lock().await = None;
                *pid_ref.lock().await = None;
            });

            messages.push(format!(
                "Started `dx serve` on port {WEB_SERVER_PORT} (building...)\n\
                 Hot reload is active — file changes trigger automatic WASM recompile."
            ));
        } else {
            messages.push(format!(
                "Web server already running on port {WEB_SERVER_PORT} (hot reload active)"
            ));
        }

        // ── Step 3: Spawn one fresh Chrome ──
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

        // Increment generation counter — each successful connect_cdp call
        // means a new CDP session (fresh page, rebuild, or reconnect after reload).
        let generation_num = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::info!("CDP connected: generation {generation_num}");

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
            && let Ok(targets) = resp.json::<Vec<Value>>().await
        {
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

        Ok(format!("Connected to Chrome CDP ✓  (ws: {ws_url})"))
    }

    async fn take_screenshot(&self, params: &ScreenshotParams) -> anyhow::Result<ScreenshotResult> {
        // Use CDP format/quality parameters.
        let format = match params.format.as_str() {
            "jpeg" => "jpeg",
            "webp" => "webp",
            _ => "png",
        };
        let mime_type = match format {
            "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };
        let mut cdp_params = json!({ "format": format });
        if let Some(q) = params.quality
            && let Some(m) = cdp_params.as_object_mut()
        {
            m.insert("quality".to_string(), json!(q));
        }
        if params.full_page {
            // Get full page metrics for full-page screenshots.
            if let Ok(metrics) = self.cdp_send("Page.getLayoutMetrics", json!({})).await
                && let Some(content_size) = metrics.get("contentSize")
            {
                let width = content_size
                    .get("width")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1440.0);
                let height = content_size
                    .get("height")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(900.0);
                if let Some(m) = cdp_params.as_object_mut() {
                    m.insert(
                        "clip".to_string(),
                        json!({"x": 0, "y": 0, "width": width, "height": height, "scale": 1}),
                    );
                }
            }
        }
        let result = self.cdp_send("Page.captureScreenshot", cdp_params).await?;
        let b64 = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No data in screenshot response"))?;
        use base64::Engine as _;
        let image_bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
        Ok(ScreenshotResult {
            image_bytes,
            mime_type: mime_type.to_string(),
        })
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

    async fn click_at(&self, x: f64, y: f64, dbl_click: bool) -> anyhow::Result<String> {
        let count: i64 = if dbl_click { 2 } else { 1 };
        let xi = x as i64;
        let yi = y as i64;
        // Use CDP Input.dispatchMouseEvent for precise clicking.
        for click_num in 1..=count {
            self.cdp_send(
                "Input.dispatchMouseEvent",
                json!({ "type": "mousePressed", "x": xi, "y": yi, "button": "left", "clickCount": click_num }),
            )
            .await?;
            self.cdp_send(
                "Input.dispatchMouseEvent",
                json!({ "type": "mouseReleased", "x": xi, "y": yi, "button": "left", "clickCount": click_num }),
            )
            .await?;
        }
        // Get info about what we clicked.
        let info = self
            .js_eval(&format!(
                r#"(function(){{
                    var el = document.elementFromPoint({x},{y});
                    if (!el) return 'No element at ({x},{y})';
                    var tag = el.tagName.toLowerCase();
                    var id = el.id ? '#'+el.id : '';
                    var txt = (el.textContent||'').trim().slice(0,40);
                    return 'Clicked '+tag+id+(txt?' "'+txt+'"':'')+' at ({x},{y})';
                }})()"#
            ))
            .await?;
        Ok(info)
    }

    async fn type_text(&self, text: &str, submit_key: Option<&str>) -> anyhow::Result<String> {
        // Use CDP Input.insertText for reliable text input.
        self.cdp_send("Input.insertText", json!({ "text": text }))
            .await?;
        // Press optional submit key (Enter, Tab, Escape, etc.).
        if let Some(key) = submit_key {
            self.cdp_send(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyDown", "key": key }),
            )
            .await?;
            self.cdp_send(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyUp", "key": key }),
            )
            .await?;
        }
        let display = match submit_key {
            Some(k) => format!("Typed \"{text}\" + {k}"),
            None => format!("Typed \"{text}\""),
        };
        Ok(display)
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

    async fn hard_kill(&self) -> anyhow::Result<String> {
        // Signal watchdog to stop restarting.
        self.shutting_down.store(true, Ordering::Relaxed);

        // Clear CDP connection.
        *self.ws.lock().await = None;

        // Cancel watchdog task.
        if let Some(handle) = self.watchdog_handle.lock().await.take() {
            handle.abort();
        }

        // SIGKILL dx serve by PID.
        let pid = self.dx_serve_pid.lock().await.take();
        *self.dx_serve_stdin.lock().await = None;
        if let Some(pid) = pid {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }

        // SIGKILL Chrome.
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", &format!("remote-debugging-port={CDP_PORT}")])
            .status()
            .await;
        // SIGKILL dx serve (pattern fallback for externally started instances).
        let _ = tokio::process::Command::new("bash")
            .args(["-c", "pkill -9 -f 'dx.*serve.*web' 2>/dev/null || true"])
            .status()
            .await;

        Ok(
            "Hard-killed Chrome and dx serve (SIGKILL). Watchdog stopped. Call launch_app to restart."
                .to_string(),
        )
    }

    async fn navigate_page(&self, params: &NavigateParams) -> anyhow::Result<String> {
        match params.nav_type.as_str() {
            "url" => {
                let url = params.url.as_deref().unwrap_or("");
                if url.is_empty() {
                    anyhow::bail!("A URL is required for navigation of type=url.");
                }
                self.cdp_send("Page.navigate", json!({ "url": url }))
                    .await?;
                // Navigation may close the WS — clear for auto-reconnect.
                *self.ws.lock().await = None;
                Ok(format!("Navigated to {url}"))
            }
            "back" => {
                self.js_eval("(function(){ window.history.back(); return 'Navigated back'; })()")
                    .await
            }
            "forward" => {
                self.js_eval(
                    "(function(){ window.history.forward(); return 'Navigated forward'; })()",
                )
                .await
            }
            "reload" => {
                let result = self
                    .cdp_send("Page.reload", json!({ "ignoreCache": params.ignore_cache }))
                    .await;
                // After reload the WS connection drops — clear for auto-reconnect.
                *self.ws.lock().await = None;
                result.map(|_| "Page reloaded.".to_string())
            }
            other => anyhow::bail!(
                "Unknown navigation type: {other}. Use url, back, forward, or reload."
            ),
        }
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Trigger a full WASM rebuild by touching a core source file so the
        // file watcher fires.  We do NOT send 'r' to stdin here because that
        // is a hotpatch-mode command; the file-watcher trigger is the correct
        // way to kick a rebuild in standard `dx serve` mode.
        //
        // We also do NOT send both signals at once — that caused a double-rebuild
        // loop that left the browser permanently stuck on the "rebuilding" overlay.
        let trigger = format!("{workspace}/crates/core/src/lib.rs");
        let _ = tokio::process::Command::new("touch")
            .arg(&trigger)
            .status()
            .await;

        // Increment the rebuild counter so get_generation's build_id field
        // increases on each rebuild_app call (mirrors desktop-devtools MCP).
        Self::increment_rebuild_counter();

        // The WASM rebuild takes 30–90 s even with a warm Cargo cache.
        // dx serve will automatically push a page-reload to the browser via the
        // hot-reload WebSocket when compilation finishes.
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        // Reconnect CDP since the page reload may have dropped the WebSocket.
        *self.ws.lock().await = None;

        Ok("Rebuild triggered (touched crates/core/src/lib.rs).\n\
             dx serve is recompiling the WASM — this takes 30-90 s with a warm cache.\n\
             The browser will auto-reload when done. Call connect_cdp afterwards."
            .to_string())
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
            json!({
                "name": "get_generation",
                "description": "Returns rebuild-detection counters for this MCP session.\n\n\
                    **generation**: increments on each successful connect_cdp call (each CDP session).\n\
                    Starts at 0 before first connect, 1 after first connect, 2 after first reconnect, etc.\n\
                    **build_id**: increments on each rebuild_app call (reads /tmp/poly-devtools-web-rebuild-counter).\n\
                    0 = no rebuild triggered yet this session. Mirrors the desktop MCP build_id semantics.\n\
                    **dx_serve_pid**: PID of the managed dx serve process (null if not started by this MCP).\n\n\
                    Decision table:\n\
                    - build_id increased, generation same → rebuild triggered, connect_cdp not yet called\n\
                    - build_id increased, generation increased → full rebuild+reconnect completed\n\
                    - dx_serve_pid changed → dx serve was restarted",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
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
            "get_generation" => {
                let generation_num = self.generation.load(Ordering::Relaxed);
                let pid = *self.dx_serve_pid.lock().await;
                let build_id = Self::read_rebuild_counter();
                Some(Ok(serde_json::json!({
                    "generation": generation_num,
                    "build_id": build_id,
                    "dx_serve_pid": pid
                })
                .to_string()))
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
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = CliConfig::parse();

    if config.headless {
        tracing::info!("Starting poly-web-devtools-mcp (headless Chrome mode)");
    } else {
        tracing::info!("Starting poly-web-devtools-mcp (visible Chrome window)");
    }

    let backend = ChromeCdpBackend::new(config.headless);
    run_mcp_loop(&backend, "poly-web").await;
}
