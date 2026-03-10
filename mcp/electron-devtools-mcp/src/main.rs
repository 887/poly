//! # poly-electron-devtools-mcp
//!
//! MCP server for debugging the Poly Desktop Electron (WASM/Electron) build.
//!
//! **Workflow:**
//! 1. `launch_app` — builds the Dioxus WASM bundle via `dx build --platform web`
//!    in `apps/desktop-electron/`, then launches Electron from
//!    `apps/desktop-electron-devtools/electron/` with Chrome DevTools Protocol
//!    (CDP) enabled on port **9224**.
//! 2. `connect_cdp` — establishes a WebSocket connection to the CDP endpoint.
//! 3. `take_screenshot`, `js_eval`, `click_at`, etc. — delegate to CDP.
//!
//! Architecturally identical to `poly-web-devtools-mcp` (Chrome CDP) but uses
//! Electron as the runtime and `dx build` (one-shot) instead of `dx serve`.
//!
//! ## Usage
//! ```bash
//! cargo run --bin poly-electron-devtools-mcp
//! ```

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use poly_devtools_protocol::backend::{
    DevtoolsBackend, NavigateParams, ScreenshotParams, ScreenshotResult,
};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

/// Chrome DevTools Protocol port for the Electron devtools build.
///
/// Using 9224 to avoid conflicts with:
/// - `poly-web-devtools-mcp` → 9222
/// - `poly-desktop-devtools` HTTP bridge → 9223
const CDP_PORT: u16 = 9224;

/// Rebuild counter file — incremented by `launch_app` and `rebuild_app`.
/// Separate from desktop (`…rebuild-counter`) and web (`…web-rebuild-counter`).
const REBUILD_COUNTER_PATH: &str = "/tmp/poly-devtools-electron-rebuild-counter";

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

// ─── Backend ─────────────────────────────────────────────────────────────────

/// Electron CDP backend.
///
/// Builds the WASM bundle once, launches Electron with CDP enabled, then drives
/// the app via WebSocket CDP commands.
struct ElectronCdpBackend {
    /// Active CDP WebSocket connection (`None` when disconnected or after reload).
    ws: Arc<Mutex<Option<WsStream>>>,
    /// Auto-incrementing CDP message ID.
    msg_id: AtomicI64,
    /// HTTP client used for CDP target discovery (`/json`).
    client: reqwest::Client,
    /// PID of the managed Electron process (`None` if not launched by us or already exited).
    electron_pid: Arc<Mutex<Option<u32>>>,
    /// Workspace root path — set in `launch_app`, reused by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
    /// Generation counter — increments on each successful `connect()` call.
    generation: AtomicU64,
}

impl ElectronCdpBackend {
    fn new() -> Self {
        Self {
            ws: Arc::new(Mutex::new(None)),
            msg_id: AtomicI64::new(1),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            electron_pid: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
            generation: AtomicU64::new(0),
        }
    }

    /// Ensure the CDP WebSocket is connected.
    ///
    /// Auto-reconnects if the page was reloaded (which invalidates the
    /// previous debugger session and opens a fresh one at a new WS URL).
    async fn ensure_ws(&self) -> anyhow::Result<()> {
        if self.ws.lock().await.is_some() {
            return Ok(());
        }
        tracing::info!("CDP WebSocket is None — auto-reconnecting…");
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
            tokio::time::sleep(Duration::from_millis(600)).await;
        }
        anyhow::bail!(
            "CDP WebSocket disconnected and auto-reconnect failed after 8 attempts. \
             The page may still be loading — call connect_cdp to retry."
        )
    }

    /// Send a CDP command and wait for the matching response.
    ///
    /// Transparently calls `ensure_ws()` first so callers don't need to
    /// manually reconnect after page reloads.
    async fn cdp_send(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.ensure_ws().await?;

        let id = self.msg_id.fetch_add(1, Ordering::Relaxed);
        let msg = json!({ "id": id, "method": method, "params": params });

        let mut ws_guard = self.ws.lock().await;
        let ws = ws_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected to CDP. Call connect_cdp first."))?;

        ws.send(Message::Text(serde_json::to_string(&msg)?.into()))
            .await?;

        // Read messages until we get our response (matching id).
        // Other messages (CDP events) are skipped.
        loop {
            let Some(Ok(raw)) = ws.next().await else {
                // WebSocket died — clear it so reconnect works.
                drop(ws_guard);
                *self.ws.lock().await = None;
                anyhow::bail!(
                    "CDP WebSocket closed unexpectedly. \
                     Electron may have crashed. Call connect_cdp to reconnect."
                );
            };

            let text = match raw {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => {
                    drop(ws_guard);
                    *self.ws.lock().await = None;
                    anyhow::bail!(
                        "CDP WebSocket closed (Electron closed or page reloaded). \
                         Call connect_cdp to reconnect."
                    );
                }
                _ => continue,
            };

            let resp: Value = serde_json::from_str(&text)?;
            if resp.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = resp.get("error") {
                    anyhow::bail!("CDP error from method '{method}': {err}");
                }
                return Ok(resp.get("result").cloned().unwrap_or(json!({})));
            }
            // Not our response — CDP event or another command's response, skip.
        }
    }

    /// Discover the CDP WebSocket URL for the Electron app page.
    ///
    /// **Priority:**
    /// 1. Page targets with `file://` or `app://` URLs (our WASM bundle)
    /// 2. Any page-type target (handles `about:blank` while the app is loading)
    async fn discover_ws_url(&self) -> anyhow::Result<String> {
        let list_url = format!("http://127.0.0.1:{CDP_PORT}/json");
        let resp = self
            .client
            .get(&list_url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Cannot reach Electron CDP at {list_url}: {e}\n\
                     Make sure Electron was launched with --remote-debugging-port={CDP_PORT}."
                )
            })?;

        let targets: Vec<Value> = resp.json().await?;

        // 1st preference: the WASM app loaded from file://, app://, or our
        // local embedded HTTP asset server.
        for target in &targets {
            if target.get("type").and_then(|v| v.as_str()) == Some("page") {
                let target_url = target.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if (target_url.starts_with("file://")
                    || target_url.starts_with("app://")
                    || target_url.starts_with("http://127.0.0.1:")
                    || target_url.starts_with("http://localhost:"))
                    && let Some(ws_url) =
                        target.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
                {
                    return Ok(ws_url.to_string());
                }
            }
        }

        // 2nd preference: any page target (e.g. about:blank while Electron boots)
        for target in &targets {
            if target.get("type").and_then(|v| v.as_str()) == Some("page")
                && let Some(ws_url) = target.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
            {
                return Ok(ws_url.to_string());
            }
        }

        anyhow::bail!(
            "No page target found in CDP /json response. Targets: {}",
            serde_json::to_string(&targets).unwrap_or_default()
        )
    }

    /// Atomically increment `/tmp/poly-devtools-electron-rebuild-counter`.
    fn increment_rebuild_counter() {
        let path = std::path::Path::new(REBUILD_COUNTER_PATH);
        let current: u64 = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let _ = std::fs::write(path, (current + 1).to_string());
    }

    /// Read the current value of the rebuild counter.
    fn read_rebuild_counter() -> u64 {
        std::fs::read_to_string(REBUILD_COUNTER_PATH)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }
}

// ─── DevtoolsBackend impl ─────────────────────────────────────────────────────

#[async_trait]
impl DevtoolsBackend for ElectronCdpBackend {
    fn name(&self) -> &str {
        "electron-cdp"
    }

    // ── Lifecycle ───────────────────────────────────────────────────────────

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        *self.workspace.lock().await = Some(workspace.to_string());
        let mut messages: Vec<String> = Vec::new();

        let dx_build_dir = format!("{workspace}/apps/desktop-electron");
        let electron_devtools_dir = format!("{workspace}/apps/desktop-electron-devtools/electron");

        // ── Step 0: Kill any existing Electron on CDP port 9224 ──────────────
        let _ = tokio::process::Command::new("pkill")
            .args(["-15", "-f", &electron_devtools_dir])
            .status()
            .await;
        let _ = tokio::process::Command::new("pkill")
            .args(["-15", "-f", &format!("remote-debugging-port={CDP_PORT}")])
            .status()
            .await;
        tokio::time::sleep(Duration::from_millis(800)).await;
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", &electron_devtools_dir])
            .status()
            .await;
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", &format!("remote-debugging-port={CDP_PORT}")])
            .status()
            .await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // ── Step 1: Build the WASM bundle ─────────────────────────────────────
        messages.push(format!(
            "Building WASM bundle (`dx build --platform web` in {dx_build_dir}).\n\
             This takes 30–90 s with a warm Cargo cache…"
        ));
        tracing::info!("Running dx build --platform web in {dx_build_dir}");

        let build_status = tokio::process::Command::new("dx")
            .args(["build", "--platform", "web"])
            .current_dir(&dx_build_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit()) // Show compile errors in the MCP log
            .status()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn `dx build`: {e}"))?;

        if !build_status.success() {
            anyhow::bail!(
                "`dx build --platform web` failed (exit code: {:?}).\n\
                 Check the MCP server stderr for compile errors.\n\
                 Note: poly-core may be in flux — wait a minute and retry.",
                build_status.code()
            );
        }
        messages.push("WASM build succeeded ✓".to_string());

        // Increment rebuild counter to mark this as a new build.
        Self::increment_rebuild_counter();

        // ── Step 2: Install Electron npm dependencies ────────────────────────
        tracing::info!("Installing npm deps in {electron_devtools_dir}");
        let npm_result = tokio::process::Command::new("npm")
            .args(["install", "--prefer-offline"])
            .current_dir(&electron_devtools_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        match npm_result {
            Ok(s) if s.success() => {
                messages.push("npm install completed ✓".to_string());
            }
            Ok(s) => {
                tracing::warn!(
                    "npm install exited with code {:?} — continuing anyway",
                    s.code()
                );
                messages.push(format!(
                    "npm install exited {:?} (continuing — may already be installed)",
                    s.code()
                ));
            }
            Err(e) => {
                tracing::warn!("npm install failed to spawn: {e} — continuing anyway");
            }
        }

        // ── Step 3: Launch Electron ──────────────────────────────────────────
        // Prefer the locally installed binary; fall back to npx.
        let local_electron = format!("{electron_devtools_dir}/node_modules/.bin/electron");
        let mut cmd = if std::path::Path::new(&local_electron).exists() {
            tracing::info!("Using local electron binary: {local_electron}");
            tokio::process::Command::new(&local_electron)
        } else {
            tracing::info!("Local electron binary not found, using npx electron");
            let mut c = tokio::process::Command::new("npx");
            c.arg("electron");
            c
        };

        cmd.arg(".")
            .current_dir(&electron_devtools_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit()); // Show Electron errors in MCP log

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn Electron: {e}"))?;

        let pid = child.id();
        *self.electron_pid.lock().await = pid;

        // Reap the child in the background so it doesn't become a zombie.
        let pid_ref = self.electron_pid.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *pid_ref.lock().await = None;
            tracing::info!("Electron process exited");
        });

        messages.push(format!(
            "Launched Electron with CDP on port {CDP_PORT} (PID: {pid:?}) ✓\n\
             Wait ~5 seconds for Electron to fully initialize, then call connect_cdp."
        ));

        Ok(messages.join("\n\n"))
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        // Close the CDP connection first.
        *self.ws.lock().await = None;

        // Gracefully terminate Electron by PID.
        if let Some(pid) = self.electron_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status()
                .await;
        }
        // Pattern fallback to catch any stray Electron --remote-debugging-port=9224 processes.
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "remote-debugging-port=9224"])
            .status()
            .await;

        Ok("Killed Electron process. CDP connection closed.".to_string())
    }

    async fn hard_kill(&self) -> anyhow::Result<String> {
        // Close CDP connection.
        *self.ws.lock().await = None;

        // SIGKILL Electron by PID.
        if let Some(pid) = self.electron_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }
        // SIGKILL by pattern fallback.
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", "remote-debugging-port=9224"])
            .status()
            .await;

        Ok("Hard-killed Electron (SIGKILL). Call launch_app to restart.".to_string())
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Track rebuild for get_generation.
        Self::increment_rebuild_counter();

        let dx_build_dir = format!("{workspace}/apps/desktop-electron");
        tracing::info!("Rebuilding WASM: dx build --platform web in {dx_build_dir}");

        let build_status = tokio::process::Command::new("dx")
            .args(["build", "--platform", "web"])
            .current_dir(&dx_build_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn `dx build` for rebuild: {e}"))?;

        if !build_status.success() {
            anyhow::bail!(
                "`dx build --platform web` failed during rebuild (exit: {:?}).\n\
                 Check the MCP server stderr for compile errors.",
                build_status.code()
            );
        }

        // Reload the page via CDP so Electron picks up the new WASM bundle.
        // The reload command returns a result before the page actually reloads,
        // so cdp_send succeeds. Then the reload closes the WebSocket — we clear
        // it so the next call auto-reconnects.
        let _ = self
            .cdp_send("Page.reload", json!({ "ignoreCache": true }))
            .await;
        *self.ws.lock().await = None;

        Ok("WASM rebuilt and Electron page reloaded ✓\n\
             Call connect_cdp to re-establish the CDP session."
            .to_string())
    }

    async fn reset_app(&self) -> anyhow::Result<String> {
        // Clear all web storage (Poly WASM uses localStorage/IndexedDB when running in Electron).
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
        *self.ws.lock().await = None;

        Ok("Cleared all web storage and reloaded Electron page.\n\
             Call connect_cdp to reconnect — app should restart at the setup wizard."
            .to_string())
    }

    // ── Connectivity ────────────────────────────────────────────────────────

    async fn connect(&self) -> anyhow::Result<String> {
        // Clear any stale connection first.
        *self.ws.lock().await = None;

        // Retry discovering the CDP WebSocket URL.
        // Electron can take a few seconds after launch before CDP is ready.
        let ws_url = {
            let mut last_err = anyhow::anyhow!("CDP not yet available");
            let mut found_url: Option<String> = None;
            for attempt in 1..=10 {
                match self.discover_ws_url().await {
                    Ok(u) => {
                        found_url = Some(u);
                        break;
                    }
                    Err(e) => {
                        last_err = e;
                        tracing::debug!(
                            "connect_cdp: discover attempt {attempt}/10 failed — waiting 1s"
                        );
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
            found_url.ok_or(last_err)?
        };

        // Retry the WebSocket handshake.
        let ws = {
            let mut last_err = anyhow::anyhow!("WebSocket connect failed");
            let mut found_ws: Option<WsStream> = None;
            for attempt in 1..=5 {
                match tokio_tungstenite::connect_async(&ws_url).await {
                    Ok((stream, _)) => {
                        found_ws = Some(stream);
                        break;
                    }
                    Err(e) => {
                        last_err = anyhow::anyhow!("WS attempt {attempt}/5 at {ws_url}: {e}");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
            found_ws.ok_or(last_err)?
        };

        *self.ws.lock().await = Some(ws);

        let generation_num = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::info!("Electron CDP connected (generation {generation_num}): {ws_url}");

        Ok(format!(
            "Connected to Electron CDP ✓  (session #{generation_num})\n\
             WebSocket: {ws_url}"
        ))
    }

    // ── Core primitives ─────────────────────────────────────────────────────

    async fn take_screenshot(&self, params: &ScreenshotParams) -> anyhow::Result<ScreenshotResult> {
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
            // Use layout metrics to capture the full scrollable page.
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

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 1..=5 {
            let _ = self.cdp_send("Page.enable", json!({})).await;
            let _ = self.cdp_send("Page.bringToFront", json!({})).await;

            match self
                .cdp_send("Page.captureScreenshot", cdp_params.clone())
                .await
            {
                Ok(result) => {
                    let b64 = result.get("data").and_then(|v| v.as_str()).ok_or_else(|| {
                        anyhow::anyhow!("No data field in CDP captureScreenshot response")
                    })?;

                    use base64::Engine as _;
                    let image_bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
                    return Ok(ScreenshotResult {
                        image_bytes,
                        mime_type: mime_type.to_string(),
                    });
                }
                Err(err) => {
                    tracing::warn!(
                        "captureScreenshot attempt {attempt}/5 failed: {err}. Retrying…"
                    );
                    last_err = Some(err);
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("captureScreenshot failed")))
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

        // Surface JS exceptions as Rust errors.
        if let Some(exception) = result.get("exceptionDetails") {
            let text = exception
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("Unknown JS exception");
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

    // ── Input ───────────────────────────────────────────────────────────────

    async fn click_at(&self, x: f64, y: f64, dbl_click: bool) -> anyhow::Result<String> {
        let count: i64 = if dbl_click { 2 } else { 1 };
        let xi = x as i64;
        let yi = y as i64;

        for click_num in 1..=count {
            self.cdp_send(
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mousePressed",
                    "x": xi, "y": yi,
                    "button": "left",
                    "clickCount": click_num,
                }),
            )
            .await?;
            self.cdp_send(
                "Input.dispatchMouseEvent",
                json!({
                    "type": "mouseReleased",
                    "x": xi, "y": yi,
                    "button": "left",
                    "clickCount": click_num,
                }),
            )
            .await?;
        }

        // Return a human-readable description of what was clicked.
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
            .await
            .unwrap_or_else(|_| format!("Clicked at ({x},{y})"));

        Ok(info)
    }

    async fn type_text(&self, text: &str, submit_key: Option<&str>) -> anyhow::Result<String> {
        self.cdp_send("Input.insertText", json!({ "text": text }))
            .await?;
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
        Ok(match submit_key {
            Some(k) => format!("Typed \"{text}\" + {k}"),
            None => format!("Typed \"{text}\""),
        })
    }

    // ── Navigation ──────────────────────────────────────────────────────────

    async fn navigate_page(&self, params: &NavigateParams) -> anyhow::Result<String> {
        match params.nav_type.as_str() {
            "url" => {
                let url = params.url.as_deref().unwrap_or("");
                if url.is_empty() {
                    anyhow::bail!("A URL is required for nav_type=url.");
                }
                // For the Electron WASM app running on file://, in-app routing
                // is handled by the Dioxus router via the History API.
                // We push the new state and dispatch a popstate event (same
                // as how browser-based SPAs handle hash/pushState routing).
                let escaped = url.replace('\'', "\\'");
                self.js_eval(&format!(
                    "(function(){{\
                        window.history.pushState(null,'','{escaped}');\
                        window.dispatchEvent(new PopStateEvent('popstate',{{state:null}}));\
                        return 'Navigated to {escaped}';\
                    }})()"
                ))
                .await
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
                // Reload invalidates the CDP session — clear WebSocket for auto-reconnect.
                *self.ws.lock().await = None;
                result.map(|_| "Page reloaded. Call connect_cdp to reconnect.".to_string())
            }
            other => anyhow::bail!(
                "Unknown navigation type: '{other}'. Use url, back, forward, or reload."
            ),
        }
    }

    // ── Extension tools ─────────────────────────────────────────────────────

    fn extension_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "page_reload",
                "description": "Reload the Electron app page (optionally bypassing cache).\n\
                    The CDP session is invalidated after reload — call connect_cdp afterwards.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ignoreCache": {
                            "type": "boolean",
                            "description": "If true, reload without using the browser cache."
                        }
                    },
                    "required": []
                }
            }),
            json!({
                "name": "get_generation",
                "description": "Returns rebuild-detection counters for this Electron MCP session.\n\n\
                    **generation**: increments on each successful connect_cdp call.\n\
                    Starts at 0 before first connect, 1 after first connect, etc.\n\n\
                    **build_id**: increments on each launch_app or rebuild_app call.\n\
                    Reads /tmp/poly-devtools-electron-rebuild-counter.\n\
                    0 = no build triggered yet this process lifetime.\n\n\
                    **electron_pid**: PID of the managed Electron process\n\
                    (null if not launched by this MCP or if Electron has exited).\n\n\
                    Decision table:\n\
                    - All three identical to previous poll → nothing changed\n\
                    - build_id increased, generation same → build triggered, connect_cdp not yet called\n\
                    - build_id increased, generation increased → build + reconnect completed\n\
                    - electron_pid null → Electron is not running",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
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
                let result = self
                    .cdp_send("Page.reload", json!({ "ignoreCache": ignore_cache }))
                    .await
                    .map(|_| {
                        "Page reloaded. Call connect_cdp to re-establish the CDP session."
                            .to_string()
                    });
                // Reload closes the current debugger session — clear WS for auto-reconnect.
                *self.ws.lock().await = None;
                Some(result)
            }
            "get_generation" => {
                let generation_num = self.generation.load(Ordering::Relaxed);
                let pid = *self.electron_pid.lock().await;
                let build_id = Self::read_rebuild_counter();
                Some(Ok(serde_json::json!({
                    "generation": generation_num,
                    "build_id": build_id,
                    "electron_pid": pid,
                })
                .to_string()))
            }
            _ => None,
        }
    }
}

// ─── CLI Mode ─────────────────────────────────────────────────────────────────
//
// PREFERRED: Use the CLI over MCP access wherever possible.
// CLI is faster, scriptable, and doesn't require a Copilot MCP session.
//
// Usage examples:
//   cargo run --bin poly-electron-devtools-mcp -- status
//   cargo run --bin poly-electron-devtools-mcp -- screenshot --save /tmp/shot.png
//   cargo run --bin poly-electron-devtools-mcp -- snapshot
//   cargo run --bin poly-electron-devtools-mcp -- eval "document.title"
//   cargo run --bin poly-electron-devtools-mcp -- launch /path/to/workspace

/// Commands that trigger CLI mode instead of MCP server mode.
const ELECTRON_CLI_COMMANDS: &[&str] = &[
    "status", "launch", "kill", "screenshot", "snapshot",
    "eval", "click", "fill", "navigate", "generation",
    "help", "--help", "-h",
];

/// Check if the first argument selects CLI mode.
fn is_electron_cli_mode(args: &[String]) -> bool {
    args.get(1)
        .map(|a| ELECTRON_CLI_COMMANDS.contains(&a.as_str()))
        .unwrap_or(false)
}

/// Write a line to stdout without `println!`.
fn electron_cli_write(text: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    writeln!(std::io::stdout().lock(), "{text}")?;
    Ok(())
}

/// Electron CLI help text.
fn electron_cli_help() -> &'static str {
    "poly-electron-devtools-mcp — CLI mode (PREFERRED over MCP)

COMMANDS:
  status                    Check CDP connection
  launch [workspace]        Build WASM + launch Electron
  kill                      Stop Electron
  screenshot [--save path]  Take a screenshot
  snapshot [--verbose]      Print DOM snapshot
  eval <script>             Evaluate JavaScript
  click <selector>          Click element
  fill <selector> <value>   Fill input
  navigate <url>            Navigate to URL
  generation                Get rebuild generation counters
  help                      Show this help

MCP mode (default, no subcommand):
  cargo run --bin poly-electron-devtools-mcp
"
}

/// Detect workspace root (POLY_WORKSPACE env var or cwd).
fn electron_detect_workspace() -> String {
    if let Ok(ws) = std::env::var("POLY_WORKSPACE") {
        return ws;
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// Handle `screenshot` CLI command for electron backend.
async fn electron_cli_screenshot(
    backend: &ElectronCdpBackend,
    args: &[String],
) -> anyhow::Result<String> {
    use base64::Engine as _;
    let save_path = args
        .iter()
        .position(|a| a == "--save")
        .and_then(|p| args.get(p + 1))
        .map(String::as_str);
    let params = ScreenshotParams::default();
    let result = backend.take_screenshot(&params).await?;
    if let Some(path) = save_path {
        std::fs::write(path, &result.image_bytes)?;
        Ok(format!("Screenshot saved to {path}"))
    } else {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&result.image_bytes);
        Ok(format!("data:{};base64,{b64}", result.mime_type))
    }
}

/// Dispatch a CLI command for the electron backend.
async fn dispatch_electron_cli(
    backend: &ElectronCdpBackend,
    cmd: &str,
    args: &[String],
) -> anyhow::Result<String> {
    match cmd {
        "status" | "connect" => backend.connect().await,
        "launch" => {
            let ws = args
                .first()
                .map(String::as_str)
                .map(str::to_string)
                .unwrap_or_else(electron_detect_workspace);
            backend.launch_app(&ws).await
        }
        "kill" => backend.kill_app().await,
        "snapshot" => {
            let verbose = args.iter().any(|a| a == "--verbose" || a == "-v");
            backend.take_snapshot(verbose).await
        }
        "eval" => {
            let script = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: eval <script>"))?;
            backend.js_eval(script).await
        }
        "click" => {
            let sel = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: click <selector>"))?;
            backend.click_element(sel).await
        }
        "fill" => {
            let sel = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: fill <selector> <value>"))?;
            let val = args
                .get(1)
                .ok_or_else(|| anyhow::anyhow!("Usage: fill <selector> <value>"))?;
            backend.fill_element(sel, val).await
        }
        "navigate" => {
            let url = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("Usage: navigate <url>"))?;
            backend
                .navigate_page(&NavigateParams {
                    nav_type: "url".to_string(),
                    url: Some(url.to_string()),
                    ..Default::default()
                })
                .await
        }
        "generation" => backend.connect().await,
        "screenshot" => electron_cli_screenshot(backend, args).await,
        _ => Ok(electron_cli_help().to_string()),
    }
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

    let args: Vec<String> = std::env::args().collect();

    if is_electron_cli_mode(&args) {
        let cmd = args.get(1).map(String::as_str).unwrap_or("help");
        let rest = args.get(2..).unwrap_or(&[]).to_vec();
        let backend = ElectronCdpBackend::new();
        match dispatch_electron_cli(&backend, cmd, &rest).await {
            Ok(out) => {
                if let Err(e) = electron_cli_write(&out) {
                    use std::io::Write as _;
                    let _ = writeln!(std::io::stderr().lock(), "Output error: {e}");
                }
            }
            Err(e) => {
                use std::io::Write as _;
                let _ = writeln!(std::io::stderr().lock(), "Error: {e}");
                std::process::exit(1);
            }
        }
    } else {
        tracing::info!(
            "Starting poly-electron-devtools-mcp (CDP port {})",
            CDP_PORT
        );
        let backend = ElectronCdpBackend::new();
        run_mcp_loop(&backend, "poly-electron").await;
    }
}
