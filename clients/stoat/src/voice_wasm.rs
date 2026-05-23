//! Stoat (Vortex) voice transport — WASM target.
//!
//! Sibling to `clients/stoat/src/voice.rs` (native). This file is
//! `#[cfg(target_arch = "wasm32")]` at the module declaration in `lib.rs`,
//! mirroring the discord `voice_bridge.rs` pattern.
//!
//! # Protocol
//!
//! 1. POST `{base_url}/channels/{channel_id}/join_call` → `VortexServerInfo`
//! 2. Open `gloo_net::websocket::futures::WebSocket` to the returned WS URL.
//! 3. Receive `{"type":"Authenticated","user_id":"…"}` on connect (server sends it).
//! 4. Spawn encode loop: mic PCM (from B.3) → OpusClient encode → 8-byte uid prefix + opus bytes → WS binary.
//! 5. Spawn decode loop: WS binary → strip 8-byte uid → OpusClient decode → push_pcm (B.4).
//! 6. Spawn event loop: WS text → parse VortexEvent → emit ClientEvent.
//!
//! # Frame wire format (Vortex)
//!
//! Binary WS frame: `[8 bytes user_id, ASCII null-padded][opus bytes]`
//! Locally-sent frames use 8 NUL bytes for the local user_id (matches native voice.rs:393).
//!
//! # Shutdown
//!
//! `StoatVoiceConnection::disconnect()` sets an `Arc<AtomicBool>` that all
//! three spawned tasks poll via `futures::select!` on each iteration.
//!
//! # wasm32 !Send
//!
//! `gloo_net::websocket::futures::WebSocket` is `!Send` on wasm32. We wrap it
//! in the connection struct with `unsafe impl Send` (cf. discord WsHandle ~L1110).
//! wasm32 is single-threaded; no cross-thread transfer is possible at runtime.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use super::voice_noise_filter::{apply_rnnoise, NoiseFilter};

use futures::{
    channel::mpsc::{self, UnboundedSender},
    select,
    stream::StreamExt,
    FutureExt, SinkExt,
};
use gloo_net::websocket::{futures::WebSocket, Message as WsMessage};
use poly_host_bridge::codec_opus_client::OpusClient;
use tracing::{info, warn};
use wasm_bindgen_futures::spawn_local;

use poly_client::{ClientEvent, VoiceParticipant};

use super::voice_common::{
    StoatVoiceError, TransmitMode, VortexServerInfo, OPUS_FRAME_SAMPLES,
};

// ── Live connection handle ────────────────────────────────────────────────────

/// A live Stoat voice connection on wasm32.
///
/// The three background tasks (WS event pump, encode loop, decode loop) check
/// `shutdown` on every iteration and stop when it becomes `true`.
///
/// Drop or call [`StoatVoiceConnection::disconnect`] to stop all tasks.
pub struct StoatVoiceConnection {
    /// Channel ID of the joined voice channel.
    pub channel_id: String,
    /// Set to `true` to signal all tasks to stop.
    shutdown: Arc<AtomicBool>,
    /// Sender for pushing outbound WS messages from the encode loop.
    ws_tx: UnboundedSender<WsMessage>,
}

// SAFETY: wasm32 is single-threaded — no cross-thread transfer is possible.
// The `IsBackend` trait surface requires Send + Sync, satisfied here only for
// wasm32 via these unsafe impls. Mirrors discord WsHandle (voice_bridge.rs ~L1110).
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
unsafe impl Send for StoatVoiceConnection {}
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
unsafe impl Sync for StoatVoiceConnection {}

impl StoatVoiceConnection {
    /// Signal all background tasks to stop and send a Vortex `Leave` message.
    pub fn disconnect(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let leave = serde_json::json!({"type": "Leave"}).to_string();
        // Best-effort; the WS write task may have already exited.
        let _ = self.ws_tx.unbounded_send(WsMessage::Text(leave));
    }
}

// ── Main entrypoint ───────────────────────────────────────────────────────────

/// Connect to a Stoat voice channel (WASM path).
///
/// 1. POST `{base_url}/channels/{channel_id}/join_call` with `Authorization: Bearer {auth_token}`.
/// 2. Parse `VortexServerInfo` from the JSON response.
/// 3. Open a `gloo_net::websocket::futures::WebSocket` to the WS URL.
/// 4. Spawn encode, decode, and event loops.
///
/// `noise_cancel_enabled` — shared `Arc<AtomicBool>` from `StoatClient::voice_noise_cancel`.
/// Written to at runtime via `StoatClient::set_noise_cancel`.  The encode loop reads it on
/// each 480-sample RNNoise chunk; toggling takes effect immediately with no gap.
///
/// Returns a `StoatVoiceConnection` handle. Call `.disconnect()` to leave.
pub async fn connect_voice_wasm(
    channel_id: String,
    base_url: String,
    auth_token: String,
    transmit_mode: Option<TransmitMode>,
    noise_cancel_enabled: Arc<AtomicBool>,
    event_tx: mpsc::UnboundedSender<ClientEvent>,
) -> Result<StoatVoiceConnection, StoatVoiceError> {
    // ── Step 1: POST /channels/{channel_id}/join_call ─────────────────────────
    let join_url = format!("{}/channels/{}/join_call", base_url.trim_end_matches('/'), channel_id);

    let resp = gloo_net::http::Request::post(&join_url)
        .header("Authorization", &format!("Bearer {auth_token}"))
        .header("Content-Type", "application/json")
        .body("{}")
        .map_err(|e| StoatVoiceError::JoinCallFailed(format!("build request: {e:?}")))?
        .send()
        .await
        .map_err(|e| StoatVoiceError::JoinCallFailed(format!("HTTP send: {e:?}")))?;

    if !resp.ok() {
        return Err(StoatVoiceError::JoinCallFailed(format!(
            "HTTP {} from join_call",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| StoatVoiceError::JoinCallFailed(format!("JSON parse: {e:?}")))?;

    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| StoatVoiceError::JoinCallFailed("missing 'token' in join_call response".into()))?
        .to_string();

    let ws_url_raw = body
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| StoatVoiceError::JoinCallFailed("missing 'url' in join_call response".into()))?
        .to_string();

    let server_info = VortexServerInfo {
        token,
        ws_url: ws_url_raw,
        channel_id: channel_id.clone(),
    };

    // ── Step 2: Open the Vortex WebSocket ─────────────────────────────────────
    let ws = WebSocket::open(&server_info.ws_url)
        .map_err(|e| StoatVoiceError::WsConnect(format!("{e:?}")))?;

    info!(channel_id = %channel_id, url = %server_info.ws_url, "Stoat WASM voice WS connected");

    let (mut ws_sink, ws_source) = ws.split();

    // ── Step 3: Opus encoder session via host-bridge ──────────────────────────
    let opus = OpusClient::from_origin();

    let encoder_session = opus
        .encoder_create(48_000, 1, "voip")
        .await
        .map_err(|e| StoatVoiceError::Opus(format!("encoder_create: {e}")))?;

    // ── Shutdown flag + channels ──────────────────────────────────────────────
    let shutdown = Arc::new(AtomicBool::new(false));

    // Unbounded channel: encode/event loops → WS write task.
    let (ws_tx, ws_rx) = mpsc::unbounded::<WsMessage>();

    // ── WS write task ─────────────────────────────────────────────────────────
    {
        let shutdown_w = Arc::clone(&shutdown);
        let mut ws_rx = ws_rx;

        spawn_local(async move {
            loop {
                if shutdown_w.load(Ordering::Relaxed) {
                    break;
                }
                match ws_rx.next().await {
                    None => break,
                    Some(msg) => {
                        if ws_sink.send(msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = ws_sink.close().await;
        });
    }

    // ── WS event + decode task ────────────────────────────────────────────────
    {
        let channel_id_ev = channel_id.clone();
        let event_tx_ev = event_tx.clone();
        let opus_ev = OpusClient::from_origin();
        let shutdown_ev = Arc::clone(&shutdown);
        let mut ws_source = ws_source;
        // Per-user decoder sessions: user_id → session_id.
        let mut decoder_sessions: HashMap<String, String> = HashMap::new();
        let mut first_frame_logged = false;

        spawn_local(async move {
            loop {
                if shutdown_ev.load(Ordering::Relaxed) {
                    break;
                }

                // Drive the WS source to completion (no select! timeout needed;
                // shutdown flag is polled at loop top).
                let msg = {
                    let next = ws_source.next().fuse();
                    futures::pin_mut!(next);
                    // We use a plain `select!` to interleave shutdown checks.
                    select! {
                        m = next => m,
                        complete => break,
                    }
                };

                match msg {
                    None => break,
                    Some(Err(e)) => {
                        warn!("Stoat WASM voice WS error: {e:?}");
                        break;
                    }
                    Some(Ok(WsMessage::Text(text))) => {
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(json) => {
                                handle_vortex_event(
                                    &json,
                                    &channel_id_ev,
                                    &mut decoder_sessions,
                                    &opus_ev,
                                    &event_tx_ev,
                                )
                                .await;
                            }
                            Err(e) => {
                                warn!("Stoat WASM voice: JSON parse error: {e}");
                            }
                        }
                    }
                    Some(Ok(WsMessage::Bytes(bytes))) => {
                        // Binary frame: 8-byte ASCII user_id (null-padded) + Opus payload.
                        if bytes.len() <= 8 {
                            continue;
                        }
                        let uid_raw = &bytes[..8];
                        let user_id = std::str::from_utf8(uid_raw)
                            .unwrap_or("")
                            .trim_end_matches('\0')
                            .to_string();
                        let opus_bytes = &bytes[8..];

                        if !first_frame_logged {
                            info!(channel_id = %channel_id_ev, user_id = %user_id, "Stoat WASM voice: first binary frame received");
                            first_frame_logged = true;
                        }

                        // B.5: per-user decoder cache.
                        let session_id = match decoder_sessions.get(&user_id) {
                            Some(id) => id.clone(),
                            None => {
                                match opus_ev.decoder_create(48_000, 1).await {
                                    Ok(sid) => {
                                        decoder_sessions.insert(user_id.clone(), sid.clone());
                                        sid
                                    }
                                    Err(e) => {
                                        warn!("Stoat WASM voice: decoder_create for user {user_id}: {e}");
                                        continue;
                                    }
                                }
                            }
                        };

                        match opus_ev.decode(&session_id, opus_bytes).await {
                            Ok(pcm) => {
                                super::voice_wasm_audio_playback::push_pcm(&user_id, pcm);
                            }
                            Err(e) => {
                                warn!("Stoat WASM voice: decode error for user {user_id}: {e}");
                            }
                        }
                    }
                }
            }

            // Cleanup: destroy all decoder sessions.
            for (uid, sid) in &decoder_sessions {
                if let Err(e) = opus_ev.close(&sid).await {
                    warn!("Stoat WASM voice: decoder close for user {uid}: {e}");
                }
            }
        });
    }

    // ── Encode task ───────────────────────────────────────────────────────────
    {
        let ws_tx_enc = ws_tx.clone();
        let opus_enc = OpusClient::from_origin();
        let encoder_session_enc = encoder_session.clone();
        let shutdown_enc = Arc::clone(&shutdown);
        let transmit = transmit_mode.unwrap_or_default();
        // B.8 — thread the noise-cancel flag into the audio-capture task.
        let noise_cancel_enc = Arc::clone(&noise_cancel_enabled);

        spawn_local(async move {
            // Open the mic stream (Phase B.3 / B.8).
            let mut mic_stream =
                match super::voice_wasm_audio_capture::open_mic_stream(noise_cancel_enc).await {
                    Ok(stream) => Box::pin(stream),
                    Err(e) => {
                        warn!("Stoat WASM voice: mic open failed: {e}");
                        return;
                    }
                };

            let mut buf: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);

            loop {
                if shutdown_enc.load(Ordering::Relaxed) {
                    break;
                }

                let frame = mic_stream.next().await;
                let Some(samples) = frame else { break };
                buf.extend_from_slice(&samples);

                while buf.len() >= OPUS_FRAME_SAMPLES {
                    let pcm_slice = &buf[..OPUS_FRAME_SAMPLES];
                    if transmit.should_transmit(pcm_slice) {
                        match opus_enc.encode(&encoder_session_enc, pcm_slice).await {
                            Ok(opus_bytes) => {
                                // Wire format: 8 NUL bytes (local user_id) + opus bytes.
                                let mut frame_bytes = vec![0u8; 8];
                                frame_bytes.extend_from_slice(&opus_bytes);
                                if ws_tx_enc.unbounded_send(WsMessage::Bytes(frame_bytes)).is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Stoat WASM voice: encode error: {e}");
                            }
                        }
                    }
                    buf.drain(..OPUS_FRAME_SAMPLES);
                }
            }

            // Destroy the encoder session.
            if let Err(e) = opus_enc.close(&encoder_session_enc).await {
                warn!("Stoat WASM voice: encoder close: {e}");
            }
        });
    }

    Ok(StoatVoiceConnection {
        channel_id,
        shutdown,
        ws_tx,
    })
}

// ── Vortex event handler ──────────────────────────────────────────────────────

/// Parse and dispatch a text JSON event from the Vortex WS.
///
/// Matches the native `handle_vortex_event` logic in `voice.rs:334-447`.
async fn handle_vortex_event(
    json: &serde_json::Value,
    channel_id: &str,
    decoder_sessions: &mut HashMap<String, String>,
    opus: &OpusClient,
    event_tx: &mpsc::UnboundedSender<ClientEvent>,
) {
    let Some(ev_type) = json.get("type").and_then(|t| t.as_str()) else {
        return;
    };

    match ev_type {
        "Authenticated" => {
            let user_id = json.get("user_id").and_then(|v| v.as_str()).unwrap_or("?");
            info!(channel_id, user_id, "Stoat WASM voice: authenticated on Vortex WS");
        }

        "VoiceParticipantJoined" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };
            let display_name = json
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(user_id)
                .to_string();
            let avatar_url = json
                .get("avatar_url")
                .and_then(|v| v.as_str())
                .map(str::to_string);

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

            let _ = event_tx.unbounded_send(ClientEvent::VoiceUserJoined {
                channel_id: channel_id.to_string(),
                participant,
            });
        }

        "VoiceParticipantLeft" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };

            // B.5: evict the per-user decoder session on participant leave.
            if let Some(session_id) = decoder_sessions.remove(user_id) {
                if let Err(e) = opus.close(&session_id).await {
                    warn!("Stoat WASM voice: decoder close on participant left (user={user_id}): {e}");
                }
            }

            // Tear down the per-user AudioContext in the playback pump (B.4).
            super::voice_wasm_audio_playback::drop_user(user_id);

            let _ = event_tx.unbounded_send(ClientEvent::VoiceUserLeft {
                channel_id: channel_id.to_string(),
                user_id: user_id.to_string(),
            });
        }

        "SpeakingUpdate" => {
            let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) else {
                return;
            };
            let is_speaking = json.get("speaking").and_then(|v| v.as_bool()).unwrap_or(false);
            let _ = event_tx.unbounded_send(ClientEvent::VoiceSpeakingUpdate {
                channel_id: channel_id.to_string(),
                user_id: user_id.to_string(),
                is_speaking,
            });
        }

        other => {
            tracing::debug!("Stoat WASM voice: unhandled WS event type: {other}");
        }
    }
}
