//! # poly-desktop-devtools-mcp
//!
//! MCP server for the **desktop** devtools backend.
//!
//! Builds the desktop-devtools app via `dx build --platform desktop` (one-shot,
//! synchronous, immediate exit-code feedback) and then launches the resulting
//! binary directly.  The binary embeds an HTTP eval-bridge that this MCP uses
//! for JS evaluation, screenshots, and DOM inspection.
//!
//! ## Build model
//!
//! - `launch_app` runs `dx build --platform desktop`, waits for the process to
//!   exit (success or failure is **immediately** visible), then spawns the built
//!   binary and waits for the eval-bridge at `http://127.0.0.1:9223`.
//! - `rebuild_app` kills the running binary and re-runs `launch_app`.
//! - No file watchers, no hotpatch, no background serve process — just a plain
//!   build + binary launch.
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

/// Desktop devtools backend — builds the app via `dx build --platform desktop`
/// then launches the resulting binary and communicates via its HTTP eval-bridge
/// at [`BASE`].
struct DesktopHttpBackend {
    client: reqwest::Client,
    /// Handle to the running desktop-devtools binary process.
    app_process: Arc<Mutex<Option<AppProcess>>>,
    /// Workspace path — set during `launch_app`, used by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
    /// Rolling combined stdout/stderr log from Dioxus build and app output.
    build_log: Arc<Mutex<RollingBuildLog>>,
    /// Structured diagnostics for the last build attempt.
    last_build: Arc<Mutex<Option<DesktopBuildRecord>>>,
}

impl DesktopHttpBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            app_process: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
            build_log: Arc::new(Mutex::new(RollingBuildLog::default())),
            last_build: Arc::new(Mutex::new(None)),
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
            mode: "dx build --platform desktop".to_string(),
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

    /// Poll the eval bridge until it responds or timeout.
    async fn wait_for_bridge(&self, max_seconds: u64) -> anyhow::Result<()> {
        let polls = max_seconds * 2; // poll every 500 ms
        for _ in 0..polls {
            if self.is_bridge_alive().await {
                return Ok(());
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
}

#[async_trait]
impl DevtoolsBackend for DesktopHttpBackend {
    fn name(&self) -> &str {
        "desktop-http"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        *self.workspace.lock().await = Some(workspace.to_string());
        let app_dir = format!("{workspace}/apps/desktop-devtools");
        let binary_path = format!(
            "{workspace}/target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools"
        );

        // ── Step 1: Kill any stale app process ───────────────────────────
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", APP_PROCESS_PATTERN])
            .status()
            .await;
        *self.app_process.lock().await = None;
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        // ── Step 2: Build with dx build --platform desktop ───────────────
        //
        // This blocks until the build finishes.  The exit code gives immediate
        // pass/fail feedback — no file watchers, no hotpatch ambiguity.
        self.start_build_record(
            "launch_app",
            "dx build --platform desktop",
            &app_dir,
            "dx build --platform desktop",
            "Building desktop app — blocks until complete, exit code gives immediate pass/fail.",
        )
        .await;

        let build_output = tokio::process::Command::new("dx")
            .args(["build", "--platform", "desktop"])
            .current_dir(&app_dir)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn `dx build --platform desktop`: {e}"))?;

        // Push build output to the rolling log.
        {
            let mut buffer = self.build_log.lock().await;
            for line in String::from_utf8_lossy(&build_output.stdout).lines() {
                buffer.push_line(format!("[stdout] {line}"));
            }
            for line in String::from_utf8_lossy(&build_output.stderr).lines() {
                buffer.push_line(format!("[stderr] {line}"));
            }
        }

        if !build_output.status.success() {
            self.finish_build_record(
                BuildLifecycleState::Failed,
                "Desktop build failed.",
                "dx build --platform desktop exited with a non-zero status. \
                 Check get_last_build_log for the exact compiler/Dioxus error.",
                build_output.status.code(),
            )
            .await;
            anyhow::bail!(
                "dx build --platform desktop failed (exit {:?}). \
                 Call get_last_build_log to see the compiler error.",
                build_output.status.code()
            );
        }

        // Build succeeded — increment the rebuild counter.
        let _ = Self::increment_rebuild_counter().await;

        // ── Step 3: Launch the built binary ──────────────────────────────
        let mut child = tokio::process::Command::new(&binary_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to launch built binary at {binary_path}: {e}"))?;

        if let Some(stdout) = child.stdout.take() {
            spawn_log_reader(stdout, "app-stdout", self.build_log.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_log_reader(stderr, "app-stderr", self.build_log.clone());
        }

        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("Launched app process has no PID"))?;
        *self.app_process.lock().await = Some(AppProcess { pid });

        let app_ref = self.app_process.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *app_ref.lock().await = None;
        });

        // ── Step 4: Wait for the eval bridge ─────────────────────────────
        // Short timeout — the binary starts fast since the build already finished.
        match self.wait_for_bridge(30).await {
            Ok(()) => {
                self.finish_build_record(
                    BuildLifecycleState::Succeeded,
                    "Desktop app built and launched. Eval bridge is ready.",
                    format!("Verified by reaching {BASE}/status after launching the built binary."),
                    build_output.status.code(),
                )
                .await;
                Ok(format!(
                    "Build succeeded ✓ — app launched (PID: {pid})\n\
                     Eval bridge ready at {BASE}\n\
                     Call connect_cdp to start interacting. Use rebuild_app to rebuild and relaunch."
                ))
            }
            Err(e) => {
                self.finish_build_record(
                    BuildLifecycleState::Failed,
                    "Desktop build succeeded but eval bridge did not respond.",
                    format!(
                        "Bridge not ready at {BASE}/status within 30s: {e}. \
                         Check get_last_build_log for app startup errors."
                    ),
                    build_output.status.code(),
                )
                .await;
                anyhow::bail!(
                    "Build succeeded but eval bridge did not respond within 30s: {e}"
                )
            }
        }
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        // SIGTERM the app by PID if we have it.
        if let Some(proc) = self.app_process.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-15", &proc.pid.to_string()])
                .status()
                .await;
        }
        // Pattern fallback to catch any stray instances.
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", APP_PROCESS_PATTERN])
            .status()
            .await;

        Ok("Killed poly-desktop-devtools. Call launch_app to rebuild and restart.".to_string())
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
        Ok(format!("Eval-bridge connected ✓  ({BASE}/status → {ok})"))
    }

    async fn take_screenshot(
        &self,
        _params: &ScreenshotParams,
    ) -> anyhow::Result<ScreenshotResult> {
        // Desktop Wry only supports PNG — format/quality params are ignored.
        let image_bytes = http_get(&self.client, "/screenshot").await?;
        Ok(ScreenshotResult {
            image_bytes,
            mime_type: "image/png".to_string(),
        })
    }

    async fn js_eval(&self, expression: &str) -> anyhow::Result<String> {
        http_eval(&self.client, expression).await
    }

    async fn hard_kill(&self) -> anyhow::Result<String> {
        // SIGKILL the app process by PID (avoids killing the MCP).
        if let Some(proc) = self.app_process.lock().await.take() {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &proc.pid.to_string()])
                .status()
                .await;
        }
        // Pattern fallback for any orphaned instances.
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", APP_PROCESS_PATTERN])
            .status()
            .await;

        Ok("Hard-killed poly-desktop-devtools (SIGKILL). Call launch_app to rebuild and restart."
            .to_string())
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Kills the running binary then rebuilds and relaunches via launch_app.
        // launch_app already kills stale instances at the start, so this is safe
        // to call directly without an extra kill step.
        self.launch_app(workspace).await
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
                    `dx build --platform desktop`, and relaunches the fresh binary.\n\n\
                    Identical to calling rebuild_app. Provided for convenience.\n\n\
                    After this tool returns:\n\
                    1. Call connect_cdp\n\
                    2. Verify with get_generation that build_id and pid both changed\n\
                    3. If anything looks wrong, inspect get_last_build_status and get_last_build_log",
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
  launch [workspace]        Start the devtools app
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
