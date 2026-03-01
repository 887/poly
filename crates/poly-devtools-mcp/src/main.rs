//! # poly-devtools-mcp
//!
//! MCP server for the **desktop** devtools backend.
//!
//! Communicates with `poly-desktop-devtools` via its embedded HTTP eval-bridge
//! at `http://127.0.0.1:9223`. Implements [`DevtoolsBackend`] from the shared
//! protocol crate and delegates the MCP main loop to [`run_mcp_loop`].
//!
//! ## Usage
//! ```bash
//! cargo run --bin poly-devtools-mcp
//! ```
//! Or via `.vscode/mcp.json` for GitHub Copilot integration.

use std::process::Stdio;

use async_trait::async_trait;
use poly_devtools_protocol::backend::{DevtoolsBackend, ScreenshotResult};
use poly_devtools_protocol::mcp::run_mcp_loop;
use serde_json::Value;

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

// ─── Desktop HTTP Backend ─────────────────────────────────────────────────────

/// Desktop devtools backend — talks to the embedded HTTP eval-bridge.
struct DesktopHttpBackend {
    client: reqwest::Client,
}

impl DesktopHttpBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
        }
    }
}

#[async_trait]
impl DevtoolsBackend for DesktopHttpBackend {
    fn name(&self) -> &str {
        "desktop-http"
    }

    async fn launch_app(&self, workspace: &str) -> anyhow::Result<String> {
        // Kill any existing instance first to prevent double-instances on port 9223.
        let _ = tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-devtools"])
            .status()
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        // Use dx build to get proper asset!() processing, then launch the output binary.
        let app_dir = format!("{workspace}/apps/desktop-devtools");
        let binary = format!(
            "{workspace}/target/dx/poly-desktop-devtools/debug/linux/app/poly-desktop-devtools"
        );

        // Build with dx if binary doesn't exist
        if !std::path::Path::new(&binary).exists() {
            let status = tokio::process::Command::new("dx")
                .args(["build", "--platform", "desktop"])
                .current_dir(&app_dir)
                .status()
                .await?;
            if !status.success() {
                return Err(anyhow::anyhow!("dx build failed"));
            }
        }

        tokio::process::Command::new(&binary)
            .current_dir(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(format!(
            "Launched {binary}\nWait ~2 seconds then call connect_cdp to verify."
        ))
    }

    async fn kill_app(&self) -> anyhow::Result<String> {
        tokio::process::Command::new("pkill")
            .args(["-f", "poly-desktop-devtools"])
            .status()
            .await?;
        Ok("Killed poly-desktop-devtools process(es)".to_string())
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
                     Make sure poly-desktop-devtools is running."
                )
            })?;
        let ok = resp.text().await?;
        Ok(format!("Eval-bridge connected ✓  ({BASE}/status → {ok})"))
    }

    async fn screenshot(&self) -> anyhow::Result<ScreenshotResult> {
        let png_bytes = http_get(&self.client, "/screenshot").await?;
        // Also save to devtools-screenshots/ for inline file viewing.
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
            r#"(function(){{
                var el = document.elementFromPoint({x},{y});
                if (!el) return 'No element at ({x},{y})';
                el.dispatchEvent(new MouseEvent('click',{{bubbles:true,cancelable:true,clientX:{x},clientY:{y}}}));
                return 'Clicked: ' + el.tagName + (el.id ? '#'+el.id : '') + (el.className ? '.'+el.className.split(' ').join('.') : '');
            }})();"#,
            x = x,
            y = y
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

    async fn reset_app(&self) -> anyhow::Result<String> {
        // Kill the app, remove the SurrealKV data directory, then relaunch
        let _ = self.kill_app().await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Remove poly's data directory
        let data_dir = dirs_data_path();
        if let Some(dir) = data_dir
            && std::path::Path::new(&dir).exists()
        {
            std::fs::remove_dir_all(&dir)?;
        }

        Ok(
            "App killed and data directory removed. Call launch_app to restart at setup wizard."
                .to_string(),
        )
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
