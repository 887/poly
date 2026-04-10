//! # poly-host-bridge
//!
//! Host-API bridge for the **dioxus WASM target**.
//!
//! ## Why this crate exists
//!
//! Poly's WIT [`host-api`](../../../../wit/messenger-plugin.wit) defines the
//! syscall-like operations that messenger backends need: `exec-command`,
//! `http-request`, `websocket-*`, `storage-*`, `log`. WASM components running
//! inside `wasmtime` get these for free via [`crates/plugin-host`]'s
//! [`host_impl`](../../plugin-host/src/host_impl.rs).
//!
//! But Poly also ships as a **dioxus WASM** app loaded inside thin native
//! shells (Wry on desktop-web, Electron on desktop-electron-web, WKWebView on
//! iOS, Android WebView on android). That WASM target is *not* a wasm
//! component — it cannot import WIT functions, and the browser sandbox
//! forbids subprocess / unrestricted FS / arbitrary sockets. To give it the
//! same capability surface, each native shell binds a small HTTP endpoint
//! ([`BRIDGE_PATH`]) on [`BRIDGE_PORT`] that speaks the JSON protocol defined
//! in this crate. WASM code calls the bridge through [`Client`].
//!
//! The protocol mirrors the WIT host-api one-to-one. New operations are
//! added here whenever the host-api gains new functions, so the same client
//! code works no matter which side of the boundary it lives on.
//!
//! ## Per-shell support
//!
//! | Shell                                 | Bridge implementation              | Status |
//! |---------------------------------------|------------------------------------|--------|
//! | `apps/desktop-web` (Wry)              | Rust [`dispatch`] in axum          | ✅     |
//! | `apps/desktop-electron-web` (Electron)| Node `http` + `child_process`      | ✅     |
//! | `apps/web` (browser)                  | none — no native side              | n/a    |
//! | iOS (WKWebView shell)                 | future — needs native shell crate  | ⏳     |
//! | Android (WebView shell)               | future — needs subprocess (Termux/terminal) | ⏳ |
//!
//! Until iOS / Android shells expose the bridge, [`Client::call`] returns
//! [`BridgeError::Unreachable`] on those targets and callers should degrade
//! gracefully (e.g. show "this backend needs the desktop shell").

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod http;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Loopback port that every native shell binds for the host bridge.
///
/// Distinct from the dev-MCP eval bridge ports (9222 / 9223 / 9224) so the
/// runtime host bridge is unaffected by whether dev tooling is loaded.
pub const BRIDGE_PORT: u16 = 9333;

/// HTTP path of the host bridge endpoint.
pub const BRIDGE_PATH: &str = "/host";

/// Full default URL of the host bridge.
pub const BRIDGE_URL: &str = "http://127.0.0.1:9333/host";

// ─── Protocol — request side ─────────────────────────────────────────────────

/// One host-api call. Tagged-union JSON: `{"call": "<kebab-case>", ...fields}`.
///
/// Mirrors the operations defined in `wit/messenger-plugin.wit` under
/// `interface host-api`. New variants land here whenever WIT gains a new
/// host function.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "call", rename_all = "kebab-case")]
pub enum HostCall {
    /// Spawn a subprocess and wait for it to exit.
    ///
    /// `program` and `args` go straight to the OS exec — no shell — so argv
    /// metacharacters (`&&`, `|`, `$`, backticks) stay inert.
    ExecCommand {
        program: String,
        args: Vec<String>,
    },
    /// Make a one-shot HTTP request via the host's network stack.
    HttpRequest {
        method: String,
        url: String,
        #[serde(default)]
        headers: Vec<(String, String)>,
        /// Base64-encoded request body, or `None` for an empty body.
        #[serde(default)]
        body_b64: Option<String>,
    },
}

// ─── Protocol — response side ────────────────────────────────────────────────

/// Successful response payload, tagged by which call produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum HostOk {
    /// Result of [`HostCall::ExecCommand`].
    ExecOutput {
        exit_code: i32,
        /// Base64-encoded process stdout bytes.
        stdout_b64: String,
        /// Base64-encoded process stderr bytes.
        stderr_b64: String,
    },
    /// Result of [`HostCall::HttpRequest`].
    HttpResponse {
        status: u16,
        headers: Vec<(String, String)>,
        body_b64: String,
    },
}

/// Bridge response: either a typed [`HostOk`] or an error string.
///
/// Wire shape: `{"ok": {...}}` or `{"err": "..."}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostResponse {
    Ok(HostOk),
    Err(String),
}

// ─── Errors ──────────────────────────────────────────────────────────────────

/// Errors returned by [`Client::call`].
#[derive(Debug, Error)]
pub enum BridgeError {
    /// The native shell isn't running, or doesn't bind the bridge port on
    /// this platform yet (e.g. mobile shells without subprocess support).
    #[error("host bridge unreachable at {url}: {source}")]
    Unreachable {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    /// HTTP / transport-level error after the bridge accepted the request.
    #[error("host bridge transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// The bridge returned a JSON body we couldn't parse.
    #[error("host bridge response not valid JSON: {0}")]
    ParseResponse(String),
    /// The bridge returned an `Err` payload (the host operation itself failed).
    #[error("host operation failed: {0}")]
    Host(String),
    /// The response was the wrong variant for the call we made
    /// (e.g. we asked for `exec-command` and got an `http-response` back).
    #[error("host bridge returned mismatched variant for {call}: {got}")]
    VariantMismatch { call: &'static str, got: String },
}

// ─── Client ──────────────────────────────────────────────────────────────────

/// Typed client for the host bridge.
///
/// Compiles for both native and WASM (uses `reqwest`, which uses fetch on
/// `wasm32-unknown-unknown`). Cheap to construct — clone freely.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    url: String,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Build a client targeting the default loopback bridge URL.
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
            url: BRIDGE_URL.to_string(),
        }
    }

    /// Build a client targeting an explicit bridge URL — useful for tests
    /// or for shells that bind a non-default port.
    #[must_use]
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            url: url.into(),
        }
    }

    /// Send one [`HostCall`] and decode the response.
    ///
    /// Returns the typed [`HostOk`] payload on success, or [`BridgeError`]
    /// on transport / dispatch failure.
    pub async fn call(&self, call: HostCall) -> Result<HostOk, BridgeError> {
        let resp = self
            .http
            .post(&self.url)
            .json(&call)
            .send()
            .await
            .map_err(|e| BridgeError::Unreachable {
                url: self.url.clone(),
                source: e,
            })?;

        let body = resp.text().await?;
        let parsed: HostResponse =
            serde_json::from_str(&body).map_err(|e| BridgeError::ParseResponse(e.to_string()))?;
        match parsed {
            HostResponse::Ok(ok) => Ok(ok),
            HostResponse::Err(msg) => Err(BridgeError::Host(msg)),
        }
    }

    /// Convenience: run an [`HostCall::ExecCommand`] and decode the
    /// `ExecOutput` variant. Returns `(exit_code, stdout, stderr)`.
    pub async fn exec(
        &self,
        program: impl Into<String>,
        args: Vec<String>,
    ) -> Result<(i32, Vec<u8>, Vec<u8>), BridgeError> {
        let ok = self
            .call(HostCall::ExecCommand {
                program: program.into(),
                args,
            })
            .await?;
        match ok {
            HostOk::ExecOutput {
                exit_code,
                stdout_b64,
                stderr_b64,
            } => {
                let stdout = b64_decode(&stdout_b64)
                    .map_err(|e| BridgeError::ParseResponse(format!("stdout_b64: {e}")))?;
                let stderr = b64_decode(&stderr_b64)
                    .map_err(|e| BridgeError::ParseResponse(format!("stderr_b64: {e}")))?;
                Ok((exit_code, stdout, stderr))
            }
            other => Err(BridgeError::VariantMismatch {
                call: "exec-command",
                got: variant_name(&other).to_string(),
            }),
        }
    }
}

fn variant_name(ok: &HostOk) -> &'static str {
    match ok {
        HostOk::ExecOutput { .. } => "exec-output",
        HostOk::HttpResponse { .. } => "http-response",
    }
}

// ─── Server-side dispatcher (Rust shells only) ───────────────────────────────

/// Run a [`HostCall`] using the host's real OS capabilities and produce a
/// [`HostResponse`]. This is the function Rust shells (apps/desktop-web)
/// hand to their HTTP framework.
///
/// **Not available on WASM** — the dispatcher needs subprocess / network
/// access that wasm32-unknown-unknown doesn't have. WASM-side callers use
/// [`Client`] instead, which routes through whichever shell *is* native.
#[cfg(not(target_arch = "wasm32"))]
pub async fn dispatch(call: HostCall) -> HostResponse {
    match call {
        HostCall::ExecCommand { program, args } => exec_command(program, args).await,
        HostCall::HttpRequest {
            method,
            url,
            headers,
            body_b64,
        } => http_request(method, url, headers, body_b64).await,
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn exec_command(program: String, args: Vec<String>) -> HostResponse {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut cmd = Command::new(&program);
    cmd.args(&args);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    match cmd.output().await {
        Ok(output) => HostResponse::Ok(HostOk::ExecOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout_b64: b64_encode(&output.stdout),
            stderr_b64: b64_encode(&output.stderr),
        }),
        Err(e) => HostResponse::Err(format!("failed to spawn `{program}`: {e}")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn http_request(
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body_b64: Option<String>,
) -> HostResponse {
    let body = match body_b64.as_deref() {
        Some(b64) => match b64_decode(b64) {
            Ok(bytes) => Some(bytes),
            Err(e) => return HostResponse::Err(format!("invalid body_b64: {e}")),
        },
        None => None,
    };

    let method_parsed = match reqwest::Method::from_bytes(method.as_bytes()) {
        Ok(m) => m,
        Err(e) => return HostResponse::Err(format!("invalid method: {e}")),
    };

    let mut req = reqwest::Client::new().request(method_parsed, &url);
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    if let Some(body) = body {
        req = req.body(body);
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let resp_headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            match resp.bytes().await {
                Ok(bytes) => HostResponse::Ok(HostOk::HttpResponse {
                    status,
                    headers: resp_headers,
                    body_b64: b64_encode(&bytes),
                }),
                Err(e) => HostResponse::Err(format!("read body: {e}")),
            }
        }
        Err(e) => HostResponse::Err(format!("http request failed: {e}")),
    }
}

// ─── base64 helpers ──────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn b64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}
