//! Discord main gateway WebSocket transport for WASM — `gateway-bridge` feature.
//!
//! Mirrors the layering of `voice_bridge.rs`: uses `gloo_net::websocket`
//! (browser-native, no FFI) instead of `tokio-tungstenite` which requires
//! `mio` / `tokio/net` and cannot compile for `wasm32-unknown-unknown`.
//!
//! # Responsibilities
//!
//! 1. Connect to `wss://gateway.discord.gg/?v=10`.
//! 2. Send op 2 IDENTIFY with the caller's bot/user token.
//! 3. Receive dispatches and stash voice credentials:
//!    - `VOICE_STATE_UPDATE` → extract `session_id` → `CredsGuard`.
//!    - `VOICE_SERVER_UPDATE` → extract `endpoint` + `token` → `CredsGuard`.
//! 4. Forward outbound payloads sent via `outbound_rx` (op 4 Voice State Update,
//!    etc.) on the WebSocket.
//! 5. Respond to op 10 HELLO with heartbeats and op 2 IDENTIFY.
//!
//! # Send handle
//!
//! The caller (lib.rs `event_stream()`) receives a
//! `tokio::sync::mpsc::UnboundedSender<String>` from `start`.  This sender
//! is `Send + Sync` and can be stored on `DiscordClient` inside an
//! `Arc<Mutex<Option<_>>>`.  `join_voice_channel_transport` locks the mutex,
//! clones the sender, and sends the op 4 JSON string.

use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use gloo_net::websocket::{Message, futures::WebSocket};
use serde_json::Value;
use tokio::sync::Mutex;

// ── VoiceServerCreds ─────────────────────────────────────────────────────────

/// Voice credentials extracted from the main gateway.
///
/// Stashed inside `Arc<Mutex<VoiceServerCreds>>` on `DiscordClient`
/// (wasm32 + gateway-bridge only). Once all three fields are `Some` and
/// non-empty, they are ready to pass to `DiscordVoiceBridgeClient::connect_voice`.
#[derive(Debug, Clone, Default)]
pub struct VoiceServerCreds {
    /// From `VOICE_SERVER_UPDATE.endpoint`.
    pub endpoint: Option<String>,
    /// From `VOICE_SERVER_UPDATE.token`.
    pub token: Option<String>,
    /// From `VOICE_STATE_UPDATE.session_id` (local user's voice state).
    pub session_id: Option<String>,
}

impl VoiceServerCreds {
    /// Returns `true` when all three credentials are present and non-empty.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(
            (&self.endpoint, &self.token, &self.session_id),
            (Some(e), Some(t), Some(s))
                if !e.is_empty() && !t.is_empty() && !s.is_empty()
        )
    }
}

/// Shared credential stash — `Arc<Mutex<VoiceServerCreds>>`.
///
/// `Arc<Mutex<>>` is `Send + Sync`, safe to store on `DiscordClient`.
pub type CredsGuard = Arc<Mutex<VoiceServerCreds>>;

// ── start ─────────────────────────────────────────────────────────────────────

/// Spawn the gateway bridge loop via `wasm_bindgen_futures::spawn_local`.
///
/// - Opens a browser WebSocket to `gateway_url`.
/// - Returns an `UnboundedSender<String>` for pushing outbound payloads
///   (op 4 Voice State Update etc.) onto the WebSocket.
/// - Spawns a local future that drives the heartbeat + receive loop until the
///   WebSocket closes.
///
/// The returned `UnboundedSender` is `Send + Sync` and can be stored in an
/// `Arc<Mutex<Option<_>>>` on `DiscordClient`.
///
/// # Errors
///
/// Returns an error string if `WebSocket::open` fails (DNS, TLS, HTTP 101
/// handshake, or the gateway returning a non-websocket response).
pub async fn start(
    gateway_url: String,
    token: String,
    creds: CredsGuard,
    local_user_id: String,
) -> Result<tokio::sync::mpsc::UnboundedSender<String>, String> {
    tracing::info!(
        target: "poly_discord::gateway_bridge",
        url = %gateway_url,
        "gateway-bridge: connecting"
    );

    let ws = WebSocket::open(&gateway_url)
        .map_err(|e| format!("gateway-bridge WebSocket::open: {e:?}"))?;

    let (ws_tx, ws_rx) = ws.split();
    let tx_rc = Rc::new(RefCell::new(ws_tx));

    let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Spawn the receive loop as a local (non-Send) future.
    let tx_for_loop = Rc::clone(&tx_rc);
    wasm_bindgen_futures::spawn_local(run_loop(
        ws_rx,
        tx_for_loop,
        token,
        creds,
        local_user_id,
        outbound_rx,
    ));

    Ok(outbound_tx)
}

// ── run_loop ──────────────────────────────────────────────────────────────────

/// Inner receive + forward loop — drives the gateway protocol on wasm32.
///
/// Exits when the WebSocket closes, an error occurs, or op 9 INVALID_SESSION
/// is received.
async fn run_loop(
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    tx: Rc<RefCell<futures::stream::SplitSink<WebSocket, Message>>>,
    token: String,
    creds: CredsGuard,
    local_user_id: String,
    mut outbound_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    // local_user_id is no longer used for filtering — each DiscordClient has its own
    // gateway connection so VOICE_STATE_UPDATE is implicitly for this account.
    let _ = local_user_id;

    let mut heartbeat_interval_ms: u64 = 45_000;
    let mut identified = false;

    loop {
        use futures::future::Either;

        // Three-way select: heartbeat timer | outbound payload | inbound WS message.
        // We compose them as nested Either to keep the match readable.
        // Each future must be pinned to a named binding before being passed to
        // `futures::future::select` — std::pin::pin! temporaries don't live
        // long enough when chained across two select calls.
        let hb_fut = gloo_timers::future::TimeoutFuture::new(heartbeat_interval_ms as u32);
        let outbound_fut = outbound_rx.recv();
        let inbound_fut = ws_rx.next();

        let mut hb_pinned = std::pin::pin!(hb_fut);
        let mut outbound_pinned = std::pin::pin!(outbound_fut);
        let mut inbound_pinned = std::pin::pin!(inbound_fut);

        // First select: heartbeat vs (outbound | inbound)
        let rest_fut = futures::future::select(outbound_pinned.as_mut(), inbound_pinned.as_mut());
        match futures::future::select(hb_pinned.as_mut(), std::pin::pin!(rest_fut)).await {
            // ── Heartbeat timer fired ─────────────────────────────────────
            Either::Left(((), _)) => {
                let hb = serde_json::json!({ "op": 1, "d": serde_json::Value::Null });
                if tx.borrow_mut().send(Message::Text(hb.to_string())).await.is_err() {
                    break;
                }
            }

            // ── Outbound payload from caller ──────────────────────────────
            Either::Right((Either::Left((Some(payload), _)), _)) => {
                if tx.borrow_mut().send(Message::Text(payload)).await.is_err() {
                    break;
                }
            }

            // ── Outbound channel closed ───────────────────────────────────
            Either::Right((Either::Left((None, _)), _)) => {
                // All senders dropped — nobody will send op 4 anymore; keep running.
            }

            // ── WS closed or error ────────────────────────────────────────
            Either::Right((Either::Right((None, _)), _))
            | Either::Right((Either::Right((Some(Err(_)), _)), _)) => {
                tracing::info!(
                    target: "poly_discord::gateway_bridge",
                    "gateway-bridge: WebSocket closed or errored"
                );
                break;
            }

            // ── WS message ────────────────────────────────────────────────
            Either::Right((Either::Right((Some(Ok(msg)), _)), _)) => {
                let text = match msg {
                    Message::Text(t) => t,
                    Message::Bytes(_) => continue,
                };

                let frame: Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let op = frame.get("op").and_then(Value::as_u64).unwrap_or(99);

                match op {
                    // op 10 HELLO — set heartbeat interval, send IDENTIFY.
                    10 => {
                        if let Some(ms) = frame
                            .get("d")
                            .and_then(|d| d.get("heartbeat_interval"))
                            .and_then(Value::as_u64)
                        {
                            heartbeat_interval_ms = ms;
                        }
                        if !identified {
                            let identify = serde_json::json!({
                                "op": 2,
                                "d": {
                                    "token": token,
                                    "intents": 513,
                                    "properties": {
                                        "$os": "browser",
                                        "$browser": "poly",
                                        "$device": "poly"
                                    },
                                    "compress": false
                                }
                            });
                            tracing::info!(
                                target: "poly_discord::gateway_bridge",
                                "gateway-bridge: sending IDENTIFY"
                            );
                            if tx
                                .borrow_mut()
                                .send(Message::Text(identify.to_string()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                            identified = true;
                        }
                    }

                    // op 0 DISPATCH — stash voice credential events.
                    0 => {
                        let event_name = frame.get("t").and_then(Value::as_str).unwrap_or("");
                        let data = frame.get("d").cloned().unwrap_or(Value::Null);

                        match event_name {
                            "VOICE_STATE_UPDATE" => {
                                let sid = data
                                    .get("session_id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string();
                                if !sid.is_empty() {
                                    let mut guard = creds.lock().await;
                                    guard.session_id = Some(sid.clone());
                                    tracing::info!(
                                        target: "poly_discord::gateway_bridge",
                                        session_id = %sid,
                                        "gateway-bridge: stashed session_id from VOICE_STATE_UPDATE"
                                    );
                                }
                            }
                            "VOICE_SERVER_UPDATE" => {
                                let endpoint = data
                                    .get("endpoint")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                let tok = data
                                    .get("token")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                {
                                    let mut guard = creds.lock().await;
                                    if let Some(ep) = endpoint.clone() {
                                        guard.endpoint = Some(ep);
                                    }
                                    if let Some(t) = tok {
                                        guard.token = Some(t);
                                    }
                                }
                                tracing::info!(
                                    target: "poly_discord::gateway_bridge",
                                    endpoint = ?endpoint,
                                    "gateway-bridge: stashed endpoint+token from VOICE_SERVER_UPDATE"
                                );
                            }
                            "READY" => {
                                let session_id = data
                                    .get("session_id")
                                    .and_then(Value::as_str)
                                    .unwrap_or("(none)");
                                tracing::info!(
                                    target: "poly_discord::gateway_bridge",
                                    session_id,
                                    "gateway-bridge: READY received"
                                );
                            }
                            _ => {
                                // Other dispatches ignored on this path.
                            }
                        }
                    }

                    // op 11 HEARTBEAT_ACK — silently acknowledged.
                    11 => {}

                    // op 9 INVALID_SESSION — reconnect not implemented; exit.
                    9 => {
                        tracing::warn!(
                            target: "poly_discord::gateway_bridge",
                            "gateway-bridge: op 9 INVALID_SESSION, closing"
                        );
                        break;
                    }

                    _ => {}
                }
            }
        }
    }

    tracing::info!(
        target: "poly_discord::gateway_bridge",
        "gateway-bridge: receive loop exited"
    );
}
