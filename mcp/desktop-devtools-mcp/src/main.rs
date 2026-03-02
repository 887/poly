//! # poly-desktop-devtools-mcp
//!
//! MCP server for the **desktop** devtools backend.
//!
//! Launches the desktop-devtools app via `dx serve` and communicates with the app
//! via its embedded HTTP eval-bridge at `http://127.0.0.1:9223`.
//!
//! ## Hot Reload
//!
//! The app runs under `dx serve` with file-watcher-based hot-reload. When you
//! make changes to poly-core, use the `rebuild_app` MCP tool which touches a
//! source file to trigger a full rebuild.
//!
//! ## Usage
//! ```bash
//! cargo run --bin poly-desktop-devtools-mcp
//! ```
//! Or via `.vscode/mcp.json` for GitHub Copilot integration.

use std::process::Stdio;
use std::sync::Arc;

use async_trait::async_trait;
use poly_devtools_protocol::backend::{DevtoolsBackend, ScreenshotResult};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::Value;
use tokio::sync::Mutex;

const BASE: &str = "http://127.0.0.1:9223";

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

// ─── dx serve Process State ──────────────────────────────────────────────────

/// Handle to a managed `dx serve` process.
///
/// Tracks the process ID for hard-kill via SIGKILL.
struct DxServeProcess {
    /// OS process ID — used for hard-kill via SIGKILL.
    pid: u32,
}

// ─── Desktop HTTP Backend ─────────────────────────────────────────────────────

/// Desktop devtools backend — launches the app via `dx serve` and
/// talks to the embedded HTTP eval-bridge at [`BASE`].
struct DesktopHttpBackend {
    client: reqwest::Client,
    /// Handle to the managed `dx serve` process (if we started it).
    dx_serve: Arc<Mutex<Option<DxServeProcess>>>,
    /// Workspace path — set during `launch_app`, used by `rebuild_app`.
    workspace: Arc<Mutex<Option<String>>>,
}

impl DesktopHttpBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            dx_serve: Arc::new(Mutex::new(None)),
            workspace: Arc::new(Mutex::new(None)),
        }
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

    /// Touch a source file to trigger ``dx serve``'s file watcher, causing a
    /// full rebuild.
    async fn touch_source_file(workspace: &str) -> anyhow::Result<()> {
        // Touch the core lib.rs — this is in the hot-reload watch path and
        // guarantees a recompilation of the devtools binary.
        let trigger = format!("{workspace}/crates/core/src/lib.rs");
        tokio::process::Command::new("touch")
            .arg(&trigger)
            .status()
            .await?;
        Ok(())
    }
}

#[async_trait]
impl DevtoolsBackend for DesktopHttpBackend {
    fn name(&self) -> &str {
        "desktop-http"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Remember workspace for rebuild_app / reset_app.
        *self.workspace.lock().await = Some(workspace.to_string());

        // ── Step 1: check if an existing instance is already healthy ──────
        if self.is_bridge_alive().await {
            return Ok(format!(
                "App already running on {BASE} — reusing existing instance.\n\
                 Hot reload is active. Call connect_cdp to interact."
            ));
        }

        // ── Step 2: kill any stale processes ──────────────────────────────
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-devtools[^-]"])
            .status()
            .await;
        // Also kill any stale dx serve for this app.
        let _ = tokio::process::Command::new("bash")
            .args([
                "-c",
                "pkill -f 'dx.*serve.*desktop-devtools' 2>/dev/null || true",
            ])
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;

        // ── Step 3: start dx serve ───────────────────────────────────────
        let app_dir = format!("{workspace}/apps/desktop-devtools");
        let mut child = tokio::process::Command::new("dx")
            .args(["serve", "--platform", "desktop"])
            .current_dir(&app_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;

        let pid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("dx serve process has no PID"))?;

        *self.dx_serve.lock().await = Some(DxServeProcess { pid });

        // Background task: reap the child and clean up state on exit.
        let dx_ref = self.dx_serve.clone();
        tokio::spawn(async move {
            let _ = child.wait().await;
            *dx_ref.lock().await = None;
        });

        // ── Step 4: wait for eval bridge (first build can take a while) ───
        // Poll for up to 120 s — initial compilation can be slow.
        match self.wait_for_bridge(120).await {
            Ok(()) => Ok(format!(
                "dx serve started in {app_dir}\n\
                 Eval bridge ready at {BASE}\n\
                 Hot reload is active — file changes trigger automatic rebuild.\n\
                 Use rebuild_app for forced rebuild, kill_app to stop everything."
            )),
            Err(_) => Ok(format!(
                "dx serve started in {app_dir}\n\
                 Eval bridge not yet responding at {BASE} — first build may still be compiling.\n\
                 Call connect_cdp in a moment to check."
            )),
        }
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        // Drop the dx serve handle (closes stdin, helping it exit).
        *self.dx_serve.lock().await = None;

        // Kill the desktop app process (not the MCP server).
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-devtools[^-]"])
            .status()
            .await;
        // Kill dx serve for this app.
        let _ = tokio::process::Command::new("bash")
            .args([
                "-c",
                "pkill -f 'dx.*serve.*desktop-devtools' 2>/dev/null || true",
            ])
            .status()
            .await;

        Ok("Killed poly-desktop-devtools and dx serve. Call launch_app to restart.".to_string())
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

    async fn screenshot(&self) -> anyhow::Result<ScreenshotResult> {
        let png_bytes = http_get(&self.client, "/screenshot").await?;
        let dir = "devtools-screenshots";
        let _ = std::fs::create_dir_all(dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = format!("{dir}/desktop-{ts}.png");
        let _ = std::fs::write(&path, &png_bytes);
        Ok(ScreenshotResult { png_bytes })
    }

    async fn js_eval(&self, expression: &str) -> anyhow::Result<String> {
        http_eval(&self.client, expression).await
    }

    async fn get_dom(&self) -> anyhow::Result<String> {
        Ok(String::from_utf8(http_get(&self.client, "/dom").await?)?)
    }

    async fn get_console(&self) -> anyhow::Result<String> {
        Ok(String::from_utf8(
            http_get(&self.client, "/console").await?,
        )?)
    }

    async fn click(&self, x: i64, y: i64) -> anyhow::Result<String> {
        let js = format!(
            r#"return (function() {{
                var x = {x}, y = {y};
                var el = document.elementFromPoint(x, y);
                if (!el) return 'No element at (' + x + ',' + y + ')';
                var opts = {{
                    bubbles: true, cancelable: true,
                    clientX: x, clientY: y, screenX: x, screenY: y,
                    view: window
                }};
                el.dispatchEvent(new PointerEvent('pointerdown', Object.assign({{pointerId:1,isPrimary:true}}, opts)));
                el.dispatchEvent(new MouseEvent('mousedown', opts));
                el.dispatchEvent(new PointerEvent('pointerup',   Object.assign({{pointerId:1,isPrimary:true}}, opts)));
                el.dispatchEvent(new MouseEvent('mouseup',   opts));
                el.dispatchEvent(new MouseEvent('click',     opts));
                var cls = (el.className || '').toString().trim().replace(/\s+/g, '.');
                var txt = (el.textContent || '').trim().slice(0, 40);
                return 'Clicked ' + el.tagName
                    + (el.id ? '#' + el.id : '')
                    + (cls   ? '.' + cls   : '')
                    + (txt   ? ' "' + txt + '"' : '')
                    + ' at (' + x + ',' + y + ')';
            }})();"#
        );
        http_eval(&self.client, &js).await
    }

    async fn type_text(&self, text: &str) -> anyhow::Result<String> {
        let escaped = text.replace('\'', "\\'");
        let js = format!(
            r#"(function(){{
                var el = document.activeElement || document.body;
                var t = '{escaped}';
                if (el.tagName==='INPUT'||el.tagName==='TEXTAREA') {{
                    el.value += t;
                    el.dispatchEvent(new InputEvent('input',{{bubbles:true}}));
                    el.dispatchEvent(new Event('change',{{bubbles:true}}));
                }} else {{
                    for (var i=0;i<t.length;i++) {{
                        var c=t[i];
                        el.dispatchEvent(new KeyboardEvent('keydown',{{key:c,bubbles:true}}));
                        el.dispatchEvent(new KeyboardEvent('keyup',{{key:c,bubbles:true}}));
                    }}
                }}
                return 'typed: '+t;
            }})();"#
        );
        http_eval(&self.client, &js).await
    }

    async fn hard_kill(&self) -> anyhow::Result<String> {
        // SIGKILL the dx serve process by PID (precise — avoids killing the MCP).
        let pid = self.dx_serve.lock().await.as_ref().map(|s| s.pid);
        *self.dx_serve.lock().await = None;

        if let Some(pid) = pid {
            let _ = tokio::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .status()
                .await;
        }
        // Also SIGKILL the app and any orphaned dx child processes.
        let _ = tokio::process::Command::new("pkill")
            .args(["-9", "-f", "poly-desktop-devtools[^-]"])
            .status()
            .await;
        let _ = tokio::process::Command::new("bash")
            .args([
                "-c",
                "pkill -9 -f 'dx.*serve.*desktop-devtools' 2>/dev/null || true",
            ])
            .status()
            .await;

        Ok(
            "Hard-killed dx serve and poly-desktop-devtools (SIGKILL). Call launch_app to restart."
                .to_string(),
        )
    }

    async fn browser_reload(&self) -> anyhow::Result<String> {
        // Reload the webview page. After reload, the devtools head script
        // re-injects automatically (it is in the custom <head>), and the
        // HTTP eval bridge stays up since it lives in native Rust.
        // We tolerate an eval error here because a reload disconnects the JS
        // context briefly — the caller should wait ~1s then call connect_cdp.
        match http_eval(
            &self.client,
            "return (function(){ window.location.reload(); return 'reloading'; })()",
        )
        .await
        {
            Ok(_) | Err(_) => Ok(
                "Browser reload triggered. Wait ~1s for the webview to settle, then call connect_cdp."
                    .to_string(),
            ),
        }
    }

    async fn rebuild_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Touch a source file to trigger dx serve's file watcher, causing a
        // full rebuild.
        Self::touch_source_file(workspace).await?;

        // Wait a moment for the rebuild to start, then poll the bridge.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        match self.wait_for_bridge(120).await {
            Ok(()) => Ok("Rebuild triggered (touched crates/core/src/lib.rs).\n\
                 dx serve is recompiling — this takes 10-30s with a warm cache.\n\
                 Eval bridge will reconnect when done."
                .to_string()),
            Err(e) => Err(anyhow::anyhow!(
                "Rebuild triggered but eval bridge didn't come back: {e}"
            )),
        }
    }

    async fn reset_app(&self) -> anyhow::Result<String> {
        // Remove poly's data directory.
        let data_dir = dirs_data_path();
        if let Some(dir) = data_dir
            && std::path::Path::new(&dir).exists()
        {
            std::fs::remove_dir_all(&dir)?;
        }

        // Trigger a rebuild so the app restarts fresh at the setup wizard.
        let ws = self.workspace.lock().await.clone();
        if let Some(ws) = ws {
            // Touch a source file to trigger rebuild.
            Self::touch_source_file(&ws).await?;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _ = self.wait_for_bridge(60).await;
            Ok(
                "Data directory removed and rebuild triggered. App should restart at setup wizard."
                    .to_string(),
            )
        } else {
            Ok(
                "Data directory removed. Call launch_app or rebuild_app to restart at setup wizard."
                    .to_string(),
            )
        }
    }
}

/// Best-effort path to Poly's data directory.
fn dirs_data_path() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(format!("{home}/.local/share/poly"))
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let backend = DesktopHttpBackend::new();
    run_mcp_loop(&backend, "poly-devtools-desktop").await;
}
