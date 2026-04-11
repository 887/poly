//! # poly-desktop-devtools-mcp
//!
//! MCP server for the **desktop** devtools backend.
//!
//! Builds and runs the desktop-devtools app via `dx serve --hotpatch`.
//! `--hotpatch` is required: it configures `LD_LIBRARY_PATH` so the launched
//! binary can find `libpoly_plugin_host.so` and other dynamic libs. Without
//! it the binary launches but exits immediately with code 127.
//!
//! ## Build model
//!
//! - `launch_app` spawns `dx serve --hotpatch` as a long-running background
//!   process (non-blocking, returns ~600 ms). Agent polls `get_last_build_status`
//!   until state = Succeeded/Failed, then calls `connect_cdp`.
//! - `rebuild_app` kills the running `dx serve` and re-runs `launch_app`.
//! - `dx serve --hotpatch` matches the working VS Code "Desktop Wry (Linux)"
//!   launch.json configuration exactly.
//!
//! ## Usage
//! ```bash
//! cargo run --bin poly-desktop-devtools-mcp
//! ```
//! Or via `.vscode/mcp.json` for GitHub Copilot integration.

use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use poly_devtools_protocol::backend::{
    BuildDiagnostics, BuildLifecycleState, DevtoolsBackend, RollingBuildLog, ScreenshotParams,
    ScreenshotResult,
};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

const BASE: &str = "http://127.0.0.1:9223";
const BUILD_LOG_EXCERPT_LINES: usize = 60;
const APP_PROCESS_PATTERN: &str = "poly-desktop-devtools($|[^-])";
const WEB_SHELL_PATTERN: &str = "poly-desktop-web($|[^-])";

/// Web-shell dev server port (dx serve --platform web --port 3002).
const WEB_SERVE_PORT: u16 = 3002;

/// Check if legacy hotpatch mode is enabled.
fn is_legacy_mode() -> bool {
    std::env::var("POLY_DESKTOP_LEGACY")
        .map(|v| v == "1")
        .unwrap_or(false)
}

// ─── HTTP helpers ─────────────────────────────────────────────────────────────

async fn http_eval(client: &reqwest::Client, js: &str) -> anyhow::Result<String> {
    let resp = client
        .post(format!("{BASE}/eval"))
        .body(js.to_string())
        .send()
        .await?;
    let body = resp.text().await?;
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::String(body));
    if let Some(r) = v.get("result").and_then(|r| r.as_str()) {
        return Ok(r.to_string());
    }
    if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
        return Err(anyhow::anyhow!("{e}"));
    }
    Ok(v.to_string())
}

async fn http_get(client: &reqwest::Client, path: &str) -> anyhow::Result<Vec<u8>> {
    let resp = client.get(format!("{BASE}{path}")).send().await?;
    Ok(resp.bytes().await?.to_vec())
}

// ─── App Process State ───────────────────────────────────────────────────────

/// Handle to the running desktop-devtools binary.
///
/// Tracks the process ID for hard-kill via SIGKILL.
struct AppProcess {
    /// OS process ID — used for hard-kill via SIGKILL.
    pid: u32,
}

/// Snapshot of the desktop app's `/generation` endpoint.
#[derive(Debug, Clone, serde::Deserialize)]
struct DesktopGenerationInfo {
    generation: u64,
    build_id: u64,
    pid: u32,
}

/// Internal build record tracked by this MCP backend.
#[derive(Debug, Clone)]
struct DesktopBuildRecord {
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

fn spawn_log_reader<R>(reader: R, stream_name: &'static str, buffer: Arc<Mutex<RollingBuildLog>>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    buffer
                        .lock()
                        .await
                        .push_line(format!("[{stream_name}] {line}"));
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

// ─── Desktop HTTP Backend ─────────────────────────────────────────────────────

/// Desktop devtools backend.
///
/// **New default mode** (`POLY_DESKTOP_LEGACY` not set):
///   - Starts `dx serve --platform web --port 3002` in `apps/desktop`
///   - Launches `poly-desktop-web` (thin Wry shell) which loads from port 3002
///   - Rebuild: kills dx serve, restarts it, calls `/reload` on the eval bridge
///
/// **Legacy hotpatch mode** (`POLY_DESKTOP_LEGACY=1`):
///   - Uses the old `dx serve --hotpatch` in `apps/desktop-devtools`
///   - The binary embeds the app + eval bridge
///
/// `Clone` is derived so that a cheap Arc-clone can be given to background build
/// tasks without sharing ownership of the entire backend.
#[derive(Clone)]
struct DesktopHttpBackend {
    client: reqwest::Client,
    /// Handle to the running desktop-devtools (legacy) or dx serve (web) process.
    app_process: Arc<Mutex<Option<AppProcess>>>,
    /// PID of the dx serve process (web shell mode only).
    dx_serve_pid: Arc<Mutex<Option<u32>>>,
    /// PID of the poly-desktop-web shell process (web shell mode only).
    shell_pid: Arc<Mutex<Option<u32>>>,
    /// Workspace path — set during `launch_app`, used by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
    /// Rolling combined stdout/stderr log from Dioxus build and app output.
    build_log: Arc<Mutex<RollingBuildLog>>,
    /// Structured diagnostics for the last build attempt.
    last_build: Arc<Mutex<Option<DesktopBuildRecord>>>,
    /// Background build task handle — `None` when idle, `Some` while a build
    /// is in progress. Used to prevent concurrent builds.
    build_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl DesktopHttpBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            app_process: Arc::new(Mutex::new(None)),
            dx_serve_pid: Arc::new(Mutex::new(None)),
            shell_pid: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
            build_log: Arc::new(Mutex::new(RollingBuildLog::default())),
            last_build: Arc::new(Mutex::new(None)),
            build_task: Arc::new(Mutex::new(None)),
        }
    }

    fn read_rebuild_counter() -> u64 {
        let path = std::path::Path::new("/tmp/poly-devtools-rebuild-counter");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0)
    }

    async fn fetch_generation_info(&self) -> Option<DesktopGenerationInfo> {
        let resp = self
            .client
            .get(format!("{BASE}/generation"))
            .send()
            .await
            .ok()?;
        let text = resp.text().await.ok()?;
        serde_json::from_str::<DesktopGenerationInfo>(&text).ok()
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

        let generation_before = self.fetch_generation_info().await;
        let diagnostics = BuildDiagnostics {
            backend: self.name().to_string(),
            trigger: trigger.to_string(),
            mode: mode.to_string(),
            working_directory: working_directory.to_string(),
            command_line: command_line.to_string(),
            state: BuildLifecycleState::Running,
            summary: summary.to_string(),
            verification: "Build command started; waiting for Dioxus output / readiness signal."
                .to_string(),
            exit_code: None,
            started_at_unix_ms: Some(unix_now_ms()),
            finished_at_unix_ms: None,
            duration_ms: None,
            build_id_before: Some(Self::read_rebuild_counter()),
            build_id_after: None,
            generation_before: generation_before.as_ref().map(|g| g.generation),
            generation_after: None,
            process_id_before: generation_before.as_ref().map(|g| g.pid),
            process_id_after: None,
            log_line_count: 0,
            log_excerpt: String::new(),
        };

        *self.last_build.lock().await = Some(DesktopBuildRecord {
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
        let generation_after = self.fetch_generation_info().await;
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
            record.diagnostics.build_id_after = Some(
                generation_after
                    .as_ref()
                    .map_or_else(Self::read_rebuild_counter, |g| g.build_id),
            );
            record.diagnostics.generation_after = generation_after.as_ref().map(|g| g.generation);
            record.diagnostics.process_id_after = generation_after.as_ref().map(|g| g.pid);
            record.diagnostics.log_line_count = lines.len();
            record.diagnostics.log_excerpt = log_excerpt;
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
            mode: "dx serve --hotpatch".to_string(),
            working_directory: String::new(),
            command_line: String::new(),
            state: BuildLifecycleState::NotStarted,
            summary: "No Dioxus build has been recorded yet in this MCP session.".to_string(),
            verification: "Call launch_app, rebuild_app, or force_rebuild first.".to_string(),
            exit_code: None,
            started_at_unix_ms: None,
            finished_at_unix_ms: None,
            duration_ms: None,
            build_id_before: Some(Self::read_rebuild_counter()),
            build_id_after: Some(Self::read_rebuild_counter()),
            generation_before: None,
            generation_after: None,
            process_id_before: None,
            process_id_after: None,
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

    /// Hide the transient Dioxus rebuild toast when the real app root is already visible.
    async fn suppress_rebuild_toast_if_app_ready(&self) {
        let _ignored = http_eval(
            &self.client,
            r#"return (function(){
                var appRoot = document.querySelector('#main');
                var toast = document.querySelector('#__dx-toast');
                if (appRoot && toast) {
                    toast.style.display = 'none';
                    toast.setAttribute('data-poly-hidden-rebuild-toast', 'true');
                    return JSON.stringify({ hidden: true, reason: 'app-root-present' });
                }
                return JSON.stringify({ hidden: false, appRoot: !!appRoot, toast: !!toast });
            })()"#,
        )
        .await;
    }

    /// Check if the eval bridge is currently responding.
    async fn is_bridge_alive(&self) -> bool {
        self.client
            .get(format!("{BASE}/status"))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Poll the eval bridge until it responds, a crash is detected, or timeout.
    ///
    /// On each iteration the build log is scanned for fatal crash patterns
    /// (exit 127, undefined symbol, missing `.so`, etc.). If the app binary
    /// crashed at startup the wait aborts immediately instead of blocking for
    /// the full `max_seconds`.
    async fn wait_for_bridge(&self, max_seconds: u64) -> anyhow::Result<()> {
        let polls = max_seconds * 2; // poll every 500 ms
        for _ in 0..polls {
            if self.is_bridge_alive().await {
                return Ok(());
            }
            // Early abort: check if the app binary crashed before the bridge came up.
            if let Some(crash_line) = self.build_log.lock().await.check_for_app_crash() {
                anyhow::bail!(
                    "App binary crashed before eval bridge came up.\n\
                     Matched log line: {crash_line}\n\n\
                     This usually means the binary has an undefined symbol or missing .so.\n\
                     Fix: run `cd apps/desktop-devtools && dx build --platform desktop` \
                     to rebuild everything in sync, then retry launch_app."
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::bail!("Eval bridge at {BASE} did not become ready within {max_seconds}s")
    }

    /// Atomically increment `/tmp/poly-devtools-rebuild-counter`.
    async fn increment_rebuild_counter() -> anyhow::Result<()> {
        let path = std::path::Path::new("/tmp/poly-devtools-rebuild-counter");
        let current: u64 = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        std::fs::write(path, (current + 1).to_string())?;
        Ok(())
    }

    /// Poll `http://127.0.0.1:<port>/` until it returns any HTTP response.
    async fn wait_for_port(&self, port: u16, max_seconds: u64) -> anyhow::Result<()> {
        let url = format!("http://127.0.0.1:{port}/");
        let polls = max_seconds * 2; // 500 ms intervals
        for _ in 0..polls {
            let ok = self
                .client
                .get(&url)
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await
                .map(|r| r.status().as_u16() < 500)
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            // Early abort: check if dx serve crashed.
            if let Some(crash_line) = self.build_log.lock().await.check_for_app_crash() {
                anyhow::bail!("dx serve crashed: {crash_line}");
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::bail!(
            "dx serve did not become ready on port {port} within {max_seconds}s. \
             Call get_last_build_log for errors."
        )
    }

    /// Kill the tracked dx serve process (web shell mode).
    async fn kill_dx_serve(&self) {
        if let Some(pid) = self.dx_serve_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status()
                .await;
        }
        let _ = tokio::process::Command::new("pkill")
            .args([
                "-f",
                &format!("dx.*serve.*--port.*{WEB_SERVE_PORT}"),
            ])
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }

    /// Kill the poly-desktop-web shell process.
    async fn kill_web_shell(&self) {
        if let Some(pid) = self.shell_pid.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status()
                .await;
        }
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", WEB_SHELL_PATTERN])
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    /// Background task for the new web-shell launch mode.
    ///
    /// 1. Start `dx serve --platform web --port 3002` in `apps/desktop`
    /// 2. Poll port 3002 until ready (max 120s)
    /// 3. Launch `poly-desktop-web` (thin Wry shell)
    /// 4. Wait for eval bridge on port 9223 (max 30s)
    async fn bg_serve_and_launch_web_shell(&self, app_dir: &str, workspace: &str) {
        tracing::info!(
            "[bg] dx serve --platform web --port {WEB_SERVE_PORT} --fullstack  in {app_dir}"
        );

        // ── Spawn dx serve (fullstack) ────────────────────────────────────────
        //
        // apps/desktop is a Dioxus fullstack app: its server half merges
        // `poly_host::router(state)` into the Dioxus router, so `/host/*` is
        // served on the SAME port as the WASM bundle. The Wry thin shell
        // (`poly-desktop-web`, launched below) is a pure Chromium-like webview
        // and reaches the host bridge on port WEB_SERVE_PORT — it no longer
        // runs its own listener on 9333. The `@server --platform server`
        // split is REQUIRED, otherwise dx builds the server for
        // wasm32-unknown-unknown and fails.
        let mut serve_child = match tokio::process::Command::new("dx")
            .args([
                "serve",
                "--platform",
                "web",
                "--port",
                &WEB_SERVE_PORT.to_string(),
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
            .current_dir(app_dir)
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
                    format!("Failed to spawn `dx serve --platform web`: {e}"),
                    "dx serve could not be spawned. Is dx installed and on PATH?",
                    None,
                )
                .await;
                return;
            }
        };

        if let Some(stdout) = serve_child.stdout.take() {
            spawn_log_reader(stdout, "dx-stdout", self.build_log.clone());
        }
        if let Some(stderr) = serve_child.stderr.take() {
            spawn_log_reader(stderr, "dx-stderr", self.build_log.clone());
        }

        let serve_pid = serve_child.id();
        *self.dx_serve_pid.lock().await = serve_pid;

        // Auto-clear when dx serve exits.
        let pid_ref = self.dx_serve_pid.clone();
        tokio::spawn(async move {
            let _ = serve_child.wait().await;
            *pid_ref.lock().await = None;
            tracing::info!("[bg] dx serve exited");
        });

        let _ = Self::increment_rebuild_counter().await;

        // ── Wait for dx serve port ────────────────────────────────────────────
        match self.wait_for_port(WEB_SERVE_PORT, 120).await {
            Ok(()) => tracing::info!("[bg] dx serve ready on port {WEB_SERVE_PORT}"),
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve --platform web did not become ready.",
                    format!("{e}"),
                    None,
                )
                .await;
                return;
            }
        }

        // ── Launch poly-desktop-web shell ─────────────────────────────────────
        let shell_bin = format!("{workspace}/target/debug/poly-desktop-web");
        let mut shell_cmd = if std::path::Path::new(&shell_bin).exists() {
            tracing::info!("[bg] Using pre-built shell binary: {shell_bin}");
            tokio::process::Command::new(&shell_bin)
        } else {
            tracing::info!("[bg] Running shell via cargo run --bin poly-desktop-web");
            let mut c = tokio::process::Command::new("cargo");
            c.args(["run", "--bin", "poly-desktop-web"]);
            c.current_dir(workspace);
            c
        };

        shell_cmd
            .env("POLY_DEV_URL", format!("http://127.0.0.1:{WEB_SERVE_PORT}"))
            // webkit2gtk's DMA-BUF renderer path trips a "Protocol error (71)
            // dispatching to Wayland display" on some compositor versions
            // and kills the window during its first paint. Disabling dmabuf
            // keeps the GTK Wayland backend but forces webkit to use the
            // safer shared-memory render path.
            .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
            .env_remove("ELECTRON_RUN_AS_NODE")
            .env_remove("ELECTRON_NO_ATTACH_CONSOLE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());

        let mut shell_child = match shell_cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[bg] Failed to spawn poly-desktop-web: {e}");
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    format!("dx serve ready but shell launch failed: {e}"),
                    "poly-desktop-web could not be spawned.",
                    None,
                )
                .await;
                return;
            }
        };

        let shell_pid = shell_child.id();
        *self.shell_pid.lock().await = shell_pid;

        let shell_pid_ref = self.shell_pid.clone();
        tokio::spawn(async move {
            let status = shell_child.wait().await;
            let code = status.as_ref().ok().and_then(|s| s.code());
            *shell_pid_ref.lock().await = None;
            tracing::info!("poly-desktop-web exited (code {code:?})");
        });

        // ── Wait for eval bridge on port 9223 ─────────────────────────────────
        match self.wait_for_bridge(30).await {
            Ok(()) => {
                self.finish_build_record(
                    BuildLifecycleState::Succeeded,
                    format!(
                        "dx serve ready on port {WEB_SERVE_PORT}. \
                         poly-desktop-web shell launched (PID: {shell_pid:?}). \
                         Eval bridge ready on port 9223. Call connect_cdp."
                    ),
                    format!("Verified {BASE}/status. Shell stays alive across rebuilds."),
                    None,
                )
                .await;
                tracing::info!(
                    "[bg] Web shell launched (shell PID: {shell_pid:?}, dx serve PID: {serve_pid:?})"
                );
            }
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve and shell started but eval bridge did not respond within 30s.",
                    format!("{e}"),
                    None,
                )
                .await;
            }
        }
    }

    /// Background task for the new web-shell rebuild mode.
    ///
    /// 1. Kill dx serve (NOT the shell)
    /// 2. Restart dx serve
    /// 3. Poll port 3002
    /// 4. Call eval bridge POST /reload
    async fn bg_rebuild_web_shell(&self, app_dir: &str) {
        tracing::info!("[bg] rebuild: killing dx serve, restarting on port {WEB_SERVE_PORT}");

        self.kill_dx_serve().await;

        // Restart dx serve (fullstack — see bg_serve_and_launch_web_shell)
        let mut serve_child = match tokio::process::Command::new("dx")
            .args([
                "serve",
                "--platform",
                "web",
                "--port",
                &WEB_SERVE_PORT.to_string(),
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
            .current_dir(app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("[bg] Failed to restart dx serve: {e}");
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
            spawn_log_reader(stdout, "dx-stdout", self.build_log.clone());
        }
        if let Some(stderr) = serve_child.stderr.take() {
            spawn_log_reader(stderr, "dx-stderr", self.build_log.clone());
        }

        let serve_pid = serve_child.id();
        *self.dx_serve_pid.lock().await = serve_pid;

        let pid_ref = self.dx_serve_pid.clone();
        tokio::spawn(async move {
            let _ = serve_child.wait().await;
            *pid_ref.lock().await = None;
        });

        match self.wait_for_port(WEB_SERVE_PORT, 120).await {
            Ok(()) => tracing::info!("[bg] dx serve ready on port {WEB_SERVE_PORT} after rebuild"),
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve did not become ready after rebuild.",
                    format!("{e}"),
                    None,
                )
                .await;
                return;
            }
        }

        // Reload the shell page via the eval bridge
        let reload_ok = self
            .client
            .post(format!("{BASE}/reload"))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        self.finish_build_record(
            BuildLifecycleState::Succeeded,
            if reload_ok {
                format!(
                    "WASM recompiled (dx serve PID {serve_pid:?}). \
                     Shell reloaded via eval bridge /reload. Shell window survived."
                )
            } else {
                "WASM recompiled. Shell reload failed — call connect_cdp or launch_app.".to_string()
            },
            if reload_ok {
                "Shell page reload triggered. WASM bundle updated."
            } else {
                "dx serve ready but reload failed. The shell may have crashed — use launch_app."
            },
            None,
        )
        .await;
        tracing::info!("[bg] Rebuild done. reload_ok={reload_ok}");
    }

    /// Background task body for `launch_app` / `rebuild_app` (legacy hotpatch mode).
    ///
    /// Spawns `dx serve --hotpatch` as a long-running background process.
    /// `--hotpatch` is required: it configures `LD_LIBRARY_PATH` so the launched
    /// app binary can find `libpoly_plugin_host.so` and other dynamic libs.
    /// Without `--hotpatch` the binary launches but exits immediately with
    /// code 127 (dynamic linker failure). This matches the working VS Code
    /// `launch.json` "Desktop Wry (Linux)" configuration.
    async fn bg_build_and_launch_desktop(&self, app_dir: &str) {
        // ── Spawn dx serve as a long-running background process ───────────────
        let mut child = match tokio::process::Command::new("dx")
            .args(["serve", "--hotpatch"])
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
                    format!("Could not spawn `dx serve --hotpatch`: {e}"),
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

        let pid = match child.id() {
            Some(p) => p,
            None => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve process has no PID immediately after spawn.",
                    "tokio process::id() returned None.",
                    None,
                )
                .await;
                return;
            }
        };
        // Store the dx serve PID so kill_app can terminate it.
        *self.app_process.lock().await = Some(AppProcess { pid });

        // Auto-clear the handle when dx serve exits (build failure, etc.).
        let app_ref = self.app_process.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *app_ref.lock().await = None;
        });

        let _ = Self::increment_rebuild_counter().await;

        // ── Wait for the eval bridge ──────────────────────────────────────────
        // Allow up to 120 s: dx serve must compile before launching the app.
        match self.wait_for_bridge(120).await {
            Ok(()) => {
                self.finish_build_record(
                    BuildLifecycleState::Succeeded,
                    "Desktop app compiled and launched by dx serve. Eval bridge ready. Call connect_cdp.",
                    format!("Verified {BASE}/status (dx serve PID {pid})."),
                    None,
                )
                .await;
            }
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "dx serve started but eval bridge did not respond within 120 s.",
                    format!(
                        "Bridge not ready at {BASE}/status: {e}. \
                         Check get_last_build_log for dx serve / cargo errors."
                    ),
                    None,
                )
                .await;
            }
        }
    }
}

#[async_trait]
impl DevtoolsBackend for DesktopHttpBackend {
    fn name(&self) -> &str {
        "desktop-http"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        *self.workspace.lock().await = Some(workspace.to_string());

        // ── Guard: refuse concurrent builds ──────────────────────────────────
        {
            let guard = self.build_task.lock().await;
            if let Some(handle) = guard.as_ref()
                && !handle.is_finished()
            {
                return Ok(
                        "A build is already in progress.\n\
                         Poll get_last_build_status — state will change Running → Succeeded/Failed."
                            .to_string(),
                    );
            }
        }

        if is_legacy_mode() {
            // ── Legacy hotpatch mode ──────────────────────────────────────────
            let app_dir = format!("{workspace}/apps/desktop-devtools");
            let _ = tokio::process::Command::new("pkill")
                .args(["-f", "dx.*serve.*hotpatch"])
                .status()
                .await;
            let _ = tokio::process::Command::new("pkill")
                .args(["-f", APP_PROCESS_PATTERN])
                .status()
                .await;
            *self.app_process.lock().await = None;
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;

            self.start_build_record(
                "launch_app",
                "dx serve --hotpatch (legacy)",
                &app_dir,
                "dx serve --hotpatch",
                "dx serve --hotpatch started in background (legacy mode). \
                 Poll get_last_build_status for progress.",
            )
            .await;

            let ctx = self.clone();
            let handle = tokio::spawn(async move {
                ctx.bg_build_and_launch_desktop(&app_dir).await;
            });
            *self.build_task.lock().await = Some(handle);

            Ok("Build started in background (legacy hotpatch mode, state: Running).\n\
                 Poll `get_last_build_status` until state = Succeeded or Failed.\n\
                 On Succeeded: call `connect_cdp`.\n\
                 On Failed: call `get_last_build_log` for the compiler error."
                .to_string())
        } else {
            // ── New web-shell mode ────────────────────────────────────────────
            let app_dir = format!("{workspace}/apps/desktop");
            // Kill stale dx serve and shell
            self.kill_dx_serve().await;
            self.kill_web_shell().await;
            *self.app_process.lock().await = None;
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;

            self.start_build_record(
                "launch_app",
                &format!("dx serve --platform web --port {WEB_SERVE_PORT}"),
                &app_dir,
                &format!("dx serve --platform web --port {WEB_SERVE_PORT}"),
                "dx serve started in background (web-shell mode). \
                 Poll get_last_build_status for progress.",
            )
            .await;

            let ctx = self.clone();
            let ws = workspace.to_string();
            let handle = tokio::spawn(async move {
                ctx.bg_serve_and_launch_web_shell(&app_dir, &ws).await;
            });
            *self.build_task.lock().await = Some(handle);

            Ok(format!(
                "Build started in background (web-shell mode, state: Running).\n\
                 Command: dx serve --platform web --port {WEB_SERVE_PORT}  (in apps/desktop/)\n\
                 First compile takes 30-90 s.\n\
                 \n\
                 Poll get_last_build_status every 5-10 s:\n\
                   state = \"Running\"   → keep polling\n\
                   state = \"Succeeded\" → shell is running, call connect_cdp\n\
                   state = \"Failed\"    → call get_last_build_log for the compiler error\n\
                 \n\
                 The poly-desktop-web shell stays alive across rebuilds — use rebuild_app to recompile WASM.\n\
                 Set POLY_DESKTOP_LEGACY=1 to use the old dx serve --hotpatch mode."
            ))
        }
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        if is_legacy_mode() {
            // Legacy: SIGTERM dx serve (which also terminates the devtools binary).
            if let Some(proc) = self.app_process.lock().await.take() {
                let _ = tokio::process::Command::new("kill")
                    .args(["-15", &proc.pid.to_string()])
                    .status()
                    .await;
            }
            let _ = tokio::process::Command::new("pkill")
                .args(["-f", "dx.*serve.*hotpatch"])
                .status()
                .await;
            let _ = tokio::process::Command::new("pkill")
                .args(["-f", APP_PROCESS_PATTERN])
                .status()
                .await;
            Ok("Killed dx serve and poly-desktop-devtools. Call launch_app to restart.".to_string())
        } else {
            // Web-shell mode: kill dx serve and the shell separately.
            self.kill_dx_serve().await;
            self.kill_web_shell().await;
            *self.app_process.lock().await = None;
            Ok("Killed dx serve and poly-desktop-web shell. Call launch_app to restart.".to_string())
        }
    }

    async fn connect(&self) -> anyhow::Result<String> {
        let resp = self
            .client
            .get(format!("{BASE}/status"))
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Cannot reach eval-bridge at {BASE}/status: {e}\n\
                     Make sure poly-desktop-devtools is running (call launch_app)."
                )
            })?;
        let ok = resp.text().await?;
        self.suppress_rebuild_toast_if_app_ready().await;
        Ok(format!("Eval-bridge connected ✓  ({BASE}/status → {ok})"))
    }

    async fn take_screenshot(
        &self,
        _params: &ScreenshotParams,
    ) -> anyhow::Result<ScreenshotResult> {
        self.suppress_rebuild_toast_if_app_ready().await;
        // Desktop Wry only supports PNG — format/quality params are ignored.
        let image_bytes = http_get(&self.client, "/screenshot").await?;
        Ok(ScreenshotResult {
            image_bytes,
            mime_type: "image/png".to_string(),
        })
    }

    async fn js_eval(&self, expression: &str) -> anyhow::Result<String> {
        self.suppress_rebuild_toast_if_app_ready().await;
        http_eval(&self.client, expression).await
    }

    async fn hard_kill(&self) -> anyhow::Result<String> {
        if is_legacy_mode() {
            if let Some(proc) = self.app_process.lock().await.take() {
                let _ = tokio::process::Command::new("kill")
                    .args(["-9", &proc.pid.to_string()])
                    .status()
                    .await;
            }
            let _ = tokio::process::Command::new("pkill")
                .args(["-9", "-f", "dx.*serve.*hotpatch"])
                .status()
                .await;
            let _ = tokio::process::Command::new("pkill")
                .args(["-9", "-f", APP_PROCESS_PATTERN])
                .status()
                .await;
            Ok(
                "Hard-killed dx serve and poly-desktop-devtools (SIGKILL). Call launch_app to restart."
                    .to_string(),
            )
        } else {
            // Web-shell mode: SIGKILL both dx serve and shell.
            if let Some(pid) = self.dx_serve_pid.lock().await.take() {
                let _ = tokio::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .status()
                    .await;
            }
            if let Some(pid) = self.shell_pid.lock().await.take() {
                let _ = tokio::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .status()
                    .await;
            }
            let _ = tokio::process::Command::new("pkill")
                .args([
                    "-9",
                    "-f",
                    &format!("dx.*serve.*--port.*{WEB_SERVE_PORT}"),
                ])
                .status()
                .await;
            let _ = tokio::process::Command::new("pkill")
                .args(["-9", "-f", WEB_SHELL_PATTERN])
                .status()
                .await;
            *self.app_process.lock().await = None;
            Ok(
                "Hard-killed dx serve and poly-desktop-web shell (SIGKILL). Call launch_app to restart."
                    .to_string(),
            )
        }
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        if is_legacy_mode() {
            // Legacy: full kill + relaunch.
            return self.launch_app(workspace).await;
        }

        // ── Guard: refuse concurrent builds ──────────────────────────────────
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

        let app_dir = format!("{workspace}/apps/desktop");

        let _ = Self::increment_rebuild_counter().await;

        self.start_build_record(
            "rebuild_app",
            &format!("dx serve --platform web --port {WEB_SERVE_PORT}"),
            &app_dir,
            &format!("restart dx serve --platform web --port {WEB_SERVE_PORT}"),
            "Restarting dx serve to recompile WASM. Shell window will NOT restart.",
        )
        .await;

        let ctx = self.clone();
        let app_dir2 = app_dir.clone();
        let handle = tokio::spawn(async move {
            ctx.bg_rebuild_web_shell(&app_dir2).await;
        });
        *self.build_task.lock().await = Some(handle);

        Ok(format!(
            "WASM rebuild started in background (state: Running).\n\
             Restarting: dx serve --platform web --port {WEB_SERVE_PORT}\n\
             Shell window stays alive — only the page content reloads.\n\
             \n\
             Poll get_last_build_status every 5-10 s:\n\
               state = \"Running\"   → keep polling\n\
               state = \"Succeeded\" → call connect_cdp to verify the new build\n\
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
        // Remove poly's data directory.
        let data_dir = dirs_data_path();
        if let Some(dir) = data_dir
            && std::path::Path::new(&dir).exists()
        {
            std::fs::remove_dir_all(&dir)?;
        }

        // Rebuild and relaunch so the app starts fresh at the setup wizard.
        let ws = self.workspace.lock().await.clone();
        if let Some(ws) = ws {
            self.launch_app(&ws)
                .await
                .map(|msg| format!("Data directory removed.\n{msg}"))
        } else {
            Ok(
                "Data directory removed. Call launch_app to rebuild and restart at the setup wizard."
                    .to_string(),
            )
        }
    }

    fn extension_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "get_generation",
                "description": "Returns rebuild-detection counters for this MCP session.\n\n\
                    **Desktop MCP counters:**\n\
                    - **generation**: value from the app's /generation endpoint. Starts at 1 when the\n\
                      binary launches; changes only on full process restart (PID change).\n\
                    - **build_id**: increments on each successful launch_app / rebuild_app call\n\
                      (reads /tmp/poly-devtools-rebuild-counter). 0 = no build this session.\n\
                    - **pid**: OS process ID of the running desktop-devtools binary.\n\n\
                    **Decision table:**\n\
                    - build_id increased → a rebuild was triggered\n\
                    - pid changed → the binary was relaunched (full restart)\n\n\
                    If build_id does not move the way you expect, immediately inspect\n\
                    get_last_build_status and get_last_build_log for the Dioxus CLI error.",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }),
            json!({
                "name": "force_rebuild",
                "description": "Alias for rebuild_app — kills the running app, rebuilds with\n\
                    `dx serve --hotpatch`, and relaunches the fresh binary.\n\n\
                    Identical to calling rebuild_app. Provided for convenience.\n\n\
                    After this tool returns:\n\
                    1. Poll get_last_build_status until Succeeded\n\
                    2. Call connect_cdp\n\
                    3. Verify with get_generation that build_id and pid both changed\n\
                    4. If anything looks wrong, inspect get_last_build_status and get_last_build_log",
                "inputSchema": { "type": "object", "properties": {}, "required": [] }
            }),
        ]
    }

    async fn handle_extension_tool(
        &self,
        name: &str,
        _args: &Value,
    ) -> Option<anyhow::Result<String>> {
        match name {
            "get_generation" => {
                let result = async {
                    let resp = self
                        .client
                        .get(format!("{BASE}/generation"))
                        .send()
                        .await
                        .map_err(|e| anyhow::anyhow!("HTTP error: {e}"))?;
                    resp.text()
                        .await
                        .map_err(|e| anyhow::anyhow!("Read error: {e}"))
                }
                .await;
                Some(result)
            }
            "force_rebuild" => {
                // Identical to rebuild_app: kills the running binary, builds fresh,
                // and relaunches. Provided as a named alias for MCP discoverability.
                let workspace = self
                    .workspace
                    .lock()
                    .await
                    .clone()
                    .unwrap_or_else(|| "/home/laragana/workspcacemsg".to_string());
                Some(self.rebuild_app(&workspace).await)
            }
            _ => None,
        }
    }

    // ── Input method overrides ────────────────────────────────────────────────
    //
    // The desktop eval bridge wraps JS in `async function(dioxus) { SCRIPT }`.
    // The `do_eval` helper further wraps any script containing top-level
    // semicolons in a second IIFE `return (function(){ SCRIPT; return null; })()`
    // which discards the inner function's return value.
    //
    // The fix: start every script with `return ` so that `do_eval` passes it
    // through unchanged (it only skips wrapping when the trimmed JS already
    // starts with `return `).  We wrap ourselves in `return (function(){...})()`
    // which correctly propagates the inner return value.
    //
    // `click_at` additionally works around the WebKit2GTK `elementFromPoint`
    // issue (returns null for physical-pixel coords on HiDPI displays) by:
    //   1. Trying `elementFromPoint(x, y)` at CSS pixel coords.
    //   2. Retrying at `(x/dpr, y/dpr)` in case the caller used physical pixels.
    //   3. Falling back to a `getBoundingClientRect()` scan over all elements.

    async fn click_at(&self, x: f64, y: f64, dbl_click: bool) -> anyhow::Result<String> {
        let count = if dbl_click { 2 } else { 1 };
        // Starts with `return ` → do_eval passes it through without double-wrapping.
        //
        // IMPORTANT: x,y must be CSS pixel coordinates (same space as
        // getBoundingClientRect()), NOT screenshot display pixels.
        // The displayed screenshot image is scaled by the viewer — do NOT use
        // image pixel offsets here.  Always use getBoundingClientRect() to find
        // exact element centres before calling click_at.
        let js = format!(
            r#"return (function(){{
                var rx={x},ry={y},count={count};
                var dpr=window.devicePixelRatio||1;

                // Interactive tags / roles — prefer these as the click target over
                // presentational children (span, div, svg, etc).
                var INTERACTIVE=['A','BUTTON','INPUT','SELECT','TEXTAREA','LABEL'];
                var INTERACTIVE_ROLES=['button','link','menuitem','option','tab','checkbox','radio','combobox','listbox'];

                function isInteractive(el){{
                    if(INTERACTIVE.indexOf(el.tagName)!==-1)return true;
                    var r=(el.getAttribute('role')||'').toLowerCase();
                    if(INTERACTIVE_ROLES.indexOf(r)!==-1)return true;
                    if(el.hasAttribute('onclick')||el.hasAttribute('data-dioxus-id'))return true;
                    return false;
                }}

                // Walk up from a hit element to the nearest interactive ancestor
                // (within 8 hops) — this prevents landing on a child span/svg
                // inside a button and missing the click handler.
                function liftToInteractive(el){{
                    var cur=el,hops=0;
                    while(cur&&hops<8){{
                        if(isInteractive(cur))return cur;
                        cur=cur.parentElement;hops++;
                    }}
                    return el; // give up — return original
                }}

                // Hit-test: try native first, then bounding-rect scan.
                function findAt(cx,cy){{
                    var el=document.elementFromPoint(cx,cy);
                    if(el&&el!==document.documentElement&&el!==document.body)return el;
                    // Manual scan: smallest element whose rect contains the point.
                    var all=Array.from(document.querySelectorAll('*')),best=null,bestSz=Infinity;
                    for(var i=0;i<all.length;i++){{
                        var r=all[i].getBoundingClientRect();
                        if(cx>=r.left&&cx<=r.right&&cy>=r.top&&cy<=r.bottom){{
                            var sz=r.width*r.height;
                            if(sz>0&&sz<bestSz){{bestSz=sz;best=all[i];}}
                        }}
                    }}
                    return best;
                }}

                // Try CSS pixel coords; if null and DPR!=1 try scaled fallback.
                var hit=findAt(rx,ry);
                var usedX=rx,usedY=ry;
                if(!hit&&dpr!==1){{
                    var sx=rx/dpr,sy=ry/dpr;
                    hit=findAt(sx,sy);
                    if(hit){{usedX=sx;usedY=sy;}}
                }}
                if(!hit)return 'No element at ('+rx+','+ry+') dpr='+dpr+'. Use evaluate_script+getBoundingClientRect() for exact coords.';

                // Lift to nearest interactive ancestor to avoid missing Dioxus handlers.
                var el=liftToInteractive(hit);

                el.scrollIntoView({{block:'nearest',behavior:'instant'}});
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{el.focus();}}

                var opts={{bubbles:true,cancelable:true,clientX:usedX,clientY:usedY,screenX:usedX,screenY:usedY,view:window}};
                for(var k=0;k<count;k++){{
                    el.dispatchEvent(new PointerEvent('pointerdown',Object.assign({{pointerId:1,isPrimary:true}},opts)));
                    el.dispatchEvent(new MouseEvent('mousedown',opts));
                    el.dispatchEvent(new PointerEvent('pointerup',Object.assign({{pointerId:1,isPrimary:true}},opts)));
                    el.dispatchEvent(new MouseEvent('mouseup',opts));
                    el.dispatchEvent(new MouseEvent('click',Object.assign({{detail:count}},opts)));
                }}

                var tag=el.tagName.toLowerCase();
                var id=el.id?'#'+el.id:'';
                var cls=(el.className&&typeof el.className==='string')&&el.className.trim()?'.'+el.className.trim().split(/\s+/)[0]:'';
                var txt=(el.textContent||el.value||'').trim().slice(0,60);
                var liftMsg=(el!==hit)?' (lifted from '+hit.tagName.toLowerCase()+')'  :'';
                return 'Clicked '+tag+(id||cls||'')+liftMsg+' at ('+usedX+','+usedY+')'+(txt?' "'+txt+'"':'');
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn click_element(&self, selector: &str) -> anyhow::Result<String> {
        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        // Starts with `return ` → bypasses do_eval double-wrapping.
        let js = format!(
            r#"return (function(){{
                var el=document.querySelector('{escaped}');
                if(!el)return 'Error: No element found for selector: {escaped}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{el.focus();}}
                el.dispatchEvent(new MouseEvent('click',{{bubbles:true,cancelable:true}}));
                var tag=el.tagName.toLowerCase();
                var id=el.id?'#'+el.id:'';
                var txt=(el.textContent||el.value||'').trim().slice(0,50);
                return 'Clicked '+tag+id+(txt?' "'+txt+'"':'');
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn hover_element(&self, selector: &str) -> anyhow::Result<String> {
        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"return (function(){{
                var el=document.querySelector('{escaped}');
                if(!el)return 'Error: No element found for selector: {escaped}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                var rect=el.getBoundingClientRect();
                var cx=rect.left+rect.width/2,cy=rect.top+rect.height/2;
                var opts={{bubbles:true,clientX:cx,clientY:cy,view:window}};
                el.dispatchEvent(new MouseEvent('mouseenter',opts));
                el.dispatchEvent(new MouseEvent('mouseover',opts));
                el.dispatchEvent(new MouseEvent('mousemove',opts));
                return 'Hovered over '+el.tagName.toLowerCase()+(el.id?'#'+el.id:'');
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn fill_element(&self, selector: &str, value: &str) -> anyhow::Result<String> {
        let sel = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let val = value.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            r#"return (function(){{
                var el=document.querySelector('{sel}');
                if(!el)return 'Error: No element found for selector: {sel}';
                el.scrollIntoView({{block:'center',behavior:'instant'}});
                el.focus();
                if(el.tagName==='SELECT'){{
                    for(var i=0;i<el.options.length;i++){{
                        if(el.options[i].value==='{val}'||el.options[i].text==='{val}'){{
                            el.selectedIndex=i;
                            el.dispatchEvent(new Event('change',{{bubbles:true}}));
                            return 'Selected "'+el.options[i].text+'"';
                        }}
                    }}
                    return 'Error: Option not found: {val}';
                }}
                var nativeSet=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')
                    ||Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value');
                if(nativeSet&&nativeSet.set)nativeSet.set.call(el,'{val}');else el.value='{val}';
                el.dispatchEvent(new Event('input',{{bubbles:true}}));
                el.dispatchEvent(new Event('change',{{bubbles:true}}));
                return 'Filled '+el.tagName.toLowerCase()+(el.id?'#'+el.id:'')+' with "'+'{val}'.slice(0,40)+'"';
            }})()"#
        );
        self.js_eval(&js).await
    }

    async fn type_text(&self, text: &str, submit_key: Option<&str>) -> anyhow::Result<String> {
        let escaped_text = text.replace('\\', "\\\\").replace('\'', "\\'");
        let key_js = match submit_key {
            Some(k) => {
                let ek = k.replace('\'', "\\'");
                format!(
                    "el.dispatchEvent(new KeyboardEvent('keydown',{{key:'{ek}',bubbles:true}}));\
                     el.dispatchEvent(new KeyboardEvent('keyup',{{key:'{ek}',bubbles:true}}));"
                )
            }
            None => String::new(),
        };
        let display = match submit_key {
            Some(k) => format!("Typed \"{escaped_text}\" + {k}"),
            None => format!("Typed \"{escaped_text}\""),
        };
        let js = format!(
            r#"return (function(){{
                var el=document.activeElement||document.body;
                var t='{escaped_text}';
                if(el.tagName==='INPUT'||el.tagName==='TEXTAREA'){{
                    var nativeSet=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')
                        ||Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value');
                    if(nativeSet&&nativeSet.set)nativeSet.set.call(el,el.value+t);else el.value+=t;
                    el.dispatchEvent(new Event('input',{{bubbles:true}}));
                    el.dispatchEvent(new Event('change',{{bubbles:true}}));
                }}else{{
                    for(var i=0;i<t.length;i++){{
                        var c=t[i];
                        el.dispatchEvent(new KeyboardEvent('keydown',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keypress',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keyup',{{key:c,bubbles:true}}));
                    }}
                }}
                {key_js}
                return '{display}';
            }})()"#
        );
        self.js_eval(&js).await
    }
}

/// Best-effort path to Poly's data directory.
fn dirs_data_path() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(format!("{home}/.local/share/poly"))
}

// ─── CLI Mode ─────────────────────────────────────────────────────────────────
//
// PREFERRED: Use the CLI over MCP access wherever possible.
// CLI is faster, scriptable, and testable without a Copilot MCP session.
//
// Usage examples:
//   cargo run --bin poly-desktop-devtools-mcp -- status
//   cargo run --bin poly-desktop-devtools-mcp -- screenshot --save /tmp/shot.png
//   cargo run --bin poly-desktop-devtools-mcp -- snapshot
//   cargo run --bin poly-desktop-devtools-mcp -- eval "document.title"
//   cargo run --bin poly-desktop-devtools-mcp -- launch /path/to/workspace
//   cargo run --bin poly-desktop-devtools-mcp -- generation

/// Detect the workspace root at runtime (POLY_WORKSPACE env var or cwd).
fn cli_detect_workspace() -> String {
    if let Ok(ws) = std::env::var("POLY_WORKSPACE") {
        return ws;
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".to_string())
}

/// Commands that trigger CLI mode instead of MCP server mode.
const CLI_COMMANDS: &[&str] = &[
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
fn is_cli_mode(args: &[String]) -> bool {
    args.get(1)
        .map(|a| CLI_COMMANDS.contains(&a.as_str()))
        .unwrap_or(false)
}

/// Write a line to stdout without using `println!`.
fn cli_write(text: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    writeln!(std::io::stdout().lock(), "{text}")?;
    Ok(())
}

/// Extract value of `--flag <value>` from args.
fn extract_cli_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).map(String::as_str)
}

/// CLI help text for the desktop MCP.
fn desktop_cli_help() -> &'static str {
    "poly-desktop-devtools-mcp — CLI mode (PREFERRED over MCP)

COMMANDS:
  status                    Check if app is running
  launch [workspace]        Start the devtools app (non-blocking, polls until done)
  rebuild [workspace]       Rebuild and relaunch (non-blocking, polls until done)
  kill                      Stop the devtools app
  screenshot [--save path]  Take a screenshot (saves PNG or prints base64)
  snapshot [--verbose]      Print DOM snapshot
  eval <script>             Evaluate JavaScript expression
  click <selector>          Click a CSS selector
  fill <selector> <value>   Fill an input element
  navigate <url>            Navigate to a URL
  generation                Get rebuild/hotpatch generation counters
    build-status              Get structured diagnostics for the last Dioxus build/hotpatch
    build-log                 Get the raw log for the last Dioxus build/hotpatch
  help                      Show this help

MCP mode (default, no subcommand):
  cargo run --bin poly-desktop-devtools-mcp
"
}

/// Handle the `screenshot` CLI command.
async fn cli_screenshot_cmd(
    backend: &DesktopHttpBackend,
    args: &[String],
) -> anyhow::Result<String> {
    use base64::Engine as _;
    let save_path = extract_cli_flag(args, "--save");
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

/// Dispatch a single CLI command for the desktop backend.
async fn dispatch_desktop_cli(
    backend: &DesktopHttpBackend,
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
                .unwrap_or_else(cli_detect_workspace);
            // launch_app is non-blocking — poll until background build finishes,
            // otherwise the process exits and kills the tokio task.
            let initial_msg = backend.launch_app(&ws).await?;
            cli_write(&initial_msg)?;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let status_str = backend.get_last_build_status().await.unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&status_str).unwrap_or(serde_json::Value::Null);
                let state = parsed
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                cli_write(&format!("[build] state = {state}"))?;
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
                .unwrap_or_else(cli_detect_workspace);
            // rebuild_app is non-blocking — poll until background rebuild finishes.
            let initial_msg = backend.rebuild_app(&ws).await?;
            cli_write(&initial_msg)?;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let status_str = backend.get_last_build_status().await.unwrap_or_default();
                let parsed: serde_json::Value =
                    serde_json::from_str(&status_str).unwrap_or(serde_json::Value::Null);
                let state = parsed
                    .get("state")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown");
                cli_write(&format!("[rebuild] state = {state}"))?;
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
        "fill" => dispatch_desktop_fill(backend, args).await,
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
        "generation" => http_get(&backend.client, "/generation")
            .await
            .map(|b| String::from_utf8_lossy(&b).into_owned()),
        "build-status" => backend.get_last_build_status().await,
        "build-log" => backend.get_last_build_log().await,
        "screenshot" => cli_screenshot_cmd(backend, args).await,
        _ => Ok(desktop_cli_help().to_string()),
    }
}

async fn dispatch_desktop_fill(
    backend: &DesktopHttpBackend,
    args: &[String],
) -> anyhow::Result<String> {
    let sel = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("Usage: fill <selector> <value>"))?;
    let val = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: fill <selector> <value>"))?;
    backend.fill_element(sel, val).await
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let backend = DesktopHttpBackend::new();
    if is_cli_mode(&args) {
        let cmd = args.get(1).map(String::as_str).unwrap_or("help");
        let rest = args.get(2..).unwrap_or(&[]).to_vec();
        match dispatch_desktop_cli(&backend, cmd, &rest).await {
            Ok(out) => {
                if let Err(e) = cli_write(&out) {
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
        run_mcp_loop(&backend, "poly-devtools-desktop").await;
    }
}
