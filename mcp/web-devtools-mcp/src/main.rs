//! # poly-web-devtools-mcp
//!
//! MCP server for the **web** devtools backend.
//!
//! Builds the Poly web WASM bundle with `dx build --platform web` (one-shot,
//! synchronous, immediate exit-code feedback), serves the output directory with
//! Python's built-in static HTTP server, and launches Chromium with the Chrome
//! DevTools Protocol (CDP) for inspection and interaction.
//!
//! ## Build model
//!
//! - `launch_app` runs `dx build --platform web`, waits for the process to
//!   exit (success or failure is **immediately** visible), starts a static file
//!   server (`python3 -m http.server 3000`) serving
//!   `target/dx/poly-web/debug/web/public/`, then launches Chromium.
//! - `rebuild_app` re-runs `dx build --platform web` and reloads Chrome. No
//!   server restart needed — the build overwrites files in the same directory.
//! - No file watchers, no hotpatch, no background dx serve process.
//!
//! If Chrome crashes or exits while the MCP server is still running, it is
//! automatically restarted by a watchdog task.
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
    BuildDiagnostics, BuildLifecycleState, DevtoolsBackend, NavigateParams, RollingBuildLog,
    ScreenshotParams, ScreenshotResult,
};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;

/// Port for the static file server that serves the built WASM bundle.
/// NOTE: Port 8080 is used by `dx serve --platform desktop` for its hot-reload
/// asset server, so we use 3000 here to avoid the conflict when both MCPs run
/// simultaneously.
const WEB_SERVER_PORT: u16 = 3000;
/// Chrome DevTools Protocol debugging port.
const CDP_PORT: u16 = 9222;
const BUILD_LOG_EXCERPT_LINES: usize = 60;
const CDP_SEND_TIMEOUT_SECS: u64 = 5;
const CDP_RESPONSE_TIMEOUT_SECS: u64 = 15;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// CLI configuration parsed at startup.
struct CliConfig {
    /// Run Chrome in headless mode (no visible window).
    headless: bool,
    /// If set, run as CLI with this command instead of MCP server mode.
    cli_command: Option<Vec<String>>,
}

/// Internal build record tracked by the web backend.
#[derive(Debug, Clone)]
struct WebBuildRecord {
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
    lines.get(start..).unwrap_or(&[]).join("\n")
}

/// Known CLI subcommand names.
const WEB_CLI_COMMANDS: &[&str] = &[
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

impl CliConfig {
    fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let headless = args.iter().any(|a| a == "--headless");
        // Check for CLI command: first non-flag arg that matches a known command.
        let cli_command = args
            .iter()
            .skip(1)
            .find(|a| !a.starts_with("--") && WEB_CLI_COMMANDS.contains(&a.as_str()))
            .map(|_| {
                // Return all args from the command position onward.
                let pos = args
                    .iter()
                    .position(|a| WEB_CLI_COMMANDS.contains(&a.as_str()))
                    .unwrap_or(1);
                args.get(pos..).unwrap_or(&[]).to_vec()
            });
        Self {
            headless,
            cli_command,
        }
    }
}

// ─── Chrome CDP Backend ───────────────────────────────────────────────────────

/// Chrome CDP Backend — builds the WASM bundle via `dx build`, serves it with
/// a static file server, and drives Chrome via CDP.
#[derive(Clone)]
struct ChromeCdpBackend {
    ws: Arc<Mutex<Option<WsStream>>>,
    msg_id: Arc<AtomicI64>,
    client: reqwest::Client,
    /// Whether to run Chrome headless (no window).
    headless: bool,
    /// Set to `true` when the MCP server is shutting down — suppresses restart.
    shutting_down: Arc<AtomicBool>,
    /// Handle to the Chrome watchdog task (if running).
    watchdog_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// PID of the static file server (python3 -m http.server) process.
    /// Used for cleanup and shown in `get_generation` as `dx_serve_pid`.
    dx_serve_pid: Arc<Mutex<Option<u32>>>,
    /// Workspace path — set during `launch_app`, used by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
    /// Generation counter — increments on each successful `connect()` call.
    generation: Arc<AtomicU64>,
    /// Rolling combined stdout/stderr log from dx build and app output.
    build_log: Arc<Mutex<RollingBuildLog>>,
    /// Structured diagnostics for the last build attempt.
    last_build: Arc<Mutex<Option<WebBuildRecord>>>,
    /// Background build task handle — `None` when idle, `Some` while a build
    /// is in progress. Used to prevent concurrent builds.
    build_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

/// Pipe an async reader (stdout/stderr) line-by-line into a [`RollingBuildLog`].
///
/// Spawns a detached tokio task that reads until EOF, tagging each line with
/// `[stream_name]` so stdout and stderr are distinguishable in the log.
fn spawn_log_reader<R>(
    reader: R,
    stream_name: &'static str,
    buffer: Arc<Mutex<RollingBuildLog>>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt as _;
        let mut lines = tokio::io::BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    buffer.lock().await.push_line(format!("[{stream_name}] {line}"));
                }
                Ok(None) => break,
                Err(err) => {
                    buffer
                        .lock()
                        .await
                        .push_line(format!("[{stream_name}] <read error: {err}>"));
                    break;
                }
            }
        }
    });
}

impl ChromeCdpBackend {
    fn new(headless: bool) -> Self {
        Self {
            ws: Arc::new(Mutex::new(None)),
            msg_id: Arc::new(AtomicI64::new(1)),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            headless,
            shutting_down: Arc::new(AtomicBool::new(false)),
            watchdog_handle: Arc::new(Mutex::new(None)),
            dx_serve_pid: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
            build_log: Arc::new(Mutex::new(RollingBuildLog::default())),
            last_build: Arc::new(Mutex::new(None)),
            build_task: Arc::new(Mutex::new(None)),
        }
    }

    /// Wait for the dx serve web server to respond, with early crash detection.
    ///
    /// Checks the build log on every poll for fatal crash patterns (exit 127,
    /// undefined symbol, missing `.so`, etc.). Aborts immediately instead of
    /// blocking for 120 s when the build already failed.
    async fn wait_for_web_server(&self, max_seconds: u64) -> anyhow::Result<()> {
        let polls = max_seconds * 2;
        for _ in 0..polls {
            let ok = self
                .client
                .get(format!("http://127.0.0.1:{WEB_SERVER_PORT}"))
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await
                .map(|resp| resp.status().is_success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            // Early abort: check if the build/app crashed.
            if let Some(crash_line) = self.build_log.lock().await.check_for_app_crash() {
                anyhow::bail!(
                    "Build/app crashed before web server came up.\n\
                     Matched log line: {crash_line}\n\n\
                     Fix: run `cd apps/web && dx build --platform web` \
                     to rebuild everything in sync, then retry launch_app."
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::bail!(
            "Static file server did not become reachable on port {WEB_SERVER_PORT} within {max_seconds}s"
        )
    }

    async fn start_build_record(
        &self,
        trigger: &str,
        mode: &str,
        working_directory: &str,
        command_line: &str,
        summary: &str,
    ) -> u64 {
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
            process_id_before: *self.dx_serve_pid.lock().await,
            process_id_after: None,
            log_line_count: 0,
            log_excerpt: String::new(),
        };
        *self.last_build.lock().await = Some(WebBuildRecord {
            diagnostics,
            log_start_seq: start_seq,
        });
        start_seq
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
        let pid_after = *self.dx_serve_pid.lock().await;
        let generation_after = self.generation.load(Ordering::Relaxed);

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

        let now = unix_now_ms();
        let generation_after = self.generation.load(Ordering::Relaxed);
        let pid_after = *self.dx_serve_pid.lock().await;
        let lines = self
            .build_log
            .lock()
            .await
            .lines_since(record_snapshot.log_start_seq);

        if let Some(record) = self.last_build.lock().await.as_mut() {
            record.diagnostics.state = BuildLifecycleState::Succeeded;
            record.diagnostics.summary =
                "Browser/CDP reconnected successfully after the most recent build.".to_string();
            record.diagnostics.verification =
                "Verified by a successful connect_cdp after the rebuild/launch workflow."
                    .to_string();
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
            process_id_before: *self.dx_serve_pid.lock().await,
            process_id_after: *self.dx_serve_pid.lock().await,
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
        ];

        if self.headless {
            // Headless mode: no visible window
            args.insert(0, "--headless=new".to_string());
            args.push(format!("http://127.0.0.1:{WEB_SERVER_PORT}"));
        } else {
            // Visible mode: ensure window is created and visible
            args.push(format!("http://127.0.0.1:{WEB_SERVER_PORT}"));
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

        if tokio::time::timeout(
            std::time::Duration::from_secs(CDP_SEND_TIMEOUT_SECS),
            ws.send(Message::Text(serde_json::to_string(&msg)?.into())),
        )
        .await
        .is_err()
        {
            drop(ws_guard);
            *self.ws.lock().await = None;
            anyhow::bail!(
                "CDP send timeout for method '{method}' after {CDP_SEND_TIMEOUT_SECS}s. The renderer may be hung; call connect_cdp to retry."
            );
        }

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(CDP_RESPONSE_TIMEOUT_SECS),
            async {
                // Read messages until we get our response (matching id)
                loop {
                    let Some(Ok(raw)) = ws.next().await else {
                        anyhow::bail!(
                            "CDP WebSocket closed unexpectedly — Chrome may have crashed. Call connect_cdp to reconnect."
                        );
                    };

                    let text = match raw {
                        Message::Text(t) => t.to_string(),
                        Message::Close(_) => {
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
            },
        )
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
                    "CDP response timeout for method '{method}' after {CDP_RESPONSE_TIMEOUT_SECS}s. The page may be frozen; call connect_cdp to reconnect once it recovers."
                );
            }
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

    /// Background task body for `launch_app`.
    ///
    /// Spawns `dx serve --platform web --port {WEB_SERVER_PORT}` as a long-running
    /// background process. `dx serve` compiles the WASM bundle and serves it on
    /// port {WEB_SERVER_PORT} — no python3 static server needed. We poll for HTTP
    /// readiness then launch Chrome via the watchdog.
    async fn bg_build_and_launch(&self, app_dir: &str) {
        // ── Kill any existing dx serve process ────────────────────────────────
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }

        // ── Spawn dx serve as a long-running background process ───────────────
        let mut child = match tokio::process::Command::new("dx")
            .args([
                "serve",
                "--platform",
                "web",
                "--port",
                &WEB_SERVER_PORT.to_string(),
            ])
            .current_dir(app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "Failed to spawn dx serve.",
                    format!("Could not spawn `dx serve --platform web`: {e}"),
                    None,
                )
                .await;
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(stdout, "dx-stdout", self.build_log.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(stderr, "dx-stderr", self.build_log.clone());
        }

        if let Some(pid) = child.id() {
            *self.dx_serve_pid.lock().await = Some(pid);
        }
        let pid_ref = self.dx_serve_pid.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *pid_ref.lock().await = None;
        });

        Self::increment_rebuild_counter();

        // ── Wait for dx serve's HTTP server ───────────────────────────────────
        // Allow up to 120 s: dx serve must compile the WASM bundle before serving.
        if let Err(e) = self.wait_for_web_server(120).await {
            self.finish_build_record(
                BuildLifecycleState::Failed,
                "dx serve started but HTTP server did not respond within 120 s.",
                format!(
                    "Port {WEB_SERVER_PORT} not reachable: {e}. \
                     Check get_last_build_log for dx serve / cargo errors."
                ),
                None,
            )
            .await;
            return;
        }

        self.finish_build_record(
            BuildLifecycleState::Succeeded,
            "WASM compiled \u{2713} \u{2014} dx serve is up. Launching Chrome.",
            format!(
                "dx serve answered on port {WEB_SERVER_PORT}. Launching Chrome with CDP."
            ),
            None,
        )
        .await;

        // ── Chrome ────────────────────────────────────────────────────────────
        self.shutting_down.store(false, Ordering::Relaxed);
        if let Err(e) = self.spawn_chrome_with_watchdog().await
            && let Some(rec) = self.last_build.lock().await.as_mut()
        {
            rec.diagnostics.summary = format!(
                "WASM compiled and dx serve started, but Chrome failed to launch: {e}"
            );
        }
    }

    /// Background task body for `rebuild_app`.
    ///
    /// Kills the running `dx serve`, spawns a fresh one to recompile and re-serve
    /// the WASM bundle, then reloads the Chrome page via CDP. On success the agent
    /// only needs to call `connect_cdp` — the page reload is done automatically.
    async fn bg_rebuild(&self, app_dir: &str) {
        // ── Kill current dx serve ─────────────────────────────────────────────
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "dx.*serve.*web"])
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        // ── Spawn fresh dx serve ──────────────────────────────────────────────
        let mut child = match tokio::process::Command::new("dx")
            .args([
                "serve",
                "--platform",
                "web",
                "--port",
                &WEB_SERVER_PORT.to_string(),
            ])
            .current_dir(app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "Failed to spawn dx serve for rebuild.",
                    format!("Could not spawn `dx serve --platform web`: {e}"),
                    None,
                )
                .await;
                return;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(stdout, "dx-stdout", self.build_log.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(stderr, "dx-stderr", self.build_log.clone());
        }

        if let Some(pid) = child.id() {
            *self.dx_serve_pid.lock().await = Some(pid);
        }
        let pid_ref = self.dx_serve_pid.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *pid_ref.lock().await = None;
        });

        // ── Wait for dx serve to recompile and come up ────────────────────────
        if let Err(e) = self.wait_for_web_server(120).await {
            self.finish_build_record(
                BuildLifecycleState::Failed,
                "Fresh dx serve did not become ready within 120 s.",
                format!("Port {WEB_SERVER_PORT} not reachable after restart: {e}."),
                None,
            )
            .await;
            return;
        }

        // ── CDP page reload ───────────────────────────────────────────────────
        // Chrome is still alive (we didn't kill it). Reload so it picks up the
        // new WASM bundle. The WS connection drops during reload; clear it so
        // the next connect_cdp reconnects cleanly.
        let reload_ok = self
            .cdp_send("Page.reload", json!({ "ignoreCache": true }))
            .await
            .is_ok();
        *self.ws.lock().await = None;

        self.finish_build_record(
            BuildLifecycleState::Succeeded,
            "WASM rebuilt \u{2713} \u{2014} dx serve restarted, Chrome reloaded. Call connect_cdp.",
            format!("dx serve up on port {WEB_SERVER_PORT}. CDP reload sent: {reload_ok}."),
            None,
        )
        .await;
    }
}

#[async_trait]
impl DevtoolsBackend for ChromeCdpBackend {
    fn name(&self) -> &str {
        "web-cdp"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        let app_dir = format!("{workspace}/apps/web");

        *self.workspace.lock().await = Some(workspace.to_string());

        // ── Guard: refuse concurrent builds ──────────────────────────────────
        {
            let guard = self.build_task.lock().await;
            if let Some(handle) = guard.as_ref()
                && !handle.is_finished() {
                    return Ok(
                        "A build is already in progress.\n\
                         Poll get_last_build_status \u{2014} state will change Running \u{2192} Succeeded/Failed."
                            .to_string(),
                    );
                }
        }

        // ── Step 0: Kill Chrome / static server synchronously (fast) ─────────
        self.shutting_down.store(true, Ordering::Relaxed);
        if let Some(handle) = self.watchdog_handle.lock().await.take() {
            handle.abort();
        }
        *self.ws.lock().await = None;
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", &format!("remote-debugging-port={CDP_PORT}")])
            .status()
            .await;
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "dx.*serve.*web"])
            .status()
            .await;
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        // ── Hand off the slow work (build + server + Chrome) to a background task
        //
        // `dx build --platform web` takes 30-90 s.  Running it synchronously would
        // block this MCP response long enough for VS Code to drop the connection.
        // We set state = Running, return immediately (\u2248 50 ms), and the agent
        // polls get_last_build_status until Running \u2192 Succeeded/Failed.
        let _ = self
            .start_build_record(
                "launch_app",
                "dx serve --platform web",
                &app_dir,
                "dx serve --platform web",
                "dx serve started in background (state: Running). \
                 Poll get_last_build_status for progress.",
            )
            .await;

        let ctx = self.clone(); // cheap Arc clone — shares all state
        let handle = tokio::spawn(async move {
            ctx.bg_build_and_launch(&app_dir).await;
        });
        *self.build_task.lock().await = Some(handle);

        Ok(
            "\u{1f527} Build started in background (state: Running).\n\
             \u{25b6} Poll `get_last_build_status` until state = Succeeded or Failed.\n\
             \u{1f4cb} On Succeeded: Chrome is already running \u{2014} call `connect_cdp`.\n\
             \u{274c} On Failed: call `get_last_build_log` for the exact compiler error."
                .to_string(),
        )
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        // Signal watchdog to stop restarting.
        self.shutting_down.store(true, Ordering::Relaxed);

        // Close CDP connection.
        *self.ws.lock().await = None;

        // Cancel watchdog task.
        if let Some(handle) = self.watchdog_handle.lock().await.take() {
            handle.abort();
        }

        // Kill Chrome.
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "remote-debugging-port=9222"])
            .status()
            .await;
        // Kill static file server by PID if we have it.
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }
        // Kill any stale dx serve from previous sessions.
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "dx serve"])
            .status()
            .await;

        Ok(
            "Killed Chrome and static file server. Watchdog stopped. Call launch_app to restart."
                .to_string(),
        )
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
        self.note_successful_connect().await;

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

        // SIGKILL static file server by PID.
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
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

        // SIGKILL any stale dx serve from previous sessions (pattern fallback).
        let _ = tokio::process::Command::new("bash")
            .args(["-c", "pkill -9 -f 'dx.*serve.*web' 2>/dev/null || true"])
            .status()
            .await;

        Ok("Hard-killed Chrome and static file server (SIGKILL). \
             Watchdog stopped. Call launch_app to rebuild and restart."
            .to_string())
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
        let app_dir = format!("{workspace}/apps/web");

        // ── Guard: refuse concurrent builds ──────────────────────────────────
        {
            let guard = self.build_task.lock().await;
            if let Some(handle) = guard.as_ref()
                && !handle.is_finished() {
                    return Ok(
                        "A build is already in progress.\n\
                         Poll get_last_build_status \u{2014} state will change Running \u{2192} Succeeded/Failed."
                            .to_string(),
                    );
                }
        }

        // Increment counter before build so build_id advances on every rebuild_app call.
        Self::increment_rebuild_counter();

        let _ = self
            .start_build_record(
                "rebuild_app",
                "dx serve --platform web",
                &app_dir,
                "dx serve --platform web",
                "dx serve restarting in background (state: Running). \
                 Poll get_last_build_status for progress.",
            )
            .await;

        // ── Spawn background rebuild task (returns immediately to avoid MCP timeout)
        //
        // `dx serve --platform web` takes 30-90 s to recompile.  We set state =
        // Running, return immediately, and the agent polls get_last_build_status
        // until Running \u{2192} Succeeded or Failed.  On success the background task
        // has already reloaded Chrome — the agent only needs connect_cdp.
        let ctx = self.clone(); // cheap Arc clone — shares all mutable state
        let handle = tokio::spawn(async move {
            ctx.bg_rebuild(&app_dir).await;
        });
        *self.build_task.lock().await = Some(handle);

        Ok(
            "\u{1f527} WASM rebuild started in background (state: Running).\n\
             \u{25b6} Poll `get_last_build_status` until state = Succeeded or Failed.\n\
             \u{1f4cb} On Succeeded: call `connect_cdp` (Chrome already reloaded).\n\
             \u{274c} On Failed: call `get_last_build_log` for the exact compiler error."
                .to_string(),
        )
    }

    async fn get_last_build_status(&self) -> anyhow::Result<String> {
        self.last_build_status_json().await
    }

    async fn get_last_build_log(&self) -> anyhow::Result<String> {
        self.last_build_log_text().await
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
                    **build_id**: increments on each rebuild_app / force_rebuild call (reads /tmp/poly-devtools-web-rebuild-counter).\n\
                    0 = no rebuild triggered yet this session. Mirrors the desktop MCP build_id semantics.\n\
                    **dx_serve_pid**: PID of the python3 static file server process (null if not started by this MCP).\n\n\
                    Decision table:\n\
                    - build_id increased, generation same → rebuild triggered, connect_cdp not yet called\n\
                    - build_id increased, generation increased → full rebuild+reconnect completed\n\
                    - dx_serve_pid changed → static file server was restarted\n\n\
                    If generation/build_id do not move the way you expect, immediately inspect get_last_build_status and get_last_build_log for the actual Dioxus output.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }),
            json!({
                "name": "force_rebuild",
                "description": "Force a complete WASM rebuild by running `dx build --platform web` directly.\n\n\
                    Equivalent to rebuild_app but invokable as a standalone extension tool.\n\
                    Runs `dx build --platform web` in apps/web/, which blocks until the build is done\n\
                    and gives an immediate exit-code pass/fail signal.\n\
                    The python3 static file server keeps running; Chrome picks up the new files on reload.\n\n\
                    After this tool returns:\n\
                    1. Call connect_cdp (the page reloaded automatically if Chrome was connected)\n\
                    2. Verify with get_generation that build_id incremented\n\
                    3. If anything looks wrong, inspect get_last_build_status and get_last_build_log",
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
            "force_rebuild" => {
                let workspace = self
                    .workspace
                    .lock()
                    .await
                    .clone()
                    .unwrap_or_else(|| "/home/laragana/workspcacemsg".to_string());
                // Delegate to rebuild_app, which now starts the build in the background
                // and returns immediately.  This prevents MCP client timeouts.
                Some(self.rebuild_app(&workspace).await)
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

// ─── CLI Mode ─────────────────────────────────────────────────────────────────
//
// PREFERRED: Use the CLI over MCP access wherever possible.
// CLI is faster, scriptable, and doesn't require a Copilot MCP session.
//
// Usage examples:
//   cargo run --bin poly-web-devtools-mcp -- status
//   cargo run --bin poly-web-devtools-mcp -- screenshot --save /tmp/shot.png
//   cargo run --bin poly-web-devtools-mcp -- snapshot
//   cargo run --bin poly-web-devtools-mcp -- eval "document.title"
//   cargo run --bin poly-web-devtools-mcp -- [--headless] launch /path/to/ws

/// Write a line to stdout without `println!`.
fn web_cli_write(text: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    writeln!(std::io::stdout().lock(), "{text}")?;
    Ok(())
}

/// Web CLI help text.
fn web_cli_help() -> &'static str {
    "poly-web-devtools-mcp — CLI mode (PREFERRED over MCP)

COMMANDS:
  status                    Check CDP connection
  launch [workspace]        Build WASM (dx build --platform web), start static server + Chrome
  rebuild [workspace]       Rebuild WASM + reload Chrome (non-blocking, polls until done)
  kill                      Stop Chrome and static file server
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

FLAGS (before command):
  --headless                Run Chrome headless

MCP mode (default, no subcommand):
  cargo run --bin poly-web-devtools-mcp [--headless]
"
}

/// Handle `screenshot` CLI command for web backend.
async fn web_cli_screenshot(backend: &ChromeCdpBackend, args: &[String]) -> anyhow::Result<String> {
    use base64::Engine as _;
    let save_path = args
        .iter()
        .position(|a| a == "--save")
        .and_then(|p| args.get(p + 1))
        .map(String::as_str);
    let params = poly_devtools_protocol::backend::ScreenshotParams::default();
    let result = backend.take_screenshot(&params).await?;
    if let Some(path) = save_path {
        std::fs::write(path, &result.image_bytes)?;
        Ok(format!("Screenshot saved to {path}"))
    } else {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&result.image_bytes);
        Ok(format!("data:{};base64,{b64}", result.mime_type))
    }
}

/// Detect workspace root (POLY_WORKSPACE env var or cwd).
fn web_detect_workspace() -> String {
    if let Ok(ws) = std::env::var("POLY_WORKSPACE") {
        return ws;
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// Dispatch a CLI command for the web backend.
async fn dispatch_web_cli(
    backend: &ChromeCdpBackend,
    cmd: &str,
    args: &[String],
) -> anyhow::Result<String> {
    use poly_devtools_protocol::backend::NavigateParams;
    match cmd {
        "status" | "connect" => backend.connect().await,
        "launch" => {
            let ws = args
                .first()
                .map(String::as_str)
                .map(str::to_string)
                .unwrap_or_else(web_detect_workspace);
            // launch_app is non-blocking — poll until background build finishes,
            // otherwise the process exits and kills the tokio task.
            let initial_msg = backend.launch_app(&ws).await?;
            web_cli_write(&initial_msg)?;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let status_str = backend.get_last_build_status().await.unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&status_str).unwrap_or(serde_json::Value::Null);
                let state = parsed
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                web_cli_write(&format!("[build] state = {state}"))?;
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
                .map(String::as_str)
                .map(str::to_string)
                .unwrap_or_else(web_detect_workspace);
            // rebuild_app is non-blocking — poll until background rebuild finishes.
            let initial_msg = backend.rebuild_app(&ws).await?;
            web_cli_write(&initial_msg)?;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let status_str = backend.get_last_build_status().await.unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&status_str).unwrap_or(serde_json::Value::Null);
                let state = parsed
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                web_cli_write(&format!("[rebuild] state = {state}"))?;
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
        "generation" => {
            // For web CLI: connect to CDP and report status with generation info.
            backend.connect().await
        }
        "build-status" => backend.get_last_build_status().await,
        "build-log" => backend.get_last_build_log().await,
        "screenshot" => web_cli_screenshot(backend, args).await,
        _ => Ok(web_cli_help().to_string()),
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

    let config = CliConfig::parse();

    if let Some(cli_args) = config.cli_command {
        let backend = ChromeCdpBackend::new(config.headless);
        let cmd = cli_args.first().map(String::as_str).unwrap_or("help");
        let rest = cli_args.get(1..).unwrap_or(&[]).to_vec();
        match dispatch_web_cli(&backend, cmd, &rest).await {
            Ok(out) => {
                if let Err(e) = web_cli_write(&out) {
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
        return;
    }

    if config.headless {
        tracing::info!("🔍 Starting poly-web-devtools-mcp in HEADLESS mode (no visible window)");
        tracing::info!("   To see a visible Chromium window: remove the --headless flag");
    } else {
        tracing::info!("🖥️  Starting poly-web-devtools-mcp with VISIBLE Chromium window");
        tracing::info!("   → A Chromium window will launch automatically when you call launch_app");
        tracing::info!("   → Watch it to see exactly what the MCP is doing!");
    }

    let backend = ChromeCdpBackend::new(config.headless);
    run_mcp_loop(&backend, "poly-web").await;
}
