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
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use poly_devtools_protocol::backend::{
    BuildDiagnostics, BuildLifecycleState, DevtoolsBackend, NavigateParams, RollingBuildLog,
    ScreenshotParams, ScreenshotResult,
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

/// Port that `dx serve --platform web` listens on for the Electron WASM build.
/// Electron loads from `http://127.0.0.1:DX_SERVE_PORT/` in dev mode.
const DX_SERVE_PORT: u16 = 3001;
/// Path the dx-serve fullstack server uses to serve the compiled WASM loader.
/// 200 here = the wasm half actually finished; the server half (which serves
/// `/host/status`) often comes up many seconds before wasm is ready, and
/// sometimes the wasm half silently fails to compile at all.
const WASM_BUNDLE_PATH: &str = "/assets/dioxus/poly-desktop-electron.js";
/// Once the server is responding but the bundle is still 404 for >this many
/// seconds, treat it as a silent-hang and abort with actionable guidance.
const SILENT_HANG_THRESHOLD_SECS: u64 = 60;

/// Rebuild counter file — incremented by `launch_app` and `rebuild_app`.
/// Separate from desktop (`…rebuild-counter`) and web (`…web-rebuild-counter`).
const REBUILD_COUNTER_PATH: &str = "/tmp/poly-devtools-electron-rebuild-counter";
const BUILD_LOG_EXCERPT_LINES: usize = 60;
const CDP_SEND_TIMEOUT_SECS: u64 = 5;
const CDP_RESPONSE_TIMEOUT_SECS: u64 = 15;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Internal build record tracked by the Electron backend.
#[derive(Debug, Clone)]
struct ElectronBuildRecord {
    diagnostics: BuildDiagnostics,
    log_start_seq: u64,
}

fn unix_now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn excerpt_from_lines(lines: &[String]) -> String {
    let total = lines.len();
    let start = total.saturating_sub(BUILD_LOG_EXCERPT_LINES);
    lines
        .iter()
        .skip(start)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── Backend ─────────────────────────────────────────────────────────────────

/// Electron CDP backend.
///
/// Builds the WASM bundle once, launches Electron with CDP enabled, then drives
/// the app via WebSocket CDP commands.
///
/// `Clone` is derived so background tokio tasks can hold an owned copy that
/// shares all `Arc`-wrapped state with the main backend instance.
#[derive(Clone)]
struct ElectronCdpBackend {
    /// Active CDP WebSocket connection (`None` when disconnected or after reload).
    ws: Arc<Mutex<Option<WsStream>>>,
    /// Auto-incrementing CDP message ID.
    msg_id: Arc<AtomicI64>,
    /// HTTP client used for CDP target discovery (`/json`).
    client: reqwest::Client,
    /// PID of the managed Electron process (`None` if not launched by us or already exited).
    electron_pid: Arc<Mutex<Option<u32>>>,
    /// Workspace root path — set in `launch_app`, reused by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
    /// Generation counter — increments on each successful `connect()` call.
    generation: Arc<AtomicU64>,
    /// Rolling combined build log for dx build output.
    build_log: Arc<Mutex<RollingBuildLog>>,
    /// Structured diagnostics for the last dx build attempt.
    last_build: Arc<Mutex<Option<ElectronBuildRecord>>>,
    /// Active background build task — guards against concurrent builds.
    build_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// PID of the managed `dx serve --platform web` process.
    dx_serve_pid: Arc<Mutex<Option<u32>>>,
    /// Set to `true` on MCP shutdown — stops any Electron watchdog from restarting.
    shutting_down: Arc<AtomicBool>,
}

impl ElectronCdpBackend {
    fn new() -> Self {
        Self {
            ws: Arc::new(Mutex::new(None)),
            msg_id: Arc::new(AtomicI64::new(1)),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            electron_pid: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
            build_log: Arc::new(Mutex::new(RollingBuildLog::default())),
            last_build: Arc::new(Mutex::new(None)),
            build_task: Arc::new(Mutex::new(None)),
            dx_serve_pid: Arc::new(Mutex::new(None)),
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn start_build_record(
        &self,
        trigger: &str,
        mode: &str,
        working_directory: &str,
        command_line: &str,
        summary: &str,
    ) {
        let start_seq = {
            let mut buffer = self.build_log.lock().await;
            let seq = buffer.next_sequence();
            buffer.push_line(format!(
                "[meta] trigger={trigger} mode={mode} command={command_line} cwd={working_directory}"
            ));
            seq
        };
        let diagnostics = BuildDiagnostics {
            backend: self.name().to_string(),
            trigger: trigger.to_string(),
            mode: mode.to_string(),
            working_directory: working_directory.to_string(),
            command_line: command_line.to_string(),
            state: BuildLifecycleState::Running,
            summary: summary.to_string(),
            verification: "Build command started; waiting for dx build to finish.".to_string(),
            exit_code: None,
            started_at_unix_ms: Some(unix_now_ms()),
            finished_at_unix_ms: None,
            duration_ms: None,
            build_id_before: Some(Self::read_rebuild_counter()),
            build_id_after: None,
            generation_before: Some(self.generation.load(Ordering::Relaxed)),
            generation_after: None,
            process_id_before: *self.electron_pid.lock().await,
            process_id_after: None,
            log_line_count: 0,
            log_excerpt: String::new(),
        };
        *self.last_build.lock().await = Some(ElectronBuildRecord {
            diagnostics,
            log_start_seq: start_seq,
        });
    }

    async fn finish_build_record(
        &self,
        state: BuildLifecycleState,
        summary: impl Into<String>,
        verification: impl Into<String>,
        exit_code: Option<i32>,
    ) {
        let Some(log_start_seq) = self
            .last_build
            .lock()
            .await
            .as_ref()
            .map(|record| record.log_start_seq)
        else {
            return;
        };
        let lines = self.build_log.lock().await.lines_since(log_start_seq);
        let log_excerpt = excerpt_from_lines(&lines);
        let now = unix_now_ms();
        let generation_after = self.generation.load(Ordering::Relaxed);
        let pid_after = *self.electron_pid.lock().await;

        if let Some(record) = self.last_build.lock().await.as_mut() {
            let duration_ms = record
                .diagnostics
                .started_at_unix_ms
                .map(|started| now.saturating_sub(started));
            record.diagnostics.state = state;
            record.diagnostics.summary = summary.into();
            record.diagnostics.verification = verification.into();
            record.diagnostics.exit_code = exit_code;
            record.diagnostics.finished_at_unix_ms = Some(now);
            record.diagnostics.duration_ms = duration_ms;
            record.diagnostics.build_id_after = Some(Self::read_rebuild_counter());
            record.diagnostics.generation_after = Some(generation_after);
            record.diagnostics.process_id_after = pid_after;
            record.diagnostics.log_line_count = lines.len();
            record.diagnostics.log_excerpt = log_excerpt;
        }
    }

    async fn note_successful_connect(&self) {
        let record_snapshot = self.last_build.lock().await.clone();
        let Some(record_snapshot) = record_snapshot else {
            return;
        };
        if !matches!(
            record_snapshot.diagnostics.state,
            BuildLifecycleState::Running | BuildLifecycleState::Unknown
        ) {
            return;
        }

        let lines = self
            .build_log
            .lock()
            .await
            .lines_since(record_snapshot.log_start_seq);
        let now = unix_now_ms();
        let generation_after = self.generation.load(Ordering::Relaxed);
        let pid_after = *self.electron_pid.lock().await;
        if let Some(record) = self.last_build.lock().await.as_mut() {
            record.diagnostics.state = BuildLifecycleState::Succeeded;
            record.diagnostics.summary =
                "Electron CDP reconnected successfully after the most recent build.".to_string();
            record.diagnostics.verification =
                "Verified by a successful connect_cdp after the build/reload workflow.".to_string();
            record.diagnostics.finished_at_unix_ms = Some(now);
            record.diagnostics.duration_ms = record
                .diagnostics
                .started_at_unix_ms
                .map(|started| now.saturating_sub(started));
            record.diagnostics.generation_after = Some(generation_after);
            record.diagnostics.process_id_after = pid_after;
            record.diagnostics.log_line_count = lines.len();
            record.diagnostics.log_excerpt = excerpt_from_lines(&lines);
            record.diagnostics.build_id_after = Some(Self::read_rebuild_counter());
        }
    }

    async fn last_build_status_json(&self) -> anyhow::Result<String> {
        let record = self.last_build.lock().await.clone();
        if let Some(record) = record {
            let mut diagnostics = record.diagnostics;
            let lines = self
                .build_log
                .lock()
                .await
                .lines_since(record.log_start_seq);
            diagnostics.log_line_count = lines.len();
            diagnostics.log_excerpt = excerpt_from_lines(&lines);
            return serde_json::to_string_pretty(&diagnostics)
                .map_err(|e| anyhow::anyhow!("serialize build diagnostics: {e}"));
        }

        let diagnostics = BuildDiagnostics {
            backend: self.name().to_string(),
            trigger: "none".to_string(),
            mode: "dx build --platform web".to_string(),
            working_directory: String::new(),
            command_line: String::new(),
            state: BuildLifecycleState::NotStarted,
            summary: "No Dioxus build has been recorded yet in this MCP session.".to_string(),
            verification: "Call launch_app or rebuild_app first.".to_string(),
            exit_code: None,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            duration_ms: None,
            build_id_before: Some(Self::read_rebuild_counter()),
            build_id_after: Some(Self::read_rebuild_counter()),
            generation_before: Some(self.generation.load(Ordering::Relaxed)),
            generation_after: Some(self.generation.load(Ordering::Relaxed)),
            process_id_before: *self.electron_pid.lock().await,
            process_id_after: *self.electron_pid.lock().await,
            log_line_count: 0,
            log_excerpt: String::new(),
        };
        serde_json::to_string_pretty(&diagnostics)
            .map_err(|e| anyhow::anyhow!("serialize build diagnostics: {e}"))
    }

    async fn last_build_log_text(&self) -> anyhow::Result<String> {
        let record = self.last_build.lock().await.clone();
        if let Some(record) = record {
            let lines = self
                .build_log
                .lock()
                .await
                .lines_since(record.log_start_seq);
            return Ok(if lines.is_empty() {
                "<no Dioxus build output captured yet for the last attempt>".to_string()
            } else {
                lines.join("\n")
            });
        }

        let tail = self
            .build_log
            .lock()
            .await
            .tail_lines(BUILD_LOG_EXCERPT_LINES);
        Ok(if tail.is_empty() {
            "<no Dioxus build has been recorded yet in this MCP session>".to_string()
        } else {
            tail.join("\n")
        })
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

        if tokio::time::timeout(
            Duration::from_secs(CDP_SEND_TIMEOUT_SECS),
            ws.send(Message::Text(serde_json::to_string(&msg)?.into())),
        )
        .await
        .is_err()
        {
            drop(ws_guard);
            *self.ws.lock().await = None;
            anyhow::bail!(
                "CDP send timeout for method '{method}' after {CDP_SEND_TIMEOUT_SECS}s. The Electron renderer may be hung; call connect_cdp to retry."
            );
        }

        let response =
            tokio::time::timeout(Duration::from_secs(CDP_RESPONSE_TIMEOUT_SECS), async {
                // Read messages until we get our response (matching id).
                // Other messages (CDP events) are skipped.
                loop {
                    let Some(Ok(raw)) = ws.next().await else {
                        anyhow::bail!(
                            "CDP WebSocket closed unexpectedly. \
                             Electron may have crashed. Call connect_cdp to reconnect."
                        );
                    };

                    let text = match raw {
                        Message::Text(t) => t.to_string(),
                        Message::Close(_) => {
                            anyhow::bail!(
                                "CDP WebSocket closed (Electron closed or page reloaded). \
                                 Call connect_cdp to reconnect."
                            );
                        }
                        Message::Binary(_)
                        | Message::Ping(_)
                        | Message::Pong(_)
                        | Message::Frame(_) => continue,
                    };

                    let resp: Value = serde_json::from_str(&text)?;
                    if resp.get("id").and_then(Value::as_i64) == Some(id) {
                        if let Some(err) = resp.get("error") {
                            anyhow::bail!("CDP error from method '{method}': {err}");
                        }
                        return Ok(resp.get("result").cloned().unwrap_or(json!({})));
                    }
                    // Not our response — CDP event or another command's response, skip.
                }
            })
            .await;

        match response {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(err)) => {
                drop(ws_guard);
                *self.ws.lock().await = None;
                Err(err)
            }
            Err(_) => {
                drop(ws_guard);
                *self.ws.lock().await = None;
                anyhow::bail!(
                    "CDP response timeout for method '{method}' after {CDP_RESPONSE_TIMEOUT_SECS}s. The Electron page may be frozen; call connect_cdp to reconnect once it recovers."
                );
            }
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
        drop(std::fs::write(
            path,
            current.saturating_add(1).to_string(),
        ));
    }

    /// Read the current value of the rebuild counter.
    fn read_rebuild_counter() -> u64 {
        std::fs::read_to_string(REBUILD_COUNTER_PATH)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Hide the transient Dioxus rebuild toast when the real app root is already visible.
    async fn suppress_rebuild_toast_if_app_ready(&self) {
        let _ignored = self
            .cdp_send(
                "Runtime.evaluate",
                json!({
                    "expression": r#"(function(){
                        var appRoot = document.querySelector('#main');
                        var toast = document.querySelector('#__dx-toast');
                        if (appRoot && toast) {
                            toast.style.display = 'none';
                            toast.setAttribute('data-poly-hidden-rebuild-toast', 'true');
                            return JSON.stringify({ hidden: true, reason: 'app-root-present' });
                        }
                        return JSON.stringify({ hidden: false, appRoot: !!appRoot, toast: !!toast });
                    })()"#,
                    "returnByValue": true,
                    "awaitPromise": true,
                }),
            )
            .await;
    }

    // ── Port / process helpers ──────────────────────────────────────────────

    /// Poll the dx-serve fullstack endpoint for the actual WASM bundle
    /// (not just port-up). Detects the silent-hang case where the server
    /// half binds and answers `/host/status` for many seconds but the wasm
    /// half never compiles. Returns Ok on first 200 from the bundle path.
    async fn wait_for_wasm_bundle(&self, port: u16, max_seconds: u64) -> anyhow::Result<()> {
        let bundle_url = format!("http://127.0.0.1:{port}{WASM_BUNDLE_PATH}");
        let port_url = format!("http://127.0.0.1:{port}/host/status");
        let polls = max_seconds.saturating_mul(2);
        let mut port_up_since: Option<std::time::Instant> = None;
        for _ in 0..polls {
            let bundle_status = self
                .client
                .get(&bundle_url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .map(|r| r.status().as_u16())
                .unwrap_or(0);
            if bundle_status == 200 {
                return Ok(());
            }
            let port_up = self
                .client
                .get(&port_url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if port_up && port_up_since.is_none() {
                port_up_since = Some(std::time::Instant::now());
            }
            if let Some(t0) = port_up_since {
                if t0.elapsed().as_secs() > SILENT_HANG_THRESHOLD_SECS && bundle_status == 404 {
                    anyhow::bail!(
                        "dx serve appears wedged: server on port {port} has been answering \
                         for >{SILENT_HANG_THRESHOLD_SECS}s but {WASM_BUNDLE_PATH} is still \
                         404. The wasm half of dx serve sometimes silently fails to invoke \
                         cargo build.\n\n\
                         Fix: run\n  \
                         cd apps/desktop-electron && cargo build \
                         --target wasm32-unknown-unknown\n\
                         to surface the real compile error, then retry launch_app."
                    );
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        anyhow::bail!(
            "WASM bundle {WASM_BUNDLE_PATH} not served by port {port} within {max_seconds}s. \
             Call get_last_build_log to inspect the build output."
        )
    }

    /// Kill the tracked `dx serve` process and wait briefly for it to exit.
    async fn kill_dx_serve(&self) {
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
            drop(tokio::process::Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status()
                .await);
        }
        // Pattern fallback.
        drop(tokio::process::Command::new("pkill")
            .args(["-f", &format!("dx.*serve.*--port.*{DX_SERVE_PORT}")])
            .status()
            .await);
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    // ── Background task helpers ─────────────────────────────────────────────
    // These take `self` by value (via `self.clone()` at the call site) so they
    // can be dropped into `tokio::spawn` without lifetime issues.

    /// Background task body for `launch_app`.
    ///
    /// Starts `dx serve --platform web --port DX_SERVE_PORT`, waits for the
    /// dev server to become ready, then launches Electron with `POLY_DEV=1` so
    /// it loads from the live dev server.  Electron stays alive across rebuilds.
    async fn bg_serve_and_launch_electron(
        self,
        app_dir: String,
        electron_dir: String,
    ) {
        tracing::info!(
            "[bg] dx serve --platform web --port {DX_SERVE_PORT} --fullstack  in {app_dir}"
        );

        // ── Spawn dx serve (long-running, fullstack) ─────────────────────
        //
        // apps/desktop-electron is a Dioxus fullstack app: the server half
        // merges `poly_host::router(state)` into the Dioxus router so `/host/*`
        // is served on the SAME port as the WASM bundle. Electron's renderer
        // loads http://127.0.0.1:DX_SERVE_PORT/ and reaches the host bridge
        // there. The `@server --platform server` split is REQUIRED, otherwise
        // dx builds the server for wasm32-unknown-unknown and fails.
        let mut serve_child = match tokio::process::Command::new("dx")
            .args([
                "serve",
                "--platform",
                "web",
                "--port",
                &DX_SERVE_PORT.to_string(),
                "--fullstack",
                "@client",
                "--no-default-features",
                "--features",
                "dev-plugins,web",
                "@server",
                "--platform",
                "server",
                "--no-default-features",
                "--features",
                "dev-plugins,server",
            ])
            .current_dir(&app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[bg] Failed to spawn dx serve: {e}");
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    format!("Failed to spawn `dx serve`: {e}"),
                    "dx serve could not be spawned. Is dx installed and on PATH?",
                    None,
                )
                .await;
                return;
            }
        };

        // Stream dx serve stdout/stderr into the rolling build log.
        if let Some(stdout) = serve_child.stdout.take() {
            let buf = self.build_log.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt as _;
                let mut lines = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    buf.lock().await.push_line(format!("[dx-stdout] {line}"));
                }
            });
        }
        if let Some(stderr) = serve_child.stderr.take() {
            let buf = self.build_log.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt as _;
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    buf.lock().await.push_line(format!("[dx-stderr] {line}"));
                }
            });
        }

        let serve_pid = serve_child.id();
        *self.dx_serve_pid.lock().await = serve_pid;

        // Auto-clear when dx serve exits.
        let pid_ref = self.dx_serve_pid.clone();
        tokio::spawn(async move {
            drop(serve_child.wait().await);
            *pid_ref.lock().await = None;
            tracing::info!("[bg] dx serve exited");
        });

        Self::increment_rebuild_counter();

        // ── Wait for dx serve to actually serve the WASM bundle ───────────
        // Up to 600 s for cold builds. wait_for_wasm_bundle polls the bundle
        // path (not just port-up) and bails early when the server binds but
        // wasm silently never compiles.
        match self.wait_for_wasm_bundle(DX_SERVE_PORT, 600).await {
            Ok(()) => tracing::info!("[bg] dx serve WASM bundle is live on port {DX_SERVE_PORT}"),
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve did not serve the WASM bundle.",
                    format!("{e}"),
                    None,
                )
                .await;
                return;
            }
        }

        // ── Launch Electron with POLY_DEV=1 ───────────────────────────────
        // Use the electron binary from the original app's node_modules so that
        // the thin shell's node_modules (if any) don't shadow the built-in
        // `require('electron')` module inside main.js.
        let original_electron = format!("{app_dir}/electron/node_modules/.bin/electron");
        let local_electron = format!("{electron_dir}/node_modules/.bin/electron");
        let mut cmd = if std::path::Path::new(&original_electron).exists() {
            tracing::info!("[bg] Using original app electron binary: {original_electron}");
            tokio::process::Command::new(&original_electron)
        } else if std::path::Path::new(&local_electron).exists() {
            tracing::info!("[bg] Using thin-shell electron binary: {local_electron}");
            tokio::process::Command::new(&local_electron)
        } else {
            tracing::info!("[bg] Falling back to npx electron");
            let mut c = tokio::process::Command::new("npx");
            c.arg("electron");
            c
        };

        // Synthetic audio/video for cross-shell voice E2E smoke tests:
        // auto-accept getUserMedia permission prompts and return deterministic
        // fake streams ("Fake Audio 1" / "Fake Video 1") without requiring a
        // real mic or camera. Electron forwards these Chromium flags when they
        // appear before the app-directory argument.
        // See docs/plans/plan-voice-media-plane-e2e.md.
        cmd.arg("--use-fake-ui-for-media-stream")
            .arg("--use-fake-device-for-media-stream")
            .arg(&electron_dir)
            .current_dir(&electron_dir)
            .env("POLY_DEV", "1")
            .env("POLY_DEV_SERVE_PORT", DX_SERVE_PORT.to_string())
            .env(
                "POLY_ELECTRON_REMOTE_DEBUGGING_PORT",
                CDP_PORT.to_string(),
            )
            // ELECTRON_RUN_AS_NODE makes Electron behave as plain Node.js,
            // suppressing all Electron bindings.  Unset it so the process
            // starts as a proper Electron browser process.
            .env_remove("ELECTRON_RUN_AS_NODE")
            .env("ELECTRON_DISABLE_SANDBOX", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[bg] Failed to spawn Electron: {e}");
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    format!("dx serve is ready but Electron launch failed: {e}"),
                    "Electron process could not be spawned.",
                    None,
                )
                .await;
                return;
            }
        };

        let pid = child.id();
        *self.electron_pid.lock().await = pid;

        // Reap in background so it doesn't become a zombie.
        let pid_ref = self.electron_pid.clone();
        let shutting_down = self.shutting_down.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            let exit_code = status.as_ref().ok().and_then(std::process::ExitStatus::code);
            *pid_ref.lock().await = None;
            if !shutting_down.load(Ordering::Relaxed) && exit_code != Some(0_i32) {
                tracing::warn!(
                    "Electron exited unexpectedly (code {exit_code:?}). \
                     Call launch_app to restart."
                );
            } else {
                tracing::info!("Electron process exited (code {exit_code:?})");
            }
        });

        self.finish_build_record(
            BuildLifecycleState::Succeeded,
            format!(
                "dx serve ready on port {DX_SERVE_PORT}. \
                 Electron launched with CDP on port {CDP_PORT} (PID: {pid:?}). \
                 Electron stays alive across rebuilds — use rebuild_app to update WASM."
            ),
            "dx serve is running and Electron is connected to it. \
             Call connect_cdp (wait ~3 s first). \
             Use rebuild_app to recompile WASM — Electron window will NOT restart.",
            None,
        )
        .await;
        tracing::info!("[bg] Electron launched (PID: {pid:?}). dx serve PID: {serve_pid:?}.");
    }

    /// Background task body for `rebuild_app`.
    ///
    /// Restarts `dx serve --platform web` (recompiles WASM) then sends a CDP
    /// `Page.reload` so Electron picks up the fresh bundle.  Electron is NOT
    /// killed — the window stays alive, only the page content reloads.
    async fn bg_restart_serve_and_reload(self, app_dir: String) {
        tracing::info!(
            "[bg] rebuild: killing dx serve, restarting dx serve --platform web --port {DX_SERVE_PORT}"
        );

        // ── Kill current dx serve ─────────────────────────────────────────
        self.kill_dx_serve().await;

        // ── Restart dx serve (fullstack — see bg_serve_and_launch_electron) ──
        let mut serve_child = match tokio::process::Command::new("dx")
            .args([
                "serve",
                "--platform",
                "web",
                "--port",
                &DX_SERVE_PORT.to_string(),
                "--fullstack",
                "@client",
                "--no-default-features",
                "--features",
                "dev-plugins,web",
                "@server",
                "--platform",
                "server",
                "--no-default-features",
                "--features",
                "dev-plugins,server",
            ])
            .current_dir(&app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[bg] Failed to spawn dx serve for rebuild: {e}");
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    format!("Failed to restart `dx serve`: {e}"),
                    "dx serve could not be spawned.",
                    None,
                )
                .await;
                return;
            }
        };

        if let Some(stdout) = serve_child.stdout.take() {
            let buf = self.build_log.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt as _;
                let mut lines = tokio::io::BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    buf.lock().await.push_line(format!("[dx-stdout] {line}"));
                }
            });
        }
        if let Some(stderr) = serve_child.stderr.take() {
            let buf = self.build_log.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt as _;
                let mut lines = tokio::io::BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    buf.lock().await.push_line(format!("[dx-stderr] {line}"));
                }
            });
        }

        let serve_pid = serve_child.id();
        *self.dx_serve_pid.lock().await = serve_pid;

        let pid_ref = self.dx_serve_pid.clone();
        tokio::spawn(async move {
            drop(serve_child.wait().await);
            *pid_ref.lock().await = None;
        });

        // ── Wait for the WASM bundle to come back ─────────────────────────
        match self.wait_for_wasm_bundle(DX_SERVE_PORT, 600).await {
            Ok(()) => tracing::info!("[bg] dx serve WASM bundle live on port {DX_SERVE_PORT} after rebuild"),
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve did not serve the WASM bundle after rebuild.",
                    format!("{e}"),
                    None,
                )
                .await;
                return;
            }
        }

        // ── Reload Electron page via CDP (window stays alive) ─────────────
        let reload_ok = self
            .cdp_send("Page.reload", json!({ "ignoreCache": true }))
            .await
            .is_ok();
        *self.ws.lock().await = None;

        self.finish_build_record(
            BuildLifecycleState::Succeeded,
            if reload_ok {
                format!(
                    "WASM recompiled and Electron page reloaded in-place (dx serve PID {serve_pid:?}). \
                     Electron window survived the rebuild."
                )
            } else {
                "WASM recompiled. CDP reload failed — call connect_cdp to reconnect Electron.".to_string()
            },
            if reload_ok {
                "Page reload sent. Call connect_cdp to get a fresh CDP session."
            } else {
                "dx serve is ready but CDP reload failed. Electron may have crashed — use launch_app."
            },
            None,
        )
        .await;
        tracing::info!("[bg] Rebuild done. reload_ok={reload_ok}");
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
        // Guard against concurrent builds.
        {
            let guard = self.build_task.lock().await;
            if let Some(handle) = guard.as_ref()
                && !handle.is_finished()
            {
                return Ok("A build is already in progress.\n\
                     Poll get_last_build_status until state = \"Succeeded\" or \"Failed\"."
                    .to_string());
            }
        }

        *self.workspace.lock().await = Some(workspace.to_string());
        let app_dir = format!("{workspace}/apps/desktop-electron");
        let electron_dir = format!("{workspace}/apps/desktop-electron-web/electron");

        // Kill existing dx serve and ALL Electron processes from this app.
        self.kill_dx_serve().await;
        if let Some(pid) = self.electron_pid.lock().await.take() {
            drop(tokio::process::Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status()
                .await);
        }
        // Kill orphaned Electron processes from previous MCP sessions.
        // Match by CDP port (renderer) AND by the thin-shell app path (catches
        // main, GPU, network utility, and renderer processes).
        drop(tokio::process::Command::new("pkill")
            .args(["-f", &format!("remote-debugging-port={CDP_PORT}")])
            .status()
            .await);
        drop(tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-electron-web"])
            .status()
            .await);
        tokio::time::sleep(Duration::from_millis(600)).await;

        // Drop the stale CDP connection.
        *self.ws.lock().await = None;

        self.start_build_record(
            "launch_app",
            &format!("dx serve --platform web --port {DX_SERVE_PORT}"),
            &app_dir,
            &format!("dx serve --platform web --port {DX_SERVE_PORT}"),
            "Starting dx serve (long-running). Electron will load from the live dev server.",
        )
        .await;

        let ctx = self.clone();
        let handle = tokio::spawn(ctx.bg_serve_and_launch_electron(app_dir, electron_dir));
        *self.build_task.lock().await = Some(handle);

        Ok(format!(
            "🔧 dx serve started in background (state: Running).\n\
             Command: dx serve --platform web --port {DX_SERVE_PORT}  (in apps/desktop-electron/)\n\
             First compile takes 30-90 s.\n\
             \n\
             Poll get_last_build_status every 5-10 s:\n\
               state = \"Running\"   → keep polling\n\
               state = \"Succeeded\" → Electron is running, call connect_cdp (wait ~3 s first)\n\
               state = \"Failed\"    → call get_last_build_log for the compiler error\n\
             \n\
             ✨ After first launch: use rebuild_app to recompile WASM — Electron window stays alive."
        ))
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        *self.ws.lock().await = None;
        self.kill_dx_serve().await;

        if let Some(pid) = self.electron_pid.lock().await.take() {
            drop(tokio::process::Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status()
                .await);
        }
        drop(tokio::process::Command::new("pkill")
            .args(["-f", "remote-debugging-port=9224"])
            .status()
            .await);
        drop(tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-electron-web"])
            .status()
            .await);

        Ok("Killed dx serve and Electron. Call launch_app to restart.".to_string())
    }

    async fn hard_kill(&self) -> anyhow::Result<String> {
        *self.ws.lock().await = None;
        self.kill_dx_serve().await;

        if let Some(pid) = self.electron_pid.lock().await.take() {
            drop(tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await);
        }
        drop(tokio::process::Command::new("pkill")
            .args(["-9", "-f", "remote-debugging-port=9224"])
            .status()
            .await);
        drop(tokio::process::Command::new("pkill")
            .args(["-9", "-f", "poly-desktop-electron-web"])
            .status()
            .await);

        Ok("Hard-killed dx serve and Electron (SIGKILL). Call launch_app to restart.".to_string())
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Guard against concurrent builds.
        {
            let guard = self.build_task.lock().await;
            if let Some(handle) = guard.as_ref()
                && !handle.is_finished()
            {
                return Ok("A build is already in progress.\n\
                     Poll get_last_build_status until state = \"Succeeded\" or \"Failed\"."
                    .to_string());
            }
        }

        let app_dir = format!("{workspace}/apps/desktop-electron");

        // Increment counter now so build_id advances immediately.
        Self::increment_rebuild_counter();

        self.start_build_record(
            "rebuild_app",
            &format!("dx serve --platform web --port {DX_SERVE_PORT}"),
            &app_dir,
            &format!("restart dx serve --platform web --port {DX_SERVE_PORT}"),
            "Restarting dx serve to recompile WASM. Electron window will NOT restart.",
        )
        .await;

        let ctx = self.clone();
        let handle = tokio::spawn(ctx.bg_restart_serve_and_reload(app_dir));
        *self.build_task.lock().await = Some(handle);

        Ok(format!(
            "🔧 WASM rebuild started in background (state: Running).\n\
             Restarting: dx serve --platform web --port {DX_SERVE_PORT}\n\
             ✨ Electron window stays alive — only the page content reloads.\n\
             \n\
             Poll get_last_build_status every 5-10 s:\n\
               state = \"Running\"   → keep polling\n\
               state = \"Succeeded\" → call connect_cdp to get a fresh CDP session\n\
               state = \"Failed\"    → call get_last_build_log for the compiler error"
        ))
    }

    async fn get_last_build_status(&self) -> anyhow::Result<String> {
        self.last_build_status_json().await
    }

    async fn get_last_build_log(&self) -> anyhow::Result<String> {
        self.last_build_log_text().await
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
            for attempt in 1_u32..=10_u32 {
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
            for attempt in 1_u32..=5_u32 {
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
        self.note_successful_connect().await;
        self.suppress_rebuild_toast_if_app_ready().await;

        Ok(format!(
            "Connected to Electron CDP ✓  (session #{generation_num})\n\
             WebSocket: {ws_url}"
        ))
    }

    // ── Core primitives ─────────────────────────────────────────────────────

    async fn take_screenshot(&self, params: &ScreenshotParams) -> anyhow::Result<ScreenshotResult> {
        self.suppress_rebuild_toast_if_app_ready().await;

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
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1440.0_f64);
                let height = content_size
                    .get("height")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(900.0_f64);
                if let Some(m) = cdp_params.as_object_mut() {
                    m.insert(
                        "clip".to_string(),
                        json!({"x": 0_i32, "y": 0_i32, "width": width, "height": height, "scale": 1_i32}),
                    );
                }
            }
        }

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 1_u32..=5_u32 {
            drop(self.cdp_send("Page.enable", json!({})).await);
            drop(self.cdp_send("Page.bringToFront", json!({})).await);

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
        self.suppress_rebuild_toast_if_app_ready().await;

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
            other @ (Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Array(_)
            | Value::Object(_)) => other.to_string(),
        })
    }

    // ── Input ───────────────────────────────────────────────────────────────

    async fn click_at(&self, x: f64, y: f64, dbl_click: bool) -> anyhow::Result<String> {
        let count: i64 = if dbl_click { 2_i64 } else { 1_i64 };
        // CSS-pixel viewport coords are bounded; CDP wants integers.
        fn f64_to_i64(v: f64) -> i64 {
            if v.is_nan() {
                return 0;
            }
            if v >= 9_223_372_036_854_775_807.0_f64 {
                return i64::MAX;
            }
            if v <= -9_223_372_036_854_775_808.0_f64 {
                return i64::MIN;
            }
            // SAFETY: bounds checked above.
            // lint-allow-unused: bounds checked + intentional truncation
            #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
            let out = v.round() as i64;
            out
        }
        let xi = f64_to_i64(x);
        let yi = f64_to_i64(y);

        for click_num in 1_i64..=count {
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
                    - electron_pid null → Electron is not running\n\
                    If generation/build_id do not move the way you expect, immediately inspect get_last_build_status and get_last_build_log for the exact Dioxus CLI output.",
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
                    .and_then(serde_json::Value::as_bool)
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
    "status",
    "launch",
    "rebuild",
    "kill",
    "screenshot",
    "snapshot",
    "eval",
    "click",
    "fill",
    "navigate",
    "generation",
    "build-status",
    "build-log",
    "help",
    "--help",
    "-h",
];

/// Check if the first argument selects CLI mode.
fn is_electron_cli_mode(args: &[String]) -> bool {
    args.get(1)
        .is_some_and(|a| ELECTRON_CLI_COMMANDS.contains(&a.as_str()))
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
  rebuild [workspace]       Rebuild WASM + reload Electron (non-blocking, polls until done)
  kill                      Stop Electron
  screenshot [--save path]  Take a screenshot
  snapshot [--verbose]      Print DOM snapshot
  eval <script>             Evaluate JavaScript
  click <selector>          Click element
  fill <selector> <value>   Fill input
  navigate <url>            Navigate to URL
  generation                Get rebuild generation counters
    build-status              Get structured diagnostics for the last Dioxus build/rebuild
    build-log                 Get the raw log for the last Dioxus build/rebuild
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
    std::env::current_dir().map_or_else(
        |_| ".".to_string(),
        |p| p.to_string_lossy().into_owned(),
    )
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
        .and_then(|p| args.get(p.saturating_add(1)))
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
        // `generation` originally had its own handler; it now alias-resolves to
        // `connect` like `status`, merged here per clippy::match_same_arms.
        "status" | "connect" | "generation" => backend.connect().await,
        "launch" => {
            let ws = args
                .first()
                .map_or_else(electron_detect_workspace, std::clone::Clone::clone);
            // launch_app is non-blocking — it returns immediately.  In CLI mode we
            // must poll until the background build finishes, otherwise the process
            // exits and kills the spawned task before the build completes.
            let initial_msg = backend.launch_app(&ws).await?;
            electron_cli_write(&initial_msg)?;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let status_str = backend.get_last_build_status().await.unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&status_str).unwrap_or(serde_json::Value::Null);
                let state = parsed
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                electron_cli_write(&format!("[build] state = {state}"))?;
                if state != "Running" {
                    break;
                }
            }
            backend.get_last_build_status().await
        }
        "kill" => backend.kill_app().await,
        "rebuild" => {
            let ws = args
                .first()
                .map_or_else(electron_detect_workspace, std::clone::Clone::clone);
            // rebuild_app is non-blocking — poll until background build finishes.
            let initial_msg = backend.rebuild_app(&ws).await?;
            electron_cli_write(&initial_msg)?;
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let status_str = backend.get_last_build_status().await.unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&status_str).unwrap_or(serde_json::Value::Null);
                let state = parsed
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                electron_cli_write(&format!("[rebuild] state = {state}"))?;
                if state != "Running" {
                    break;
                }
            }
            backend.get_last_build_status().await
        }
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
        "build-status" => backend.get_last_build_status().await,
        "build-log" => backend.get_last_build_log().await,
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
        let cmd = args.get(1).map_or("help", String::as_str);
        let rest = args.get(2..).unwrap_or(&[]).to_vec();
        let backend = ElectronCdpBackend::new();
        match dispatch_electron_cli(&backend, cmd, &rest).await {
            Ok(out) => {
                if let Err(e) = electron_cli_write(&out) {
                    use std::io::Write as _;
                    drop(writeln!(std::io::stderr().lock(), "Output error: {e}"));
                }
            }
            Err(e) => {
                use std::io::Write as _;
                drop(writeln!(std::io::stderr().lock(), "Error: {e}"));
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
