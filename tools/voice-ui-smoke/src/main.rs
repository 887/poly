//! Voice-banner UI smoke test — K.4 of `docs/plans/plan-voice-video-calls.md`.
//!
//! Drives a running `apps/web` instance via Chrome DevTools Protocol (CDP)
//! to verify the voice-channel connect/disconnect flow end-to-end at the
//! DOM layer:
//!
//! 1. Connects to Chromium's CDP endpoint and finds the page target.
//! 2. Navigates to a test-stoat voice channel route.
//! 3. Clicks the `.btn-voice-join` button.
//! 4. Asserts the `.voice-banner` element appears within a deadline.
//! 5. Asserts the participant avatars container (`.voice-banner-avatars`)
//!    has at least one child (the local user).
//! 6. Clicks the disconnect button (`.voice-ctrl-btn.disconnect`).
//! 7. Asserts the `.voice-banner` element disappears.
//!
//! # Why a CDP client and not Playwright
//!
//! The repo already drives Chromium via raw CDP in
//! `mcp/web-devtools-mcp/src/main.rs` using `tokio-tungstenite`. Adding a
//! TypeScript/Playwright project would pull in a Node toolchain and a
//! separate test runner. A small Rust binary mirrors the existing patterns,
//! ships as a workspace member (`cargo build` + `cargo run -p
//! poly-voice-ui-smoke`), and reuses the same WebSocket plumbing.
//!
//! # Prerequisites (the test is skip-by-default)
//!
//! Without `RUN_VOICE_UI_SMOKE=1` the binary exits 0 immediately — this is
//! the compile-only path that runs in CI and in TEST_HARNESS step N.
//!
//! With `RUN_VOICE_UI_SMOKE=1` the binary expects:
//!
//! - `dx serve` is running for `apps/web` on `127.0.0.1:3000` (the
//!   fullstack default for that crate; see CLAUDE.md "Running apps/web
//!   with persistent storage").
//! - A Chromium instance is open with
//!   `--remote-debugging-port=9222` and has the app loaded (poly-web MCP's
//!   `launch_app` does this; alternately launch by hand:
//!   `chromium --remote-debugging-port=9222 \
//!    --user-data-dir=/tmp/poly-voice-ui-smoke-profile \
//!    --use-fake-ui-for-media-stream --use-fake-device-for-media-stream \
//!    http://127.0.0.1:3000`).
//! - The signed-in account routes to a test-stoat backend with the
//!   `CHVOICE001` voice channel seeded (the default test-stoat fixture).
//!
//! The expected channel URL is supplied via `POLY_VOICE_UI_URL` (e.g.
//! `http://127.0.0.1:3000/accounts/<acct>/channels/CHVOICE001`). If not
//! set, the smoke skips with a SKIP exit (0) and a message — the harness
//! doesn't try to navigate the side panel by clicking, because account
//! IDs are user-specific.
//!
//! # Usage
//!
//! ```bash
//! RUN_VOICE_UI_SMOKE=1 \
//!   POLY_VOICE_UI_URL="http://127.0.0.1:3000/.../CHVOICE001" \
//!   cargo run -p poly-voice-ui-smoke
//! ```
//!
//! # CI
//!
//! Compile-only by default — fits the same pattern as
//! `tools/discord-voice-smoke/` and `tools/stoat-voice-smoke/`.

// arithmetic is on small fixed in-test values. See feedback_test_lints.
// lint-allow-unused: smoke-test binary; unwrap/expect/panic/arithmetic are fine here
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use anyhow::{anyhow, bail, Context as _};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_CDP_PORT: u16 = 9222;
const CDP_SEND_TIMEOUT_SECS: u64 = 5;
const CDP_RESPONSE_TIMEOUT_SECS: u64 = 15;
const ASSERT_DEADLINE_SECS: u64 = 12;
const POLL_INTERVAL_MS: u64 = 200;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "voice_ui_smoke=info".into()),
        )
        .init();

    if std::env::var("RUN_VOICE_UI_SMOKE").unwrap_or_default() != "1" {
        tracing::info!(
            "RUN_VOICE_UI_SMOKE != 1 — skipping (compile-only check passed)"
        );
        return Ok(());
    }

    let Some(url) = std::env::var("POLY_VOICE_UI_URL").ok() else {
        tracing::warn!(
            "POLY_VOICE_UI_URL not set — cannot navigate to a voice channel. \
             SKIP. Set POLY_VOICE_UI_URL=http://127.0.0.1:3000/.../CHVOICE001 to run."
        );
        return Ok(());
    };

    let cdp_port: u16 = std::env::var("POLY_CDP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CDP_PORT);

    run(&url, cdp_port).await
}

async fn run(target_url: &str, cdp_port: u16) -> anyhow::Result<()> {
    tracing::info!("Connecting to CDP on 127.0.0.1:{cdp_port}");
    let mut cdp = CdpClient::connect(cdp_port).await?;

    tracing::info!("Navigating to {target_url}");
    cdp.navigate(target_url).await?;
    // Give the page a beat to render the channel view.
    sleep(Duration::from_millis(800)).await;

    tracing::info!("Asserting connect button (.btn-voice-join) is present");
    cdp.wait_for_selector(".btn-voice-join", ASSERT_DEADLINE_SECS)
        .await
        .context("voice connect button (.btn-voice-join) not found on page")?;

    tracing::info!("Clicking .btn-voice-join");
    cdp.click(".btn-voice-join")
        .await
        .context("failed to click .btn-voice-join")?;

    tracing::info!("Asserting .voice-banner appears");
    cdp.wait_for_selector(".voice-banner", ASSERT_DEADLINE_SECS)
        .await
        .context("voice banner did not appear after clicking connect")?;

    tracing::info!("Asserting .voice-banner-avatars has >= 1 child (local user)");
    cdp.wait_for_predicate(
        "document.querySelectorAll('.voice-banner-avatars .voice-banner-avatar').length >= 1",
        ASSERT_DEADLINE_SECS,
    )
    .await
    .context("voice banner avatars stayed empty — local user not shown")?;

    tracing::info!("Clicking disconnect button (.voice-ctrl-btn.disconnect)");
    cdp.click(".voice-ctrl-btn.disconnect")
        .await
        .context("failed to click disconnect")?;

    tracing::info!("Asserting .voice-banner disappears");
    cdp.wait_for_absent(".voice-banner", ASSERT_DEADLINE_SECS)
        .await
        .context("voice banner stayed on screen after clicking disconnect")?;

    tracing::info!("K.4 PASS — voice banner appears on connect, clears on disconnect");
    Ok(())
}

// ─── Minimal CDP client ────────────────────────────────────────────────────

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

struct CdpClient {
    ws: WsStream,
    next_id: AtomicU64,
}

impl CdpClient {
    async fn connect(port: u16) -> anyhow::Result<Self> {
        let ws_url = discover_ws_url(port).await?;
        tracing::info!("Connecting WebSocket: {ws_url}");
        let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .with_context(|| format!("CDP WS connect to {ws_url}"))?;
        Ok(Self {
            ws,
            next_id: AtomicU64::new(1),
        })
    }

    async fn send(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = json!({ "id": id, "method": method, "params": params });
        let txt = serde_json::to_string(&msg)?;

        tokio::time::timeout(
            Duration::from_secs(CDP_SEND_TIMEOUT_SECS),
            self.ws.send(Message::Text(txt.into())),
        )
        .await
        .map_err(|_| anyhow!("CDP send timeout for {method}"))??;

        let deadline = Instant::now() + Duration::from_secs(CDP_RESPONSE_TIMEOUT_SECS);
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| anyhow!("CDP response timeout for {method}"))?;
            let next = tokio::time::timeout(remaining, self.ws.next())
                .await
                .map_err(|_| anyhow!("CDP response timeout for {method}"))?;
            let Some(Ok(Message::Text(t))) = next else {
                bail!("CDP WS closed while waiting for {method}");
            };
            let v: Value = serde_json::from_str(&t)?;
            if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    bail!("CDP error on {method}: {err}");
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
            // ignored: events / other responses
        }
    }

    async fn navigate(&mut self, url: &str) -> anyhow::Result<()> {
        self.send("Page.enable", json!({})).await?;
        self.send("Page.navigate", json!({ "url": url })).await?;
        Ok(())
    }

    /// Evaluate a JS expression and return the result value (or null).
    async fn eval(&mut self, expr: &str) -> anyhow::Result<Value> {
        let res = self
            .send(
                "Runtime.evaluate",
                json!({
                    "expression": expr,
                    "returnByValue": true,
                    "awaitPromise": false,
                }),
            )
            .await?;
        if let Some(ex) = res.get("exceptionDetails") {
            bail!("JS exception in eval `{expr}`: {ex}");
        }
        Ok(res
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn wait_for_predicate(
        &mut self,
        js_bool_expr: &str,
        deadline_secs: u64,
    ) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(deadline_secs);
        loop {
            let v = self.eval(&format!("Boolean({js_bool_expr})")).await?;
            if v.as_bool() == Some(true) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!(
                    "predicate did not become true within {deadline_secs}s: `{js_bool_expr}`"
                );
            }
            sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }

    async fn wait_for_selector(
        &mut self,
        css: &str,
        deadline_secs: u64,
    ) -> anyhow::Result<()> {
        self.wait_for_predicate(
            &format!("document.querySelector({})", js_string(css)),
            deadline_secs,
        )
        .await
    }

    async fn wait_for_absent(
        &mut self,
        css: &str,
        deadline_secs: u64,
    ) -> anyhow::Result<()> {
        self.wait_for_predicate(
            &format!("!document.querySelector({})", js_string(css)),
            deadline_secs,
        )
        .await
    }

    async fn click(&mut self, css: &str) -> anyhow::Result<()> {
        let expr = format!(
            "(() => {{ const el = document.querySelector({sel}); \
              if (!el) return 'missing'; el.click(); return 'ok'; }})()",
            sel = js_string(css)
        );
        let v = self.eval(&expr).await?;
        match v.as_str() {
            Some("ok") => Ok(()),
            Some("missing") => bail!("click target not in DOM: {css}"),
            other => bail!("unexpected click result for {css}: {other:?}"),
        }
    }
}

/// JSON-encode `s` so it's safe to embed as a JS string literal.
fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

async fn discover_ws_url(port: u16) -> anyhow::Result<String> {
    let url = format!("http://127.0.0.1:{port}/json");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map_err(|e| {
            anyhow!(
                "Cannot reach Chromium CDP at {url}: {e}. \
                 Start Chromium with --remote-debugging-port={port}"
            )
        })?;
    let targets: Vec<Value> = resp.json().await?;
    // Prefer a page target whose URL is on the local app server.
    let app_prefix = "http://127.0.0.1:";
    for t in &targets {
        if t.get("type").and_then(|v| v.as_str()) == Some("page") {
            let tu = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if tu.starts_with(app_prefix)
                && let Some(ws) = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
            {
                return Ok(ws.to_string());
            }
        }
    }
    // Fall back to any page target.
    for t in &targets {
        if t.get("type").and_then(|v| v.as_str()) == Some("page")
            && let Some(ws) = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str())
        {
            return Ok(ws.to_string());
        }
    }
    bail!(
        "no page target found in CDP /json (port {port}). Targets: {}",
        serde_json::to_string_pretty(&targets)?
    )
}
