//! Electron sandbox implementation — `host-cap::sandbox-browser`.
//!
//! `ElectronSandbox` drives the Electron main-process `open-sandbox` IPC
//! handler by evaluating a JavaScript call through the Chromium DevTools
//! Protocol (CDP) WebSocket on port 9224.
//!
//! The call path is:
//!   Rust (native server, port 3001)
//!   → CDP `Runtime.evaluate` (WebSocket, port 9224, `awaitPromise: true`)
//!   → renderer JS: `window.polyElectron.openSandbox(opts)`
//!   → preload bridge: `ipcRenderer.invoke('open-sandbox', opts)`
//!   → Electron main process: `ipcMain.handle('open-sandbox', ...)`
//!   → resolves with `{ capturedUrl: string }` or rejects `'UserCancelled'`
//!
//! The CDP target is discovered via the HTTP JSON endpoint at
//! `http://127.0.0.1:9224/json` and the first page target's WebSocket URL
//! is used.

#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
pub(crate) mod impl_native {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use poly_host_sandbox::{HostSandbox, SandboxError, SandboxResult};
    use serde_json::{Value, json};
    use tokio_tungstenite::{
        connect_async,
        tungstenite::Message,
    };

    /// CDP port for the Electron shell — 9224 by default, overridable via env.
    const DEFAULT_CDP_PORT: u16 = 9224;

    /// Timeout for the entire sandbox operation (including IPC round-trip).
    const SANDBOX_TIMEOUT: Duration = Duration::from_secs(300);

    /// Timeout for the initial CDP target discovery + WebSocket connect.
    const CDP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

    /// Drives the Electron `open-sandbox` IPC handler via CDP `Runtime.evaluate`.
    pub struct ElectronSandbox {
        cdp_port: u16,
    }

    impl ElectronSandbox {
        #[must_use]
        pub fn new() -> Self {
            let port = std::env::var("POLY_ELECTRON_REMOTE_DEBUGGING_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_CDP_PORT);
            Self { cdp_port: port }
        }

        /// Discover the first page CDP WebSocket debugger URL via the HTTP JSON
        /// endpoint at `http://127.0.0.1:{port}/json`.
        async fn discover_ws_url(&self) -> Result<String, SandboxError> {
            let url = format!("http://127.0.0.1:{}/json", self.cdp_port);
            let client = reqwest::Client::new();
            let resp = tokio::time::timeout(CDP_CONNECT_TIMEOUT, client.get(&url).send())
                .await
                .map_err(|_| SandboxError::CdpError("CDP discovery timed out".into()))?
                .map_err(|e| SandboxError::CdpError(format!("CDP discovery failed: {e}")))?;
            let targets: Value = resp
                .json()
                .await
                .map_err(|e| SandboxError::CdpError(format!("CDP JSON parse error: {e}")))?;
            let ws_url = targets
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|t| t.get("webSocketDebuggerUrl"))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| SandboxError::CdpError("No CDP page target found".into()))?;
            Ok(ws_url)
        }
    }

    impl Default for ElectronSandbox {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl HostSandbox for ElectronSandbox {
        async fn open_browser_sandbox(
            &self,
            url: String,
            capture_url_pattern: String,
        ) -> Result<SandboxResult, SandboxError> {
            // Discover CDP target.
            let ws_url = self.discover_ws_url().await?;

            // Connect to the CDP WebSocket.
            let (mut ws, _) =
                tokio::time::timeout(CDP_CONNECT_TIMEOUT, connect_async(&ws_url))
                    .await
                    .map_err(|_| SandboxError::CdpError("CDP WebSocket connect timed out".into()))?
                    .map_err(|e| {
                        SandboxError::CdpError(format!("CDP WebSocket connect failed: {e}"))
                    })?;

            // Unique message ID for matching the response.
            static COUNTER: AtomicU64 = AtomicU64::new(1);
            let msg_id = COUNTER.fetch_add(1, Ordering::Relaxed);

            // Build the sandbox opts JSON for the JS side.
            let sandbox_id = uuid::Uuid::new_v4().to_string();
            // Escape the pattern for embedding in a JS string literal.
            let escaped_pattern = capture_url_pattern
                .replace('\\', r"\\")
                .replace('"', r#"\""#);
            let escaped_url = url.replace('\\', r"\\").replace('"', r#"\""#);
            let escaped_id = sandbox_id.replace('"', r#"\""#);

            let js = format!(
                r#"window.polyElectron.openSandbox({{
  id: "{escaped_id}",
  url: "{escaped_url}",
  capturePattern: "{escaped_pattern}"
}}).then(function(r) {{ return JSON.stringify(r); }})"#
            );

            let request = json!({
                "id": msg_id,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": js,
                    "returnByValue": true,
                    "awaitPromise": true,
                    "timeout": SANDBOX_TIMEOUT.as_millis() as u64,
                }
            });

            ws.send(Message::Text(request.to_string().into()))
                .await
                .map_err(|e| SandboxError::CdpError(format!("CDP send failed: {e}")))?;

            // Read responses until we get the one matching our message ID.
            let result =
                tokio::time::timeout(SANDBOX_TIMEOUT + Duration::from_secs(5), async {
                    loop {
                        let msg = ws
                            .next()
                            .await
                            .ok_or_else(|| {
                                SandboxError::CdpError("CDP connection closed".into())
                            })?
                            .map_err(|e| {
                                SandboxError::CdpError(format!("CDP recv error: {e}"))
                            })?;

                        let text = match msg {
                            Message::Text(t) => t.to_string(),
                            _ => continue,
                        };

                        let v: Value = serde_json::from_str(&text).map_err(|e| {
                            SandboxError::CdpError(format!("CDP parse error: {e}"))
                        })?;

                        if v.get("id").and_then(|id| id.as_u64()) != Some(msg_id) {
                            continue;
                        }

                        // Check for CDP-level error.
                        if let Some(err) = v.get("error") {
                            let msg = err
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("CDP error");
                            return Err(SandboxError::CdpError(msg.into()));
                        }

                        // Check for JS exception.
                        if let Some(exc) = v
                            .get("result")
                            .and_then(|r| r.get("exceptionDetails"))
                        {
                            let msg = exc
                                .get("exception")
                                .and_then(|e| e.get("description"))
                                .and_then(|d| d.as_str())
                                .unwrap_or("JS exception");

                            if msg.contains("UserCancelled") {
                                return Err(SandboxError::UserCancelled);
                            }
                            return Err(SandboxError::CdpError(format!(
                                "JS exception: {msg}"
                            )));
                        }

                        // Extract the returned JSON string.
                        let value_str = v
                            .get("result")
                            .and_then(|r| r.get("result"))
                            .and_then(|r| r.get("value"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        // Parse the `{ capturedUrl: "..." }` result.
                        let result_obj: Value =
                            serde_json::from_str(value_str).map_err(|e| {
                                SandboxError::CdpError(format!(
                                    "Sandbox result parse error: {e} (raw: {value_str})"
                                ))
                            })?;

                        let captured_url = result_obj
                            .get("capturedUrl")
                            .and_then(|u| u.as_str())
                            .ok_or_else(|| {
                                SandboxError::CdpError(
                                    "Missing capturedUrl in sandbox result".into(),
                                )
                            })?
                            .to_owned();

                        return Ok(SandboxResult { captured_url });
                    }
                })
                .await
                .map_err(|_| {
                    SandboxError::CdpError("Sandbox operation timed out".into())
                })??;

            Ok(result)
        }
    }
}

// Re-export for the server entry point.
#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
pub use impl_native::ElectronSandbox;

/// Host capabilities advertised by the Electron shell.
///
/// The Electron shell supports the sandbox-browser capability (via CDP →
/// `ipcMain.handle('open-sandbox', ...)`).
#[cfg(all(not(target_arch = "wasm32"), feature = "server"))]
#[must_use]
pub fn advertised_host_caps() -> Vec<poly_host_sandbox::HostCap> {
    vec![poly_host_sandbox::HostCap::SandboxBrowser]
}
