use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, warn};

use crate::AppState;
use crate::auth::Claims;

pub mod events;
pub use events::ServerEvent;

/// Per-user broadcast channel capacity.
const CHAN_CAPACITY: usize = 256;

/// Global WebSocket state: maps `user_id` → broadcast sender.
///
/// Each user gets one `broadcast::Sender`; every connected device for that user
/// holds a `broadcast::Receiver` clone. Sending to the `Sender` fans out to all
/// devices simultaneously.
pub struct WsState {
    channels: RwLock<HashMap<String, broadcast::Sender<ServerEvent>>>,
}

impl WsState {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a broadcast sender for a user.
    async fn sender_for(&self, user_id: &str) -> broadcast::Sender<ServerEvent> {
        {
            let guard = self.channels.read().await;
            if let Some(tx) = guard.get(user_id) {
                return tx.clone();
            }
        }
        let mut guard = self.channels.write().await;
        // Double-checked locking.
        if let Some(tx) = guard.get(user_id) {
            return tx.clone();
        }
        let (tx, _) = broadcast::channel(CHAN_CAPACITY);
        guard.insert(user_id.to_owned(), tx.clone());
        tx
    }

    /// Push an event to all connected devices of a user. Silently drops if
    /// no devices are connected (all receivers dropped).
    pub async fn send_to_user(&self, user_id: &str, event: ServerEvent) {
        let guard = self.channels.read().await;
        if let Some(tx) = guard.get(user_id) {
            // Ignore send errors — no receivers means nobody is online.
            let _ = tx.send(event);
        }
    }

    /// Push an event to every member of a list of user IDs.
    pub async fn broadcast_to_users(&self, user_ids: &[String], event: ServerEvent) {
        let guard = self.channels.read().await;
        for uid in user_ids {
            if let Some(tx) = guard.get(uid.as_str()) {
                let _ = tx.send(event.clone());
            }
        }
    }

    /// Remove the broadcast channel for a user (cleanup when all devices disconnect).
    async fn maybe_cleanup(&self, user_id: &str) {
        let mut guard = self.channels.write().await;
        if let Some(tx) = guard.get(user_id) {
            // receiver_count == 0 means all devices disconnected.
            if tx.receiver_count() == 0 {
                guard.remove(user_id);
            }
        }
    }
}

impl Default for WsState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new().route("/ws", get(ws_handler))
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

/// WebSocket upgrade handler. Authenticates via `?token=<jwt>`.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, query.token, state))
}

/// Drive one WebSocket connection for the lifetime of the session.
async fn handle_socket(socket: WebSocket, token: String, state: AppState) {
    // Validate token.
    let claims = match Claims::decode(&token, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(_) => {
            // Close immediately — invalid token.
            let (mut sink, _) = socket.split();
            let msg = serde_json::to_string(&ServerEvent::DeviceRevoked).unwrap_or_default();
            let _ = sink.send(Message::Text(msg.into())).await;
            return;
        }
    };

    let user_id = claims.sub;
    let device_id = claims.device_id;

    debug!("WS connected: user={user_id} device={device_id}");

    let tx = Arc::clone(&state.ws).sender_for(&user_id).await;
    let mut rx = tx.subscribe();

    let (mut sink, mut stream) = socket.split();

    // Clone user_id before the async move so the original remains available below.
    let send_user_id = user_id.clone();

    // Spawn a task to forward broadcast events → this WebSocket.
    let send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let Ok(json) = serde_json::to_string(&event) else {
                        continue;
                    };
                    if sink.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("WS lagged {n} messages for user={send_user_id}");
                }
            }
        }
    });

    // Read client → server messages.
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                handle_client_message(&text, &user_id, &state).await;
            }
            Message::Close(_) => break,
            // Ignore binary, ping/pong frames (axum handles pong automatically).
            _ => {}
        }
    }

    send_task.abort();
    debug!("WS disconnected: user={user_id} device={device_id}");

    // Clean up broadcast channel if this was the last device.
    Arc::clone(&state.ws).maybe_cleanup(&user_id).await;
}

// ── Client → server messages ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "data", rename_all = "snake_case")]
enum ClientMessage {
    TypingStart { channel_id: String },
    Heartbeat,
    VoiceJoin { channel_id: String },
    VoiceLeave { channel_id: String },
    VoiceSignal { target_user_id: String, sdp: String },
}

async fn handle_client_message(text: &str, user_id: &str, state: &AppState) {
    let Ok(msg) = serde_json::from_str::<ClientMessage>(text) else {
        return;
    };
    match msg {
        ClientMessage::TypingStart { channel_id } => {
            // TODO(phase-2.2.7.4): resolve channel members and broadcast TypingStart.
            // For now just log.
            debug!("TypingStart: user={user_id} channel={channel_id}");
            let _ = (channel_id, state);
        }
        ClientMessage::Heartbeat => {
            // TODO(phase-2.2.7.4): update device last_seen in DB.
        }
        ClientMessage::VoiceJoin { channel_id } => {
            debug!("VoiceJoin: user={user_id} channel={channel_id}");
        }
        ClientMessage::VoiceLeave { channel_id } => {
            debug!("VoiceLeave: user={user_id} channel={channel_id}");
        }
        ClientMessage::VoiceSignal {
            target_user_id,
            sdp,
        } => {
            // Relay WebRTC signal to target peer.
            // TODO(phase-2.2.8.4): proper VoiceSignal event type.
            debug!(
                "VoiceSignal: {user_id} → {target_user_id} ({} bytes)",
                sdp.len()
            );
        }
    }
}
