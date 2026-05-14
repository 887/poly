//! Stoat voice transport — Phase F of `docs/plans/plan-voice-video-calls.md`.
//!
//! # Architecture
//!
//! Stoat (Revolt) uses a custom voice service called **Vortex** that has evolved
//! over time. The test-stoat mock simulates a minimal subset sufficient for
//! smoke-testing the wiring. The real Stoat/Revolt protocol is documented in
//! `docs/dev/stoat-voice-protocol.md` (Phase F.2).
//!
//! ```text
//! StoatVoiceConnection
//!   ├── signaling WS (vortex_ws_loop)  ← join/leave/participant events
//!   ├── Opus encoder (audiopus) — mic PCM → Opus binary WS frames
//!   └── Opus decoder (audiopus) — binary WS frames → speaker PCM
//! ```
//!
//! # WASM safety
//!
//! This entire module is `#[cfg(feature = "voice")]`. The `voice` feature
//! requires `native`. WASM builds of `poly-stoat` MUST NOT enable `voice`.
//!
//! # Test-mock protocol
//!
//! The test-stoat mock implements:
//! - `POST /channels/{id}/join_call` → `{ "token": "<jwt>", "url": "ws://..." }`
//! - `GET /vortex/ws?token=<token>` — WebSocket that:
//!   1. Sends `{"type":"Authenticated","user_id":"<id>"}` on connect.
//!   2. Sends `VoiceParticipantJoined` for a fake "raccoon" participant 100ms later.
//!   3. Echoes back any binary Opus frame it receives (loopback).
//!   4. Accepts `{"type":"Leave"}` to close cleanly.
//!
//! See `servers/test-stoat/src/routes.rs` for the handler code.

// lint-allow-unused: byte slice indexing is locally length-checked; all slices are locally bounded
#![allow(clippy::indexing_slicing)]
// lint-allow-unused: voice transport code; all unwrap/expect are on infallible paths (codec init, mutex); panic! not used
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use audiopus::{
    coder::{Decoder as OpusDecoder, Encoder as OpusEncoder},
    Application as OpusApplication, Channels as OpusChannels, MutSignals,
    packet::Packet,
    SampleRate as OpusSampleRate,
};
use futures::{SinkExt, StreamExt};
use tokio::{
    sync::{broadcast, mpsc, Mutex as TokioMutex},
};
use tokio_tungstenite::{connect_async, tungstenite::Message as TMsg};
use tracing::{debug, info, warn};

use poly_audio_backend::{AudioFormat};
use poly_client::{ClientEvent, VoiceParticipant};

// ── Constants ─────────────────────────────────────────────────────────────────

/// 20 ms frame at 48 kHz mono = 960 i16 samples.
const OPUS_FRAME_SAMPLES: usize = 960;

/// Opus application mode for voice.
const OPUS_APP: OpusApplication = OpusApplication::Voip;

/// Default VAD threshold (-45 dB RMS).
const DEFAULT_VAD_THRESHOLD_DB: f32 = -45.0;

/// Maximum decoded PCM samples per Opus frame (120ms @ 48kHz mono).
const OPUS_MAX_DECODE_SAMPLES: usize = 5760;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors produced by the Stoat voice transport.
#[derive(Debug, thiserror::Error)]
pub enum StoatVoiceError {
    #[error("voice WebSocket connect failed: {0}")]
    WsConnect(String),

    #[error("join_call REST request failed: {0}")]
    JoinCallFailed(String),

    #[error("Opus codec error: {0}")]
    Opus(String),

    #[error("account already has an active voice connection (anti-rate-limit F.8)")]
    AlreadyConnected,

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

// ── Transmit mode ─────────────────────────────────────────────────────────────

/// Controls when the local user transmits audio.
#[derive(Debug, Clone)]
pub enum TransmitMode {
    /// Voice-activity detection: transmit when RMS exceeds `threshold_db` (-45 dB default).
    Vad { threshold_db: f32 },
    /// Push-to-talk: transmit only when `active` is `true`.
    PushToTalk { active: Arc<AtomicBool> },
}

impl Default for TransmitMode {
    fn default() -> Self {
        Self::Vad {
            threshold_db: DEFAULT_VAD_THRESHOLD_DB,
        }
    }
}

impl TransmitMode {
    fn should_transmit(&self, pcm: &[i16]) -> bool {
        match self {
            Self::Vad { threshold_db } => rms_db(pcm) >= *threshold_db,
            Self::PushToTalk { active } => active.load(Ordering::Relaxed),
        }
    }
}

/// Compute the RMS level in dBFS for an i16 PCM slice.
fn rms_db(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return -96.0;
    }
    let sum_sq: f64 = pcm.iter().map(|&s| {
        let f = f64::from(s) / 32768.0;
        f * f
    }).sum();
    let rms = (sum_sq / pcm.len() as f64).sqrt();
    if rms < 1e-10 {
        return -96.0;
    }
    (20.0 * rms.log10()) as f32
}

// ── Server info returned by join_call REST ────────────────────────────────────

/// Information returned by `POST /channels/{id}/join_call`.
#[derive(Debug, Clone)]
pub struct VortexServerInfo {
    /// JWT / bearer token for authenticating the Vortex WebSocket.
    pub token: String,
    /// WebSocket URL for the Vortex server.
    pub ws_url: String,
    /// The voice channel ID.
    pub channel_id: String,
}

// ── Live connection handle ────────────────────────────────────────────────────

/// A live Stoat voice connection.
///
/// Dropping calls [`StoatVoiceConnection::disconnect`] implicitly via the
/// shutdown broadcast.
pub struct StoatVoiceConnection {
    /// Channel ID of the joined voice channel.
    pub channel_id: String,
    /// Live participant state (user_id → VoiceParticipant).
    pub participants: Arc<TokioMutex<HashMap<String, VoiceParticipant>>>,
    /// Send `true` to tell all tasks to stop.
    shutdown_tx: broadcast::Sender<bool>,
}

impl StoatVoiceConnection {
    /// Disconnect from the voice channel (sends shutdown signal to all tasks).
    pub fn disconnect(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Snapshot the current participant list.
    pub async fn get_participants(&self) -> Vec<VoiceParticipant> {
        self.participants.lock().await.values().cloned().collect()
    }
}

// ── Per-account voice mutex (F.8) ─────────────────────────────────────────────

/// A per-account voice connection lock.
///
/// Holds `Some(StoatVoiceConnection)` while a session is active. A second
/// `connect_voice` call returns [`StoatVoiceError::AlreadyConnected`] without
/// opening any WebSocket — the anti-rate-limit guardrail for voice (Phase F.8).
pub type VoiceSessionGuard = Arc<TokioMutex<Option<StoatVoiceConnection>>>;

// ── Main connect entrypoint ───────────────────────────────────────────────────

/// Connect to a Stoat voice channel.
///
/// 1. Connects to the Vortex WebSocket at `server_info.ws_url`.
/// 2. Authenticates with `server_info.token`.
/// 3. Spawns the WS event loop (participant-join, speaking, state updates).
/// 4. Spawns the encode loop (mic PCM → Opus → WS binary).
/// 5. Spawns the decode loop (WS binary → Opus → speaker PCM).
///
/// # Errors
///
/// Returns [`StoatVoiceError::AlreadyConnected`] if the guard already holds
/// an active connection. Call [`disconnect_voice`] first.
pub async fn connect_voice(
    guard: VoiceSessionGuard,
    server_info: VortexServerInfo,
    audio: &dyn poly_audio_backend::AudioBackend,
    transmit_mode: Option<TransmitMode>,
    event_tx: mpsc::Sender<ClientEvent>,
) -> Result<(), StoatVoiceError> {
    // F.8 — single voice connection per account.
    let mut guard_lock = guard.lock().await;
    if guard_lock.is_some() {
        return Err(StoatVoiceError::AlreadyConnected);
    }

    let channel_id = server_info.channel_id.clone();
    let token = server_info.token.clone();
    let ws_url = server_info.ws_url.clone();

    // Connect the Vortex WebSocket.
    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| StoatVoiceError::WsConnect(e.to_string()))?;

    info!(channel_id = %channel_id, url = %ws_url, "Stoat voice WS connected");

    let (mut ws_sink, mut ws_source) = ws_stream.split();

    // Authenticate on the WS.
    let auth_msg = serde_json::json!({ "type": "Authenticate", "token": token });
    ws_sink.send(TMsg::Text(auth_msg.to_string().into()))
        .await
        .map_err(|e| StoatVoiceError::WsConnect(e.to_string()))?;

    // Open audio streams.
    let mic_stream = audio
        .open_input("", AudioFormat::STOAT_VOICE)
        .await
        .map_err(|e| StoatVoiceError::Opus(e.to_string()))?;
    let speaker = audio
        .open_output("", AudioFormat::STOAT_VOICE)
        .await
        .map_err(|e| StoatVoiceError::Opus(e.to_string()))?;

    // Broadcast channel: send `true` to stop all tasks.
    let (shutdown_tx, _) = broadcast::channel::<bool>(4);

    let participants = Arc::new(TokioMutex::new(HashMap::<String, VoiceParticipant>::new()));

    // Channel: encode loop → ws_write task.
    let (ws_write_tx, mut ws_write_rx) = mpsc::channel::<TMsg>(64);

    // ── WS write loop ─────────────────────────────────────────────────────────
    {
        let mut sd = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = ws_write_rx.recv() => {
                        match msg {
                            None => break,
                            Some(m) => { if ws_sink.send(m).await.is_err() { break; } }
                        }
                    }
                    _ = sd.recv() => break,
                }
            }
            drop(ws_sink.close().await);
            debug!("Stoat voice WS write loop stopped");
        });
    }

    // ── WS event loop ─────────────────────────────────────────────────────────
    {
        let ch_ev = channel_id.clone();
        let parts_ev = Arc::clone(&participants);
        let ev_tx = event_tx.clone();
        let ws_tx_ev = ws_write_tx.clone();
        let speaker_arc = Arc::new(speaker);
        let mut sd = shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut decoders: HashMap<String, OpusDecoder> = HashMap::new();

            loop {
                tokio::select! {
                    msg = ws_source.next() => {
                        match msg {
                            None | Some(Err(_)) => break,
                            Some(Ok(TMsg::Close(_))) => break,
                            Some(Ok(TMsg::Text(text))) => {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                    handle_vortex_event(
                                        &json,
                                        &ch_ev,
                                        &parts_ev,
                                        &ev_tx,
                                    ).await;
                                }
                            }
                            Some(Ok(TMsg::Binary(bytes))) => {
                                // Binary frame: Opus audio from a remote participant.
                                // Frame format (test-stoat mock): 8-byte ASCII user_id (null-padded) + Opus payload.
                                if bytes.len() <= 8 { continue; }
                                let uid_raw = &bytes[..8];
                                let user_id = String::from_utf8_lossy(uid_raw)
                                    .trim_end_matches('\0')
                                    .to_string();
                                let opus_data = &bytes[8..];

                                let decoder = decoders.entry(user_id.clone()).or_insert_with(|| {
                                    OpusDecoder::new(OpusSampleRate::Hz48000, OpusChannels::Mono)
                                        .expect("Stoat voice: Opus decoder init")
                                });

                                let mut pcm = vec![0i16; OPUS_MAX_DECODE_SAMPLES];
                                let packet = match Packet::try_from(opus_data) {
                                    Ok(p) => p,
                                    Err(_) => continue,
                                };
                                let mut_signals = match MutSignals::try_from(&mut pcm[..]) {
                                    Ok(s) => s,
                                    Err(_) => continue,
                                };
                                let decoded = match decoder.decode(
                                    Some(packet),
                                    mut_signals,
                                    false,
                                ) {
                                    Ok(n) => n,
                                    Err(e) => {
                                        debug!("Stoat voice decode error (user={user_id}): {e:?}");
                                        continue;
                                    }
                                };
                                pcm.truncate(decoded);
                                drop(speaker_arc.push(&pcm).await);
                            }
                            _ => {}
                        }
                    }
                    _ = sd.recv() => break,
                }
            }

            // Send Leave before dropping.
            let leave = serde_json::json!({ "type": "Leave" });
            drop(ws_tx_ev.send(TMsg::Text(leave.to_string().into())).await);
            debug!("Stoat voice WS event loop stopped");
        });
    }

    // ── Encode loop ───────────────────────────────────────────────────────────
    {
        let transmit = transmit_mode.unwrap_or_default();
        let ws_tx_enc = ws_write_tx;
        let mut sd = shutdown_tx.subscribe();

        tokio::spawn(async move {
            let encoder = match OpusEncoder::new(
                OpusSampleRate::Hz48000,
                OpusChannels::Mono,
                OPUS_APP,
            ) {
                Ok(e) => e,
                Err(e) => {
                    warn!("Stoat voice: Opus encoder init failed: {e:?}");
                    return;
                }
            };

            let mut mic = mic_stream;
            let mut buf: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);

            loop {
                let frame = tokio::select! {
                    f = mic.next() => f,
                    _ = sd.recv() => break,
                };
                let Some(samples) = frame else { break };
                buf.extend_from_slice(&samples);

                while buf.len() >= OPUS_FRAME_SAMPLES {
                    let pcm_slice = &buf[..OPUS_FRAME_SAMPLES];
                    if transmit.should_transmit(pcm_slice) {
                        let mut opus_out = vec![0u8; 4000];
                        match encoder.encode(pcm_slice, &mut opus_out) {
                            Ok(n) => {
                                opus_out.truncate(n);
                                // Prepend 8 bytes of zero user_id (local user).
                                let mut frame_bytes = vec![0u8; 8];
                                frame_bytes.extend_from_slice(&opus_out);
                                if ws_tx_enc.send(TMsg::Binary(frame_bytes.into())).await.is_err() {
                                    return;
                                }
                            }
                            Err(e) => debug!("Stoat voice encode error: {e:?}"),
                        }
                    }
                    buf.drain(..OPUS_FRAME_SAMPLES);
                }
            }
            debug!("Stoat voice encode loop stopped");
        });
    }

    *guard_lock = Some(StoatVoiceConnection {
        channel_id,
        participants: Arc::clone(&participants),
        shutdown_tx,
    });
    Ok(())
}

/// Handle a JSON event received from the Vortex WebSocket.
async fn handle_vortex_event(
    json: &serde_json::Value,
    channel_id: &str,
    participants: &TokioMutex<HashMap<String, VoiceParticipant>>,
    event_tx: &mpsc::Sender<ClientEvent>,
) {
    let Some(ev_type) = json.get("type").and_then(|t| t.as_str()) else {
        return;
    };

    match ev_type {
        "VoiceParticipantJoined" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };
            let display_name = json.get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(user_id)
                .to_string();
            let avatar_url = json.get("avatar_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let participant = VoiceParticipant {
                user: poly_client::User {
                    id: user_id.to_string(),
                    display_name,
                    avatar_url,
                    presence: poly_client::PresenceStatus::Online,
                    backend: poly_client::BackendType::from(crate::SLUG),
                },
                is_muted: json.get("is_muted").and_then(|v| v.as_bool()).unwrap_or(false),
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            };

            participants.lock().await.insert(user_id.to_string(), participant.clone());
            debug!(channel_id, user_id, "Stoat voice: participant joined");

            let _ = event_tx.send(ClientEvent::VoiceUserJoined {
                channel_id: channel_id.to_string(),
                participant,
            }).await;
        }

        "VoiceParticipantLeft" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };
            participants.lock().await.remove(user_id);
            debug!(channel_id, user_id, "Stoat voice: participant left");
            let _ = event_tx.send(ClientEvent::VoiceUserLeft {
                channel_id: channel_id.to_string(),
                user_id: user_id.to_string(),
            }).await;
        }

        "SpeakingUpdate" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };
            let is_speaking = json.get("speaking").and_then(|v| v.as_bool()).unwrap_or(false);
            if let Some(p) = participants.lock().await.get_mut(user_id) {
                p.is_speaking = is_speaking;
            }
            let _ = event_tx.send(ClientEvent::VoiceSpeakingUpdate {
                channel_id: channel_id.to_string(),
                user_id: user_id.to_string(),
                is_speaking,
            }).await;
        }

        "VoiceStateUpdated" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };
            let mut guard = participants.lock().await;
            if let Some(p) = guard.get_mut(user_id) {
                if let Some(v) = json.get("is_muted").and_then(|v| v.as_bool()) {
                    p.is_muted = v;
                }
                if let Some(v) = json.get("is_deafened").and_then(|v| v.as_bool()) {
                    p.is_deafened = v;
                }
                let updated = p.clone();
                drop(guard);
                let _ = event_tx.send(ClientEvent::VoiceStateUpdated {
                    channel_id: channel_id.to_string(),
                    participant: updated,
                }).await;
            }
        }

        "IncomingCall" => {
            // F.6 / H.3 — emit IncomingCall from Vortex WS events.
            let dm_id = json.get("dm_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let caller = json.get("caller_user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let with_video = json.get("with_video").and_then(|v| v.as_bool()).unwrap_or(false);
            if !dm_id.is_empty() && !caller.is_empty() {
                let _ = event_tx.send(ClientEvent::IncomingCall {
                    dm_id,
                    caller_user_id: caller,
                    with_video,
                }).await;
            }
        }

        _ => {
            debug!("Stoat voice: unhandled WS event type: {ev_type}");
        }
    }
}

/// Disconnect from the active Stoat voice channel.
///
/// Sends the shutdown broadcast to all tasks and releases the guard.
/// A new call to [`connect_voice`] is allowed after this returns.
pub async fn disconnect_voice(guard: VoiceSessionGuard) {
    let mut lock = guard.lock().await;
    if let Some(conn) = lock.take() {
        conn.disconnect();
    }
}

/// Snapshot participants from the active connection for the given channel.
///
/// Returns an empty vec if there is no active connection or the channel_id
/// doesn't match the current connection.
pub async fn get_voice_participants_cached(
    guard: &VoiceSessionGuard,
    channel_id: &str,
) -> Vec<VoiceParticipant> {
    let lock = guard.lock().await;
    match &*lock {
        Some(conn) if conn.channel_id == channel_id => {
            conn.participants.lock().await.values().cloned().collect()
        }
        _ => vec![],
    }
}
