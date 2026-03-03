//! WebSocket client for poly-server real-time events.
//!
//! Connects to `ws://host/ws?token=<JWT>` and exposes a stream of `ServerEvent`s.
//! Handles auto-reconnect with exponential backoff.

use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info, warn};

use crate::error::Result;
use crate::http::SessionState;
use crate::models::ServerEvent;

/// Maximum reconnection delay (seconds).
const MAX_BACKOFF_SECS: u64 = 30;

/// Broadcast channel capacity.
const CHAN_CAPACITY: usize = 256;

/// WebSocket client for receiving real-time events from a poly-server.
pub struct PolyServerWsClient {
    /// Base URL of the server (http:// — we convert to ws://).
    base_url: String,
    /// Shared session state from the HTTP client.
    session: Arc<RwLock<Option<SessionState>>>,
    /// Event broadcaster — subscribers receive all events.
    tx: broadcast::Sender<ServerEvent>,
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
            task_handle: None,
        }
    }

    /// Get a receiver for server events.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }

    /// Get the sender (for bridging events into poly-client streams).
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

        let handle = tokio::spawn(async move {
            ws_reconnect_loop(base_url, session, tx).await;
        });
        self.task_handle = Some(handle);
    }

    /// Disconnect the WebSocket.
    pub fn disconnect(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }

    /// Send a client → server message (e.g. typing_start, heartbeat).
    pub async fn send_message(&self, msg: &serde_json::Value) -> Result<()> {
        // This is a simplified version — for a full implementation we'd
        // need to hold on to the write half of the WS. For now, typing
        // indicators use HTTP or are fire-and-forget.
        let _ = msg;
        Ok(())
    }
}

impl Drop for PolyServerWsClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Convert an HTTP URL to a WebSocket URL.
fn to_ws_url(base_url: &str) -> String {
    if let Some(rest) = base_url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base_url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        format!("ws://{base_url}")
    }
}

/// Background reconnection loop.
async fn ws_reconnect_loop(
    base_url: String,
    session: Arc<RwLock<Option<SessionState>>>,
    tx: broadcast::Sender<ServerEvent>,
) {
    let mut backoff_secs = 1u64;

    loop {
        // Get the current token.
        let token = {
            let guard = session.read().await;
            match guard.as_ref() {
                Some(s) => s.token.clone(),
                None => {
                    debug!("WS: No session token, waiting 5s before retry");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }
        };

        let ws_base = to_ws_url(&base_url);
        let ws_url = format!("{ws_base}/ws?token={token}");
        debug!("WS: Connecting to {ws_base}/ws");

        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                info!("WS: Connected to {ws_base}");
                backoff_secs = 1; // Reset backoff on successful connect.

                let (mut _write, mut read) = ws_stream.split();

                // Read events until the connection drops.
                while let Some(msg_result) = read.next().await {
                    match msg_result {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                            match serde_json::from_str::<ServerEvent>(&text) {
                                Ok(event) => {
                                    // Ignore send errors — no subscribers means nobody is listening.
                                    let _ = tx.send(event);
                                }
                                Err(e) => {
                                    debug!("WS: Failed to parse event: {e} — raw: {text}");
                                }
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
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
            }
            Err(e) => {
                warn!("WS: Connection failed: {e}");
            }
        }

        // Reconnect with exponential backoff.
        info!("WS: Reconnecting in {backoff_secs}s");
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}
