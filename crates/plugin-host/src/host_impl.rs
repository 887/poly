//! Host-side implementation of the `host-api` WIT interface.
//!
//! Each plugin instance gets its own [`PluginHostState`] which holds its
//! scoped storage, WebSocket handles, and configuration. The host functions
//! are implemented as trait impls on this state struct.
//!
//! All I/O is mediated through the host — plugins have zero direct access
//! to the network, filesystem, or system clock.

use std::collections::HashMap;
use wasmtime::component::ResourceTable;

use super::engine::poly::messenger::host_api;
use super::engine::poly::messenger::types;

use poly_client::ClientEvent;

/// Deterministic mocked HTTP response used by plugin-host tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockHttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Raw response body bytes.
    pub body: Vec<u8>,
}

/// Per-plugin instance state stored in the wasmtime [`Store`].
///
/// Each plugin gets its own isolated state with separate storage namespace,
/// WebSocket handles, and resource limits.
pub struct PluginHostState {
    /// Plugin identifier (e.g., "stoat", "matrix", "demo").
    pub plugin_id: String,

    /// Plugin-scoped key-value storage.
    ///
    /// Keys are automatically namespaced per-plugin so plugins cannot
    /// read each other's data.
    pub storage: HashMap<String, Vec<u8>>,

    /// Active WebSocket connection handles.
    ///
    /// Each `websocket_connect` call returns an incrementing handle ID.
    /// Handles are cleaned up on plugin unload.
    pub ws_handles: HashMap<u64, WebSocketHandle>,

    /// Next WebSocket handle ID to assign.
    pub next_ws_handle: u64,

    /// Resource table for WASI (required by wasmtime-wasi).
    pub resource_table: ResourceTable,

    /// WASI context for the plugin.
    pub wasi_ctx: wasmtime_wasi::WasiCtx,

    /// Remaining fuel for this invocation (prevents infinite loops).
    pub fuel_limit: u64,

    /// Channel for events emitted by the plugin via `emit-event`.
    ///
    /// The host's `event_stream()` implementation creates the receiver side.
    /// When the guest calls `emit-event(event)`, the host converts it and
    /// sends it through this channel.
    pub event_tx: Option<tokio::sync::mpsc::Sender<ClientEvent>>,

    /// Sender for inbound WebSocket data.
    ///
    /// Each WS read task sends `WsInboundData` here. The host's event loop
    /// receives it, calls `handle-ws-data` on the guest, and the guest
    /// calls `emit-event` to push parsed events.
    pub ws_inbound_tx: tokio::sync::mpsc::Sender<WsInboundData>,

    /// Receiver for inbound WebSocket data.
    ///
    /// Consumed by the host's event loop in `event_stream()`.
    pub ws_inbound_rx: Option<tokio::sync::mpsc::Receiver<WsInboundData>>,

    /// Optional deterministic HTTP fixtures keyed by `(method, url)`.
    ///
    /// When present, `http_request` returns these fixtures instead of touching
    /// the real network. This is used by plugin-host tests to exercise real
    /// guest logic through the WASM path without depending on external services.
    pub mock_http_responses: HashMap<(String, String), Result<MockHttpResponse, String>>,
}

/// Represents an active WebSocket connection managed by the host.
pub struct WebSocketHandle {
    /// Sender half for writing to the WebSocket.
    pub tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    /// Whether the connection is still alive.
    pub alive: bool,
}

/// Inbound WebSocket data waiting to be forwarded to the guest.
///
/// The host's event loop reads from this and calls `handle-ws-data`
/// on the guest, which then calls `emit-event` to push parsed events.
#[derive(Debug)]
pub struct WsInboundData {
    /// Which WebSocket handle this data came from.
    pub handle: u64,
    /// Raw bytes received from the WebSocket.
    pub data: Vec<u8>,
}

impl PluginHostState {
    /// Create a new host state for a plugin instance.
    pub fn new(plugin_id: &str) -> Self {
        let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new().build();
        let (ws_inbound_tx, ws_inbound_rx) = tokio::sync::mpsc::channel(256);
        Self {
            plugin_id: plugin_id.to_string(),
            storage: HashMap::new(),
            ws_handles: HashMap::new(),
            next_ws_handle: 1,
            resource_table: ResourceTable::new(),
            wasi_ctx,
            fuel_limit: 1_000_000_000, // 1 billion fuel units per invocation
            event_tx: None,
            ws_inbound_tx,
            ws_inbound_rx: Some(ws_inbound_rx),
            mock_http_responses: HashMap::new(),
        }
    }

    /// Register a deterministic mocked HTTP response for plugin tests.
    #[must_use]
    pub fn with_mock_http_response(
        mut self,
        method: impl Into<String>,
        url: impl Into<String>,
        response: Result<MockHttpResponse, String>,
    ) -> Self {
        self.mock_http_responses
            .insert((method.into(), url.into()), response);
        self
    }
}

// ─── WASI trait implementations (required by wasmtime-wasi) ────────

impl wasmtime_wasi::WasiView for PluginHostState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.resource_table,
        }
    }
}

// ─── Host API trait implementation ─────────────────────────────────

// The `types` interface defines only data types (no functions), but wasmtime's
// bindgen still generates an empty `Host` trait for it.
impl types::Host for PluginHostState {}

impl host_api::Host for PluginHostState {
    /// Make an HTTP request on behalf of the plugin.
    ///
    /// Uses reqwest under the hood. The host can add URL allowlisting here.
    async fn http_request(
        &mut self,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Result<types::HttpResponse, String> {
        if let Some(mock) = self
            .mock_http_responses
            .get(&(method.clone(), url.clone()))
            .cloned()
        {
            return mock.map(|response| types::HttpResponse {
                status: response.status,
                headers: response.headers,
                body: response.body,
            });
        }

        let client = reqwest::Client::new();

        let req_method = method
            .parse::<reqwest::Method>()
            .map_err(|e| e.to_string())?;

        let mut builder = client.request(req_method, &url);

        for (key, value) in headers {
            builder = builder.header(key, value);
        }

        if let Some(body_bytes) = body {
            builder = builder.body(body_bytes);
        }

        let response = builder.send().await.map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let resp_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let resp_body = response.bytes().await.map_err(|e| e.to_string())?;

        Ok(types::HttpResponse {
            status,
            headers: resp_headers,
            body: resp_body.to_vec(),
        })
    }

    /// Open a WebSocket connection.
    ///
    /// Spawns a background task that manages the connection. Inbound data
    /// is routed to `ws_inbound_tx` — the host's event loop picks it up
    /// and calls `handle-ws-data` on the guest, which then calls `emit-event`.
    async fn websocket_connect(
        &mut self,
        url: String,
        headers: Vec<(String, String)>,
    ) -> Result<u64, String> {
        use tokio_tungstenite::connect_async;

        let handle_id = self.next_ws_handle;
        self.next_ws_handle += 1;

        // Build the request with custom headers
        // TODO(phase-2.14.3): implement full WS with custom headers
        let _ = &headers;

        // Spawn the actual WebSocket handler
        let plugin_id = self.plugin_id.clone();
        let (ws_tx, mut ws_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
        let inbound_tx = self.ws_inbound_tx.clone();

        tokio::spawn(async move {
            match connect_async(&url).await {
                Ok((ws_stream, _)) => {
                    use futures_util::{SinkExt, StreamExt};
                    let (mut write, mut read) = ws_stream.split();

                    // Read loop: WS → inbound channel → host event loop → guest handle-ws-data
                    let read_task = tokio::spawn(async move {
                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                                    if inbound_tx
                                        .send(WsInboundData {
                                            handle: handle_id,
                                            data: data.to_vec(),
                                        })
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                    if inbound_tx
                                        .send(WsInboundData {
                                            handle: handle_id,
                                            data: text.as_bytes().to_vec(),
                                        })
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                                Ok(_) => {} // Ping/Pong/Close handled by tungstenite
                                Err(e) => {
                                    tracing::warn!(
                                        "WebSocket read error for plugin {plugin_id}: {e}"
                                    );
                                    break;
                                }
                            }
                        }
                    });

                    // Write loop: outbound channel → WS
                    while let Some(data) = ws_rx.recv().await {
                        let msg = tokio_tungstenite::tungstenite::Message::Binary(data.into());
                        if write.send(msg).await.is_err() {
                            break;
                        }
                    }

                    read_task.abort();
                }
                Err(e) => {
                    tracing::error!("WebSocket connect failed for plugin {plugin_id}: {e}");
                }
            }
        });

        self.ws_handles.insert(
            handle_id,
            WebSocketHandle {
                tx: ws_tx,
                alive: true,
            },
        );

        Ok(handle_id)
    }

    /// Send data on a WebSocket.
    async fn websocket_send(&mut self, handle: u64, data: Vec<u8>) -> Result<(), String> {
        let ws = self
            .ws_handles
            .get(&handle)
            .ok_or_else(|| format!("Invalid WebSocket handle: {handle}"))?;

        if !ws.alive {
            return Err("WebSocket is closed".to_string());
        }

        ws.tx
            .send(data)
            .await
            .map_err(|e| format!("WebSocket send failed: {e}"))
    }

    /// Close a WebSocket connection.
    async fn websocket_close(&mut self, handle: u64) -> Result<(), String> {
        if let Some(mut ws) = self.ws_handles.remove(&handle) {
            ws.alive = false;
            // Dropping the tx/rx will cause the background task to end
            drop(ws);
            Ok(())
        } else {
            Err(format!("Invalid WebSocket handle: {handle}"))
        }
    }

    /// Receive an event emitted by the guest plugin.
    ///
    /// The guest calls this (via `emit-event` host import) when it has
    /// parsed WebSocket/HTTP data into a structured event. The host
    /// converts it and forwards to the `event_stream()` consumer.
    async fn emit_event(&mut self, event: types::ClientEvent) {
        if let Some(tx) = &self.event_tx {
            let client_event = super::bridge::from_wit_client_event(event);
            if let Err(e) = tx.send(client_event).await {
                tracing::warn!(
                    plugin = %self.plugin_id,
                    "Failed to forward emitted event: {e}"
                );
            }
        } else {
            tracing::debug!(
                plugin = %self.plugin_id,
                "Event emitted but no event_tx configured (event_stream not called?)"
            );
        }
    }

    /// Read from plugin-scoped key-value storage.
    async fn storage_get(&mut self, key: String) -> Option<Vec<u8>> {
        self.storage.get(&key).cloned()
    }

    /// Write to plugin-scoped key-value storage.
    async fn storage_set(&mut self, key: String, value: Vec<u8>) -> Result<(), String> {
        self.storage.insert(key, value);
        Ok(())
    }

    /// Delete from plugin-scoped key-value storage.
    async fn storage_delete(&mut self, key: String) -> Result<(), String> {
        self.storage.remove(&key);
        Ok(())
    }

    /// Log a message through the host's tracing system.
    async fn log(&mut self, level: types::LogLevel, message: String) {
        let plugin_id = &self.plugin_id;
        match level {
            types::LogLevel::Trace => tracing::trace!(plugin = %plugin_id, "{message}"),
            types::LogLevel::Debug => tracing::debug!(plugin = %plugin_id, "{message}"),
            types::LogLevel::Info => tracing::info!(plugin = %plugin_id, "{message}"),
            types::LogLevel::Warn => tracing::warn!(plugin = %plugin_id, "{message}"),
            types::LogLevel::Error => tracing::error!(plugin = %plugin_id, "{message}"),
        }
    }

    /// Get current wall-clock time as RFC3339 string.
    async fn get_current_time(&mut self) -> String {
        chrono::Utc::now().to_rfc3339()
    }
}
