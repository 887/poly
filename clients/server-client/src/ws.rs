//! WebSocket client for poly-server real-time events.
//!
//! Connects to `ws://host/ws?token=<JWT>` and exposes a stream of `ServerEvent`s.
//! Handles auto-reconnect with exponential backoff.

use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, info, warn};

use crate::error::Result;
use crate::http::SessionState;
use crate::models::ServerEvent;

/// Maximum reconnection delay (seconds).
const MAX_BACKOFF_SECS: u64 = 30;

/// Broadcast channel capacity.
const CHAN_CAPACITY: usize = 256;

/// Type alias for the WebSocket write half shared across tasks.
type WsSink = Arc<
    Mutex<
        Option<
            futures_util::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                WsMessage,
            >,
        >,
    >,
>;

/// WebSocket client for receiving real-time events from a poly-server.
pub struct PolyServerWsClient {
    /// Base URL of the server (http:// — we convert to ws://).
    base_url: String,
    /// Shared session state from the HTTP client.
    session: Arc<RwLock<Option<SessionState>>>,
    /// Event broadcaster — subscribers receive all events.
    tx: broadcast::Sender<ServerEvent>,
    /// Shared write half of the WebSocket — `None` when disconnected.
    sink: WsSink,
    /// Handle to the reconnect task (so we can abort on drop).
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl PolyServerWsClient {
    /// Create a new WebSocket client. Does NOT connect until `connect()` is called.
    pub fn new(base_url: &str, session: Arc<RwLock<Option<SessionState>>>) -> Self {
        let (tx, _) = broadcast::channel(CHAN_CAPACITY);
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            session,
            tx,
            sink: Arc::new(Mutex::new(None)),
            task_handle: None,
        }
    }

    /// Get a receiver for server events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }

    /// Get the sender (for bridging events into poly-client streams).
    #[must_use]
    pub fn sender(&self) -> broadcast::Sender<ServerEvent> {
        self.tx.clone()
    }

    /// Connect to the WebSocket and start receiving events.
    ///
    /// Spawns a background task that maintains the connection with
    /// auto-reconnect. Returns immediately.
    pub fn connect(&mut self) {
        if self.task_handle.is_some() {
            return; // Already connected.
        }
        let base_url = self.base_url.clone();
        let session = Arc::clone(&self.session);
        let tx = self.tx.clone();
        let sink = Arc::clone(&self.sink);

        let handle = tokio::spawn(async move {
            ws_reconnect_loop(base_url, session, tx, sink).await;
        });
        self.task_handle = Some(handle);
    }

    /// Disconnect the WebSocket.
    pub fn disconnect(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
        // Clear the sink so future send_message calls return gracefully.
        let sink = Arc::clone(&self.sink);
        tokio::spawn(async move {
            *sink.lock().await = None;
        });
    }

    /// Send a JSON message to the server (e.g. `typing_start`, heartbeat).
    ///
    /// Returns `Ok(())` if no connection is currently active (fire-and-forget).
    pub async fn send_message(&self, msg: &serde_json::Value) -> Result<()> {
        let text = serde_json::to_string(msg)?;
        let send_result = {
            let mut guard = self.sink.lock().await;
            if let Some(ref mut sink) = *guard {
                Some(
                    sink.send(WsMessage::Text(text.into()))
                        .await
                        .map_err(|e| crate::error::PolyServerError::WebSocket(e.to_string())),
                )
            } else {
                None
            }
        };
        if let Some(r) = send_result {
            r?;
        }
        Ok(())
    }

    /// Send a `typing_start` event for the given channel.
    pub async fn send_typing(&self, channel_id: &str) -> Result<()> {
        self.send_message(&serde_json::json!({
            "event": "typing_start",
            "channel_id": channel_id,
        }))
        .await
    }
}

impl Drop for PolyServerWsClient {
    fn drop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }
}

/// Convert an HTTP URL to a WebSocket URL.
fn to_ws_url(base_url: &str) -> String {
    base_url.strip_prefix("https://").map_or_else(
        || {
            base_url
                .strip_prefix("http://")
                .map_or_else(|| format!("ws://{base_url}"), |rest| format!("ws://{rest}"))
        },
        |rest| format!("wss://{rest}"),
    )
}

/// Background reconnection loop.
// Complexity inherent to connection lifecycle: connect → stream → reconnect with backoff.
#[allow(clippy::cognitive_complexity)]
async fn ws_reconnect_loop(
    base_url: String,
    session: Arc<RwLock<Option<SessionState>>>,
    tx: broadcast::Sender<ServerEvent>,
    sink: WsSink,
) {
    let mut backoff_secs = 1u64;

    loop {
        // Get the current token.
        let token = {
            let guard = session.read().await;
            if let Some(s) = guard.as_ref() {
                s.token.clone()
            } else {
                debug!("WS: No session token, waiting 5s before retry");
                drop(guard);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        let ws_base = to_ws_url(&base_url);
        let ws_url = format!("{ws_base}/ws?token={token}");
        debug!("WS: Connecting to {ws_base}/ws");

        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                info!("WS: Connected to {ws_base}");
                backoff_secs = 1; // Reset backoff on successful connect.

                let (write, mut read) = ws_stream.split();
                // Store the write half so callers can send messages.
                *sink.lock().await = Some(write);

                // Read events until the connection drops.
                while let Some(msg_result) = read.next().await {
                    match msg_result {
                        Ok(WsMessage::Text(text)) => {
                            match serde_json::from_str::<ServerEvent>(&text) {
                                Ok(event) => {
                                    // Ignore send errors — no subscribers means nobody is listening.
                                    drop(tx.send(event));
                                }
                                Err(e) => {
                                    debug!("WS: Failed to parse event: {e} — raw: {text}");
                                }
                            }
                        }
                        Ok(WsMessage::Close(_)) => {
                            info!("WS: Server closed connection");
                            break;
                        }
                        Err(e) => {
                            warn!("WS: Error reading message: {e}");
                            break;
                        }
                        _ => {} // Ignore binary, ping, pong frames.
                    }
                }

                // Clear the write half on disconnect.
                *sink.lock().await = None;
            }
            Err(e) => {
                warn!("WS: Connection failed: {e}");
            }
        }

        // Reconnect with exponential backoff.
        info!("WS: Reconnecting in {backoff_secs}s");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = backoff_secs.saturating_mul(2).min(MAX_BACKOFF_SECS);
    }
}
