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
    pub const fn is_complete(&self) -> bool {
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
#[allow(clippy::unused_async)] // async kept for API parity with tokio-tungstenite callers
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

/// What woke `run_loop` this iteration.
///
/// Materialising the select result into an owned value ends the borrows of the
/// three pinned futures, which is what lets the heartbeat timer live *across*
/// loop iterations instead of being rebuilt (and therefore reset) every time.
enum GatewayWake {
    /// The heartbeat deadline elapsed.
    Heartbeat,
    /// A payload arrived on the caller's outbound channel (`None` = closed).
    Outbound(Option<String>),
    /// A frame arrived on the WebSocket (`None` = stream ended).
    Inbound(Option<Result<Message, gloo_net::websocket::WebSocketError>>),
}

/// Inner receive + forward loop — drives the gateway protocol on wasm32.
///
/// Exits when the WebSocket closes, an error occurs, or op 9 INVALID_SESSION
/// is received.
// lint-allow-unused: run_loop has inherent line count from the three-way select protocol
#[allow(clippy::too_many_lines)]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::await_holding_refcell_ref)] // single-threaded wasm32: RefCell borrow cannot race
#[allow(clippy::future_not_send)] // gloo_net WebSocket is !Send by design on wasm32
#[allow(clippy::significant_drop_tightening)] // RefCell borrow scoping is intentional
#[allow(clippy::default_numeric_fallback)] // JSON op codes are unambiguously i32
#[allow(clippy::as_conversions)] // heartbeat_interval_ms casting is safe in range
#[allow(clippy::cast_possible_truncation)] // u64 ms -> u32 for gloo_timers: values <u32::MAX
#[allow(clippy::unnested_or_patterns)] // nested or-patterns in WS dispatch match are clearer
#[allow(clippy::match_same_arms)] // outbound-closed and WS-closed arms have same body intentionally
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

    // The heartbeat deadline MUST outlive a single loop iteration.  Building a
    // fresh `TimeoutFuture` inside the loop meant every inbound dispatch (and a
    // live gateway streams PRESENCE_UPDATE / TYPING_START / MESSAGE_CREATE
    // continuously) cancelled the pending timeout and restarted the interval
    // from zero — so op 1 was never sent and Discord closed the socket with
    // 4009 after ~45 s, permanently killing VOICE_STATE_UPDATE /
    // VOICE_SERVER_UPDATE delivery for the session.  It is only re-armed after
    // it actually fires, or when op 10 HELLO changes the interval.
    let mut hb_pinned = Box::pin(gloo_timers::future::TimeoutFuture::new(
        heartbeat_interval_ms as u32,
    ));

    loop {
        use futures::future::Either;

        // Three-way select: heartbeat timer | outbound payload | inbound WS message.
        // We compose them as nested Either to keep the match readable.
        // Each future must be pinned to a named binding before being passed to
        // `futures::future::select` — std::pin::pin! temporaries don't live
        // long enough when chained across two select calls.
        let outbound_fut = outbound_rx.recv();
        let inbound_fut = ws_rx.next();

        let mut outbound_pinned = std::pin::pin!(outbound_fut);
        let mut inbound_pinned = std::pin::pin!(inbound_fut);

        // First select: heartbeat vs (outbound | inbound).  The result is
        // materialised into an owned `GatewayWake` so the borrow of
        // `hb_pinned` ends here and the arms below can re-arm the timer.
        let wake = {
            let rest_fut =
                futures::future::select(outbound_pinned.as_mut(), inbound_pinned.as_mut());
            match futures::future::select(hb_pinned.as_mut(), std::pin::pin!(rest_fut)).await {
                Either::Left(((), _)) => GatewayWake::Heartbeat,
                Either::Right((Either::Left((payload, _)), _)) => GatewayWake::Outbound(payload),
                Either::Right((Either::Right((msg, _)), _)) => GatewayWake::Inbound(msg),
            }
        };

        match wake {
            // ── Heartbeat timer fired ─────────────────────────────────────
            GatewayWake::Heartbeat => {
                let hb = serde_json::json!({ "op": 1, "d": serde_json::Value::Null });
                if tx.borrow_mut().send(Message::Text(hb.to_string())).await.is_err() {
                    break;
                }
                // Re-arm for the next interval — a completed TimeoutFuture must
                // not be polled again.
                hb_pinned = Box::pin(gloo_timers::future::TimeoutFuture::new(
                    heartbeat_interval_ms as u32,
                ));
            }

            // ── Outbound payload from caller ──────────────────────────────
            GatewayWake::Outbound(Some(payload)) => {
                if tx.borrow_mut().send(Message::Text(payload)).await.is_err() {
                    break;
                }
            }

            // ── Outbound channel closed ───────────────────────────────────
            GatewayWake::Outbound(None) => {
                // All senders dropped — nobody will send op 4 anymore; keep running.
            }

            // ── WS closed or error ────────────────────────────────────────
            GatewayWake::Inbound(None) | GatewayWake::Inbound(Some(Err(_))) => {
                tracing::info!(
                    target: "poly_discord::gateway_bridge",
                    "gateway-bridge: WebSocket closed or errored"
                );
                break;
            }

            // ── WS message ────────────────────────────────────────────────
            GatewayWake::Inbound(Some(Ok(msg))) => {
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
                            // Re-arm immediately: the timer currently in flight
                            // was built with the 45 s default.
                            hb_pinned = Box::pin(gloo_timers::future::TimeoutFuture::new(
                                heartbeat_interval_ms as u32,
                            ));
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
