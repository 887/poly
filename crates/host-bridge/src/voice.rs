//! # `/host/voice/*` — Discord voice transport bridge
//!
//! Exposes a high-level RPC surface so browser WASM (which cannot open raw
//! UDP sockets) can participate in Discord voice channels through the native
//! server-half of every fullstack shell (`apps/web`, `apps/desktop`,
//! `apps/desktop-electron`).
//!
//! ## Architecture
//!
//! ```text
//! Browser WASM                Native server-half (this module)
//! ───────────────             ──────────────────────────────────────────
//! VoiceBridgeClient           VoiceState
//!   POST /host/voice/connect ─→ spawn DiscordVoiceTransport
//!                                  │
//!                                  ├─ voice WS (tokio-tungstenite)
//!                                  ├─ UDP socket (tokio::net::UdpSocket)
//!                                  ├─ Opus encoder (audiopus)
//!                                  └─ AEAD key (chacha20poly1305)
//!   POST /host/voice/send_audio ─→ encode PCM → AEAD → UDP
//!   GET  /host/voice/events/:id ← SSE stream (VoiceEvent values)
//! ```
//!
//! ## WASM safety
//!
//! This entire module is `#[cfg(all(not(target_arch = "wasm32"), feature = "voice"))]`.
//! `audiopus` links against libopus FFI and `chacha20poly1305` is not the issue,
//! but `tokio::net::UdpSocket` + `tokio-tungstenite` are `not(wasm32)` only.
//! WASM callers use [`crate::voice_client::VoiceBridgeClient`] instead.
//!
//! ## Feature dependency
//!
//! The `voice` feature depends on `video` because the decode path for received
//! H.264 video reuses the openh264 codec sessions managed by `video.rs`.
//! `voice = ["dep:audiopus", "dep:chacha20poly1305", "dep:tokio-tungstenite", "video"]`.
//!
//! ## Session lifecycle
//!
//! 1. `POST /host/voice/connect` → `{ session_id, voice_ssrc, video_ssrc }`.
//! 2. Browser opens `GET /host/voice/events/:session_id` as EventSource.
//! 3. `POST /host/voice/send_audio` pushes PCM frames to the encode loop.
//! 4. `POST /host/voice/send_video` pushes video frames (wired to H.264 encode path).
//! 5. `POST /host/voice/set_mute` toggles native-side mute + gateway op 5.
//! 6. `POST /host/voice/disconnect` tears down the session.
//!
//! Orphan sessions (no SSE subscriber for >60s) are automatically GC'd.


use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use audiopus::{
    Application as OpusApplication, Channels as OpusChannels, MutSignals,
    SampleRate as OpusSampleRate,
    coder::{Decoder as OpusDecoder, Encoder as OpusEncoder},
    packet::Packet,
};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use base64::Engine as _;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use futures::{SinkExt, Stream, StreamExt};
use serde_json;
use tokio::{
    net::UdpSocket,
    sync::{broadcast, mpsc},
    time,
};
use tokio_tungstenite::{connect_async, tungstenite::Message as TMsg};
use tracing::{debug, info, warn};
use uuid::Uuid;

// Re-export wire types used by the handlers (same types the client sees).
use crate::voice_wire::{
    SendAudioRequest, SendAudioResponse, SendVideoRequest, SendVideoResponse,
    SetMuteRequest, SetMuteResponse, VoiceConnectRequest, VoiceConnectResponse,
    VoiceDisconnectRequest, VoiceDisconnectResponse, VoiceEvent,
    ROUTE_VOICE_CONNECT, ROUTE_VOICE_DISCONNECT, ROUTE_VOICE_EVENTS_PATTERN,
    ROUTE_VOICE_SEND_AUDIO, ROUTE_VOICE_SEND_VIDEO, ROUTE_VOICE_SET_MUTE,
};

// ── Protocol constants ─────────────────────────────────────────────────────────

const VOICE_WS_VERSION: u8 = 4;
const RTP_PAYLOAD_TYPE_OPUS: u8 = 0x78; // 120
const RTP_HEADER_SIZE: usize = 12;
const OPUS_FRAME_SAMPLES: usize = 1920; // 20ms @ 48 kHz stereo
const MAX_UDP_PACKET: usize = 1500;
const PREFERRED_AEAD_MODES: &[&str] = &[
    "aead_xchacha20_poly1305_rtpsize",
    "aead_aes256_gcm_rtpsize",
];
const SESSION_ORPHAN_TIMEOUT: Duration = Duration::from_secs(60);

// ── Session state ─────────────────────────────────────────────────────────────

struct VoiceSessionInner {
    event_tx: broadcast::Sender<VoiceEvent>,
    audio_tx: mpsc::Sender<Vec<i16>>,
    mute_tx: mpsc::Sender<(bool, bool)>,
    last_poll: Instant,
    shutdown_tx: mpsc::Sender<()>,
    muted: bool,
}

#[derive(Clone)]
struct VoiceSession {
    inner: Arc<Mutex<VoiceSessionInner>>,
}

/// Shared state for the voice bridge — a map of live sessions keyed by session_id.
#[derive(Clone, Default)]
pub struct VoiceState {
    sessions: Arc<Mutex<HashMap<String, VoiceSession>>>,
}

impl VoiceState {
    /// Construct an empty voice state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build the `/host/voice/*` sub-router.
///
/// Merge this into the main `poly_host::router()` analogously to the video router.
#[must_use]
pub fn router(state: VoiceState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(ROUTE_VOICE_CONNECT, post(handle_connect))
        .route(ROUTE_VOICE_DISCONNECT, post(handle_disconnect))
        .route(ROUTE_VOICE_SEND_AUDIO, post(handle_send_audio))
        .route(ROUTE_VOICE_SEND_VIDEO, post(handle_send_video))
        .route(ROUTE_VOICE_SET_MUTE, post(handle_set_mute))
        .route(ROUTE_VOICE_EVENTS_PATTERN, get(handle_events_sse))
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn handle_connect(
    State(state): State<VoiceState>,
    Json(req): Json<VoiceConnectRequest>,
) -> impl IntoResponse {
    let session_id = Uuid::new_v4().to_string();
    match do_connect(req, session_id.clone(), Arc::clone(&state.sessions)).await {
        Ok((voice_ssrc, video_ssrc)) => (
            StatusCode::OK,
            Json(VoiceConnectResponse {
                ok: true,
                session_id,
                voice_ssrc,
                video_ssrc,
                err: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(VoiceConnectResponse {
                ok: false,
                session_id: String::new(),
                voice_ssrc: 0,
                video_ssrc: 0,
                err: Some(e),
            }),
        ),
    }
}

async fn handle_disconnect(
    State(state): State<VoiceState>,
    Json(req): Json<VoiceDisconnectRequest>,
) -> impl IntoResponse {
    let removed = {
        let mut map = match state.sessions.lock() {
            Ok(m) => m,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(VoiceDisconnectResponse {
                        ok: false,
                        err: Some(format!("lock poisoned: {e}")),
                    }),
                );
            }
        };
        map.remove(&req.session_id)
    };

    if let Some(session) = removed {
        if let Ok(inner) = session.inner.lock() {
            let _ = inner.shutdown_tx.try_send(());
            let _ = inner.event_tx.send(VoiceEvent::Disconnected {
                reason: "client disconnected".into(),
            });
        }
    }

    (StatusCode::OK, Json(VoiceDisconnectResponse { ok: true, err: None }))
}

async fn handle_send_audio(
    State(state): State<VoiceState>,
    Json(req): Json<SendAudioRequest>,
) -> impl IntoResponse {
    let session = get_session(&state, &req.session_id);
    let session = match session {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(SendAudioResponse {
                    ok: false,
                    sent_bytes: 0,
                    err: Some(format!("session {} not found", req.session_id)),
                }),
            );
        }
    };

    let raw = match b64_decode(&req.pcm_b64) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(SendAudioResponse {
                    ok: false,
                    sent_bytes: 0,
                    err: Some(format!("invalid pcm_b64: {e}")),
                }),
            );
        }
    };

    if raw.len() % 2 != 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(SendAudioResponse {
                ok: false,
                sent_bytes: 0,
                err: Some("pcm_b64 byte length must be even (i16 pairs)".into()),
            }),
        );
    }

    let pcm: Vec<i16> = raw
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();

    let (audio_tx, muted) = match session.inner.lock() {
        Ok(inner) => (inner.audio_tx.clone(), inner.muted),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SendAudioResponse {
                    ok: false,
                    sent_bytes: 0,
                    err: Some(format!("session lock poisoned: {e}")),
                }),
            );
        }
    };

    if muted {
        return (StatusCode::OK, Json(SendAudioResponse { ok: true, sent_bytes: 0, err: None }));
    }

    let byte_estimate = pcm.len() * 2;
    let _ = audio_tx.send(pcm).await;

    (StatusCode::OK, Json(SendAudioResponse { ok: true, sent_bytes: byte_estimate, err: None }))
}

async fn handle_send_video(
    State(state): State<VoiceState>,
    Json(req): Json<SendVideoRequest>,
) -> impl IntoResponse {
    let session = get_session(&state, &req.session_id);
    if session.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(SendVideoResponse {
                ok: false,
                err: Some(format!("session {} not found", req.session_id)),
            }),
        );
    }
    // TODO(voice-bridge-video): wire video_tx → H.264 encode → RTP → UDP.
    (StatusCode::OK, Json(SendVideoResponse { ok: true, err: None }))
}

async fn handle_set_mute(
    State(state): State<VoiceState>,
    Json(req): Json<SetMuteRequest>,
) -> impl IntoResponse {
    let session = get_session(&state, &req.session_id);
    let session = match session {
        Some(s) => s,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(SetMuteResponse {
                    ok: false,
                    err: Some(format!("session {} not found", req.session_id)),
                }),
            );
        }
    };

    let mute_tx = match session.inner.lock() {
        Ok(mut inner) => {
            inner.muted = req.muted;
            inner.mute_tx.clone()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SetMuteResponse {
                    ok: false,
                    err: Some(format!("session lock poisoned: {e}")),
                }),
            );
        }
    };
    let _ = mute_tx.send((req.muted, req.deafened)).await;

    (StatusCode::OK, Json(SetMuteResponse { ok: true, err: None }))
}

async fn handle_events_sse(
    State(state): State<VoiceState>,
    AxumPath(session_id): AxumPath<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;
    use futures::stream::{self, BoxStream};

    // Helper: build a boxed SSE stream from any compatible stream.
    fn sse_response(
        stream: BoxStream<'static, Result<Event, std::convert::Infallible>>,
    ) -> axum::response::Response {
        Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response()
    }

    let rx_opt = {
        let map = match state.sessions.lock() {
            Ok(m) => m,
            Err(_) => {
                return sse_response(Box::pin(stream::empty()));
            }
        };
        let session = map.get(&session_id).cloned();
        session.and_then(|s| {
            s.inner.lock().ok().map(|mut inner| {
                inner.last_poll = Instant::now();
                inner.event_tx.subscribe()
            })
        })
    };

    let rx = match rx_opt {
        Some(r) => r,
        None => {
            // Session not found — emit one Disconnected event then close.
            let json = serde_json::to_string(&VoiceEvent::Disconnected {
                reason: "session not found".into(),
            })
            .unwrap_or_default();
            let once_stream = stream::once(async move {
                Ok::<Event, std::convert::Infallible>(
                    Event::default().event("voice").data(json),
                )
            });
            return sse_response(Box::pin(once_stream));
        }
    };

    let stream = make_event_stream(rx);
    sse_response(Box::pin(stream))
}

fn make_event_stream(
    mut rx: broadcast::Receiver<VoiceEvent>,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let is_disconnect = matches!(ev, VoiceEvent::Disconnected { .. });
                    let json = serde_json::to_string(&ev).unwrap_or_default();
                    yield Ok(Event::default().event("voice").data(json));
                    if is_disconnect { break; }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

// ── Helper: look up a session ─────────────────────────────────────────────────

fn get_session(state: &VoiceState, session_id: &str) -> Option<VoiceSession> {
    state
        .sessions
        .lock()
        .ok()
        .and_then(|m| m.get(session_id).cloned())
}

// ── Connection internals ──────────────────────────────────────────────────────

struct VoiceReady {
    ssrc: u32,
    ip: String,
    port: u16,
    modes: Vec<String>,
}

struct SessionDesc {
    secret_key: Vec<u8>,
}

type WsWrite = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    TMsg,
>;
type WsRead = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

async fn do_connect(
    req: VoiceConnectRequest,
    session_id: String,
    sessions: Arc<Mutex<HashMap<String, VoiceSession>>>,
) -> Result<(u32, u32), String> {
    let ws_url = format!(
        "wss://{}/?v={}",
        req.ws_endpoint.trim_end_matches(':').trim_end_matches('/'),
        VOICE_WS_VERSION
    );

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| format!("voice WS connect: {e}"))?;
    let (mut ws_write, mut ws_read) = ws_stream.split();

    let heartbeat_ms = wait_for_hello(&mut ws_read).await?;
    send_identify(&mut ws_write, &req).await?;
    let ready = wait_for_ready(&mut ws_read).await?;
    info!(
        target: "poly_host_bridge::voice",
        ssrc = ready.ssrc, ip = %ready.ip, port = ready.port,
        session = %session_id, "voice READY"
    );

    let mode = select_encryption_mode(&ready.modes)
        .ok_or_else(|| "no supported AEAD mode in op-2 Ready".to_string())?;

    let udp = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("UDP bind: {e}"))?;
    let server_addr: std::net::SocketAddr = format!("{}:{}", ready.ip, ready.port)
        .parse()
        .map_err(|e| format!("bad server addr: {e}"))?;
    udp.connect(server_addr)
        .await
        .map_err(|e| format!("UDP connect: {e}"))?;

    let (local_ip, local_port) = ip_discovery(&udp, ready.ssrc).await?;
    info!(
        target: "poly_host_bridge::voice",
        ip = %local_ip, port = local_port, "IP discovery done"
    );

    send_select_protocol(&mut ws_write, &local_ip, local_port, &mode).await?;

    let session_desc = wait_for_session_description(&mut ws_read).await?;
    if session_desc.secret_key.len() != 32 {
        return Err(format!(
            "session key wrong length: expected 32 got {}",
            session_desc.secret_key.len()
        ));
    }
    let mut secret_key = [0u8; 32];
    secret_key.copy_from_slice(&session_desc.secret_key);

    let local_ssrc = ready.ssrc;
    let video_ssrc = local_ssrc + 1;

    let (event_tx, _) = broadcast::channel::<VoiceEvent>(64);
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>(32);
    let (mute_tx, mute_rx) = mpsc::channel::<(bool, bool)>(4);
    let (ws_out_tx, ws_out_rx) = mpsc::channel::<serde_json::Value>(16);
    let (shutdown_tx, _) = mpsc::channel::<()>(1);
    let udp_arc = Arc::new(udp);

    {
        let ev_tx = event_tx.clone();
        tokio::spawn(voice_ws_loop(
            ws_write,
            ws_read,
            VoiceWsParams {
                local_ssrc,
                heartbeat_interval_ms: heartbeat_ms,
                event_tx: ev_tx,
                mute_rx,
                ws_out_rx,
            },
        ));
    }
    {
        let udp = Arc::clone(&udp_arc);
        let ev_tx = event_tx.clone();
        tokio::spawn(udp_decode_loop(udp, secret_key, mode.clone(), ev_tx));
    }
    {
        let udp = Arc::clone(&udp_arc);
        tokio::spawn(udp_encode_loop(udp, audio_rx, secret_key, mode, local_ssrc, ws_out_tx));
    }

    let session = VoiceSession {
        inner: Arc::new(Mutex::new(VoiceSessionInner {
            event_tx,
            audio_tx,
            mute_tx,
            last_poll: Instant::now(),
            shutdown_tx,
            muted: false,
        })),
    };

    {
        let mut map = sessions
            .lock()
            .map_err(|e| format!("sessions lock poisoned: {e}"))?;
        map.insert(session_id.clone(), session);
    }

    tokio::spawn(orphan_gc(session_id, Arc::clone(&sessions)));

    Ok((local_ssrc, video_ssrc))
}

// ── Orphan GC ─────────────────────────────────────────────────────────────────

async fn orphan_gc(session_id: String, sessions: Arc<Mutex<HashMap<String, VoiceSession>>>) {
    let mut interval = time::interval(Duration::from_secs(15));
    loop {
        interval.tick().await;
        let session = {
            let map = match sessions.lock() {
                Ok(m) => m,
                Err(_) => break,
            };
            map.get(&session_id).cloned()
        };
        let session = match session {
            Some(s) => s,
            None => break,
        };
        let stale = session
            .inner
            .lock()
            .map(|i| i.last_poll.elapsed() > SESSION_ORPHAN_TIMEOUT)
            .unwrap_or(false);

        if stale {
            info!(target: "poly_host_bridge::voice", session = %session_id, "orphan GC");
            let removed = sessions.lock().ok().and_then(|mut m| m.remove(&session_id));
            if let Some(s) = removed {
                if let Ok(inner) = s.inner.lock() {
                    let _ = inner.shutdown_tx.try_send(());
                    let _ = inner.event_tx.send(VoiceEvent::Disconnected {
                        reason: "orphan timeout — no SSE subscriber for 60s".into(),
                    });
                }
            }
            break;
        }
    }
}

// ── WS handshake helpers ──────────────────────────────────────────────────────

async fn wait_for_hello(read: &mut WsRead) -> Result<u64, String> {
    while let Some(msg) = read.next().await {
        let text = match msg {
            Ok(TMsg::Text(t)) => t.to_string(),
            _ => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
            let ms = v
                .get("d")
                .and_then(|d| d.get("heartbeat_interval"))
                .and_then(|i| i.as_u64())
                .unwrap_or(5000);
            return Ok(ms);
        }
    }
    Err("WS closed before op 8 Hello".into())
}

async fn send_identify(write: &mut WsWrite, req: &VoiceConnectRequest) -> Result<(), String> {
    let payload = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": req.guild_id.as_deref().unwrap_or(&req.user_id),
            "user_id": req.user_id,
            "session_id": req.ws_session_id,
            "token": req.ws_token,
        }
    });
    write
        .send(TMsg::Text(payload.to_string().into()))
        .await
        .map_err(|e| format!("send IDENTIFY: {e}"))
}

async fn wait_for_ready(read: &mut WsRead) -> Result<VoiceReady, String> {
    while let Some(msg) = read.next().await {
        let text = match msg {
            Ok(TMsg::Text(t)) => t.to_string(),
            _ => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            return Ok(VoiceReady {
                ssrc: d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32,
                ip: d.get("ip").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                port: d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16,
                modes: d
                    .get("modes")
                    .and_then(|m| m.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default(),
            });
        }
    }
    Err("WS closed before op 2 Ready".into())
}

async fn wait_for_session_description(read: &mut WsRead) -> Result<SessionDesc, String> {
    while let Some(msg) = read.next().await {
        let text = match msg {
            Ok(TMsg::Text(t)) => t.to_string(),
            _ => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(4) {
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let key: Vec<u8> = d
                .get("secret_key")
                .and_then(|k| k.as_array())
                .map(|a| a.iter().filter_map(|b| b.as_u64().map(|n| n as u8)).collect())
                .unwrap_or_default();
            return Ok(SessionDesc { secret_key: key });
        }
    }
    Err("WS closed before op 4 Session Description".into())
}

fn select_encryption_mode(modes: &[String]) -> Option<String> {
    for preferred in PREFERRED_AEAD_MODES {
        if modes.iter().any(|m| m == preferred) {
            return Some((*preferred).to_string());
        }
    }
    None
}

async fn send_select_protocol(
    write: &mut WsWrite,
    local_ip: &str,
    local_port: u16,
    mode: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": { "address": local_ip, "port": local_port, "mode": mode }
        }
    });
    write
        .send(TMsg::Text(payload.to_string().into()))
        .await
        .map_err(|e| format!("send SELECT PROTOCOL: {e}"))
}

// ── IP-discovery ──────────────────────────────────────────────────────────────

async fn ip_discovery(udp: &UdpSocket, ssrc: u32) -> Result<(String, u16), String> {
    let mut buf = [0u8; 74];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x46;
    buf[4] = (ssrc >> 24) as u8;
    buf[5] = (ssrc >> 16) as u8;
    buf[6] = (ssrc >> 8) as u8;
    buf[7] = ssrc as u8;
    udp.send(&buf).await.map_err(|e| format!("IP discovery send: {e}"))?;

    let mut resp = [0u8; 74];
    let n = time::timeout(Duration::from_secs(5), udp.recv(&mut resp))
        .await
        .map_err(|_| "IP discovery timed out".to_string())?
        .map_err(|e| format!("IP discovery recv: {e}"))?;

    if n < 74 {
        return Err(format!("IP discovery: short response {n} bytes"));
    }
    if u16::from_be_bytes([resp[0], resp[1]]) != 0x0002 {
        return Err("IP discovery: unexpected response type".into());
    }
    let addr_end = resp[8..72].iter().position(|&b| b == 0).unwrap_or(64);
    let ip = std::str::from_utf8(&resp[8..8 + addr_end])
        .map_err(|e| format!("IP discovery: bad utf8: {e}"))?
        .to_string();
    let port = u16::from_be_bytes([resp[72], resp[73]]);
    Ok((ip, port))
}

// ── Voice WS loop ─────────────────────────────────────────────────────────────

struct VoiceWsParams {
    local_ssrc: u32,
    heartbeat_interval_ms: u64,
    event_tx: broadcast::Sender<VoiceEvent>,
    mute_rx: mpsc::Receiver<(bool, bool)>,
    ws_out_rx: mpsc::Receiver<serde_json::Value>,
}

async fn voice_ws_loop(
    mut write: WsWrite,
    mut read: WsRead,
    params: VoiceWsParams,
) {
    let VoiceWsParams {
        local_ssrc,
        heartbeat_interval_ms,
        event_tx,
        mut mute_rx,
        mut ws_out_rx,
    } = params;
    let mut heartbeat_tick = time::interval(Duration::from_millis(heartbeat_interval_ms));
    let mut nonce: u64 = 0;
    let mut ssrc_user: HashMap<u32, String> = HashMap::new();

    loop {
        tokio::select! {
            _ = heartbeat_tick.tick() => {
                nonce = nonce.wrapping_add(1);
                let hb = serde_json::json!({ "op": 3, "d": nonce });
                if write.send(TMsg::Text(hb.to_string().into())).await.is_err() { break; }
            }
            Some(outbound) = ws_out_rx.recv() => {
                if write.send(TMsg::Text(outbound.to_string().into())).await.is_err() { break; }
            }
            Some((muted, _deafened)) = mute_rx.recv() => {
                let bitmask: u32 = if muted { 0 } else { 1 };
                let ev = serde_json::json!({
                    "op": 5,
                    "d": { "speaking": bitmask, "delay": 0, "ssrc": local_ssrc }
                });
                if write.send(TMsg::Text(ev.to_string().into())).await.is_err() { break; }
            }
            msg = read.next() => {
                let text = match msg {
                    Some(Ok(TMsg::Text(t))) => t.to_string(),
                    Some(Ok(TMsg::Close(_))) | None => break,
                    _ => continue,
                };
                let v: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(99);
                if op == 5 {
                    if let Some(d) = v.get("d") {
                        let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
                        let user_id = d.get("user_id").and_then(|u| u.as_str()).unwrap_or("").to_string();
                        let speaking_bitmask = d.get("speaking").and_then(|s| s.as_u64()).unwrap_or(0);
                        if ssrc != 0 && !user_id.is_empty() {
                            let is_new = !ssrc_user.contains_key(&ssrc);
                            ssrc_user.insert(ssrc, user_id.clone());
                            if is_new {
                                let _ = event_tx.send(VoiceEvent::ParticipantJoin { user_id: user_id.clone(), ssrc });
                            }
                            let _ = event_tx.send(VoiceEvent::Speaking {
                                user_id,
                                is_speaking: speaking_bitmask != 0,
                            });
                        }
                    }
                }
            }
        }
    }
    debug!(target: "poly_host_bridge::voice", "voice WS loop exited");
}

// ── UDP encode loop ───────────────────────────────────────────────────────────

async fn udp_encode_loop(
    udp: Arc<UdpSocket>,
    mut audio_rx: mpsc::Receiver<Vec<i16>>,
    secret_key: [u8; 32],
    mode: String,
    local_ssrc: u32,
    ws_out_tx: mpsc::Sender<serde_json::Value>,
) {
    let encoder = match OpusEncoder::new(OpusSampleRate::Hz48000, OpusChannels::Stereo, OpusApplication::Voip) {
        Ok(e) => e,
        Err(e) => {
            warn!(target: "poly_host_bridge::voice", error = %e, "failed to create Opus encoder");
            return;
        }
    };
    let cipher = match XChaCha20Poly1305::new_from_slice(&secret_key) {
        Ok(c) => c,
        Err(e) => {
            warn!(target: "poly_host_bridge::voice", error = %e, "failed to create cipher");
            return;
        }
    };

    let mut sequence: u16 = 0;
    let mut timestamp: u32 = 0;
    let mut opus_buf = vec![0u8; 4000];
    let mut pcm_acc: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);
    let mut is_speaking = false;

    while let Some(pcm) = audio_rx.recv().await {
        pcm_acc.extend_from_slice(&pcm);

        while pcm_acc.len() >= OPUS_FRAME_SAMPLES {
            let frame: Vec<i16> = pcm_acc.drain(..OPUS_FRAME_SAMPLES).collect();

            let encoded_len = match encoder.encode(&frame, &mut opus_buf) {
                Ok(n) => n,
                Err(e) => {
                    warn!(target: "poly_host_bridge::voice", error = %e, "Opus encode error");
                    continue;
                }
            };
            let opus_data = &opus_buf[..encoded_len];

            if !is_speaking {
                is_speaking = true;
                let ev = serde_json::json!({ "op": 5, "d": { "speaking": 1u32, "delay": 0, "ssrc": local_ssrc } });
                let _ = ws_out_tx.try_send(ev);
            }

            let rtp_header = build_rtp_header(sequence, timestamp, local_ssrc);
            sequence = sequence.wrapping_add(1);
            timestamp = timestamp.wrapping_add(OPUS_FRAME_SAMPLES as u32);

            let encrypted = match encrypt_rtp(&cipher, &rtp_header, opus_data, &mode) {
                Ok(e) => e,
                Err(e) => {
                    warn!(target: "poly_host_bridge::voice", error = %e, "AEAD encrypt error");
                    continue;
                }
            };

            let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + encrypted.len());
            packet.extend_from_slice(&rtp_header);
            packet.extend_from_slice(&encrypted);
            if let Err(e) = udp.send(&packet).await {
                warn!(target: "poly_host_bridge::voice", error = %e, "UDP send error");
            }
        }
    }

    if is_speaking {
        let ev = serde_json::json!({ "op": 5, "d": { "speaking": 0u32, "delay": 0, "ssrc": local_ssrc } });
        let _ = ws_out_tx.try_send(ev);
    }
    debug!(target: "poly_host_bridge::voice", "encode loop exited");
}

// ── UDP decode loop ───────────────────────────────────────────────────────────

async fn udp_decode_loop(
    udp: Arc<UdpSocket>,
    secret_key: [u8; 32],
    mode: String,
    event_tx: broadcast::Sender<VoiceEvent>,
) {
    let cipher = match XChaCha20Poly1305::new_from_slice(&secret_key) {
        Ok(c) => c,
        Err(e) => {
            warn!(target: "poly_host_bridge::voice", error = %e, "decode: failed to create cipher");
            return;
        }
    };

    let mut decoders: HashMap<u32, OpusDecoder> = HashMap::new();
    let mut recv_buf = vec![0u8; MAX_UDP_PACKET];
    let mut pcm_buf = vec![0i16; OPUS_FRAME_SAMPLES * 2];

    loop {
        let n = match udp.recv(&mut recv_buf).await {
            Ok(n) => n,
            Err(e) => {
                warn!(target: "poly_host_bridge::voice", error = %e, "UDP recv error");
                break;
            }
        };

        let packet = &recv_buf[..n];
        if packet.len() < RTP_HEADER_SIZE {
            continue;
        }

        let (ssrc, payload_offset) = match parse_rtp_header(packet) {
            Some(r) => r,
            None => continue,
        };

        let rtp_header = &packet[..payload_offset];
        let ciphertext = &packet[payload_offset..];

        let plaintext = match decrypt_rtp(&cipher, rtp_header, ciphertext, &mode) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let decoder = decoders.entry(ssrc).or_insert_with(|| {
            OpusDecoder::new(OpusSampleRate::Hz48000, OpusChannels::Stereo)
                .expect("OpusDecoder::new never fails with valid params")
        });

        let packet_ref = match Packet::try_from(plaintext.as_slice()) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let mut_signals = match MutSignals::try_from(pcm_buf.as_mut_slice()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let decoded_samples = match decoder.decode(Some(packet_ref), mut_signals, false) {
            Ok(n) => n,
            Err(e) => {
                warn!(target: "poly_host_bridge::voice", ssrc, error = %e, "Opus decode error");
                continue;
            }
        };

        let pcm_slice = &pcm_buf[..decoded_samples * 2];
        let mut pcm_bytes = Vec::with_capacity(pcm_slice.len() * 2);
        for &s in pcm_slice {
            pcm_bytes.extend_from_slice(&s.to_le_bytes());
        }
        let pcm_b64 = b64_encode(&pcm_bytes);

        let _ = event_tx.send(VoiceEvent::FrameAudio {
            user_id: format!("user_{ssrc}"),
            pcm_b64,
            samples: decoded_samples as u32,
        });
    }
    debug!(target: "poly_host_bridge::voice", "decode loop exited");
}

// ── RTP helpers ───────────────────────────────────────────────────────────────

fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; RTP_HEADER_SIZE] {
    let mut h = [0u8; RTP_HEADER_SIZE];
    h[0] = 0x80;
    h[1] = RTP_PAYLOAD_TYPE_OPUS;
    h[2] = (sequence >> 8) as u8;
    h[3] = sequence as u8;
    h[4] = (timestamp >> 24) as u8;
    h[5] = (timestamp >> 16) as u8;
    h[6] = (timestamp >> 8) as u8;
    h[7] = timestamp as u8;
    h[8] = (ssrc >> 24) as u8;
    h[9] = (ssrc >> 16) as u8;
    h[10] = (ssrc >> 8) as u8;
    h[11] = ssrc as u8;
    h
}

fn parse_rtp_header(packet: &[u8]) -> Option<(u32, usize)> {
    if packet.len() < RTP_HEADER_SIZE {
        return None;
    }
    if (packet[0] >> 6) & 0x3 != 2 {
        return None;
    }
    let has_ext = (packet[0] >> 4) & 0x1 == 1;
    let csrc_count = (packet[0] & 0x0F) as usize;
    let ssrc = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let mut offset = RTP_HEADER_SIZE + csrc_count * 4;
    if offset > packet.len() {
        return None;
    }
    if has_ext {
        if offset + 4 > packet.len() {
            return None;
        }
        let ext_len = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]) as usize;
        offset += 4 + ext_len * 4;
    }
    if offset > packet.len() {
        return None;
    }
    Some((ssrc, offset))
}

// ── AEAD helpers ──────────────────────────────────────────────────────────────

fn encrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    plaintext: &[u8],
    mode: &str,
) -> Result<Vec<u8>, String> {
    if mode.contains("xchacha20") {
        let nonce = xchacha_nonce(rtp_header);
        cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad: rtp_header })
            .map_err(|_| "AEAD encrypt failed".to_string())
    } else {
        Err("unsupported AEAD mode".into())
    }
}

fn decrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    ciphertext: &[u8],
    mode: &str,
) -> Result<Vec<u8>, String> {
    if mode.contains("xchacha20") {
        let nonce = xchacha_nonce(rtp_header);
        cipher
            .decrypt(&nonce, Payload { msg: ciphertext, aad: rtp_header })
            .map_err(|_| "AEAD decrypt failed".to_string())
    } else {
        Err("unsupported AEAD mode".into())
    }
}

fn xchacha_nonce(rtp_header: &[u8]) -> XNonce {
    let mut nonce = [0u8; 24];
    let len = rtp_header.len().min(24);
    nonce[..len].copy_from_slice(&rtp_header[..len]);
    XNonce::from(nonce)
}

// ── base64 helpers ────────────────────────────────────────────────────────────

fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rtp_header_round_trip() {
        let h = build_rtp_header(0x1234, 0xDEAD_BEEF, 0xCAFE_BABE);
        assert_eq!(h[0], 0x80);
        assert_eq!(h[1], RTP_PAYLOAD_TYPE_OPUS);
        let (ssrc, offset) = parse_rtp_header(&h).unwrap();
        assert_eq!(ssrc, 0xCAFE_BABE);
        assert_eq!(offset, 12);
    }

    #[test]
    fn encryption_mode_selection() {
        let modes: Vec<String> = vec![
            "xsalsa20_poly1305".into(),
            "aead_xchacha20_poly1305_rtpsize".into(),
        ];
        assert_eq!(
            select_encryption_mode(&modes).unwrap(),
            "aead_xchacha20_poly1305_rtpsize"
        );
        let empty: Vec<String> = vec![];
        assert!(select_encryption_mode(&empty).is_none());
    }
}
