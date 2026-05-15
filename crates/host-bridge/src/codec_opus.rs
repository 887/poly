//! # `/host/codec/opus/*` — generic Opus encode/decode service
//!
//! Exposes stateful Opus encoder and decoder sessions over HTTP. WASM targets
//! (which cannot link libopus FFI) call these endpoints through
//! [`crate::codec_opus_client::OpusClient`]; native callers can link audiopus
//! directly but may also use the bridge for consistency.
//!
//! ## Routes
//!
//! ```text
//! POST /host/codec/opus/encoder/create  { sample_rate, channels, application } -> { session_id }
//! POST /host/codec/opus/encoder/encode  { session_id, pcm: base64 (i16 LE) } -> { encoded: base64 }
//! POST /host/codec/opus/decoder/create  { sample_rate, channels } -> { session_id }
//! POST /host/codec/opus/decoder/decode  { session_id, encoded: base64 } -> { pcm: base64 (i16 LE) }
//! POST /host/codec/opus/close           { session_id }
//! ```
//!
//! ## WASM safety
//!
//! `#[cfg(all(not(target_arch = "wasm32"), feature = "codec-opus"))]`

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use audiopus::{
    Application as OpusApplication, Channels as OpusChannels, MutSignals,
    SampleRate as OpusSampleRate,
    coder::{Decoder as OpusDecoder, Encoder as OpusEncoder},
    packet::Packet,
};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use base64::Engine as _;
use uuid::Uuid;

// Wire types and route constants are defined in codec_opus_client (always compiled).
pub use crate::codec_opus_client::{
    OpusCloseRequest, OpusCloseResponse, OpusDecodeRequest, OpusDecodeResponse,
    OpusDecoderCreateRequest, OpusEncodeRequest, OpusEncodeResponse, OpusEncoderCreateRequest,
    OpusSessionCreateResponse, ROUTE_OPUS_CLOSE, ROUTE_OPUS_DECODER_CREATE,
    ROUTE_OPUS_DECODER_DECODE, ROUTE_OPUS_ENCODER_CREATE, ROUTE_OPUS_ENCODER_ENCODE,
};

// ── Session state ──────────────────────────────────────────────────────────────

enum OpusSession {
    Encoder(OpusEncoder),
    Decoder(OpusDecoder),
}

/// Shared state for the Opus codec service.
#[derive(Clone, Default)]
pub struct OpusState {
    sessions: Arc<Mutex<HashMap<String, OpusSession>>>,
}

impl OpusState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ── Router ─────────────────────────────────────────────────────────────────────

#[must_use]
pub fn router(state: OpusState) -> axum::Router {
    use axum::routing::post;
    axum::Router::new()
        .route(ROUTE_OPUS_ENCODER_CREATE, post(handle_encoder_create))
        .route(ROUTE_OPUS_ENCODER_ENCODE, post(handle_encode))
        .route(ROUTE_OPUS_DECODER_CREATE, post(handle_decoder_create))
        .route(ROUTE_OPUS_DECODER_DECODE, post(handle_decode))
        .route(ROUTE_OPUS_CLOSE, post(handle_close))
        .with_state(state)
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn handle_encoder_create(
    State(state): State<OpusState>,
    Json(req): Json<OpusEncoderCreateRequest>,
) -> impl IntoResponse {
    let sr = parse_sample_rate(req.sample_rate);
    let ch = parse_channels(req.channels);
    let app = parse_application(&req.application);

    let (sr, ch, app) = match (sr, ch, app) {
        (Some(s), Some(c), Some(a)) => (s, c, a),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(OpusSessionCreateResponse {
                    ok: false,
                    session_id: String::new(),
                    err: Some(format!(
                        "invalid params: sample_rate={} channels={} application={}",
                        req.sample_rate, req.channels, req.application
                    )),
                }),
            );
        }
    };

    let encoder = match OpusEncoder::new(sr, ch, app) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OpusSessionCreateResponse {
                    ok: false,
                    session_id: String::new(),
                    err: Some(format!("OpusEncoder::new: {e}")),
                }),
            );
        }
    };

    let session_id = Uuid::new_v4().to_string();
    state
        .sessions
        .lock()
        .unwrap()
        .insert(session_id.clone(), OpusSession::Encoder(encoder));

    (
        StatusCode::OK,
        Json(OpusSessionCreateResponse { ok: true, session_id, err: None }),
    )
}

async fn handle_encode(
    State(state): State<OpusState>,
    Json(req): Json<OpusEncodeRequest>,
) -> impl IntoResponse {
    let raw = match b64_decode(&req.pcm) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(OpusEncodeResponse {
                    ok: false,
                    encoded: String::new(),
                    err: Some(format!("invalid pcm base64: {e}")),
                }),
            );
        }
    };
    if raw.len() % 2 != 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(OpusEncodeResponse {
                ok: false,
                encoded: String::new(),
                err: Some("pcm byte length must be even".into()),
            }),
        );
    }
    let pcm: Vec<i16> = raw.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();

    let mut map = state.sessions.lock().unwrap();
    let encoder = match map.get_mut(&req.session_id) {
        Some(OpusSession::Encoder(e)) => e,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(OpusEncodeResponse {
                    ok: false,
                    encoded: String::new(),
                    err: Some(format!("encoder session {} not found", req.session_id)),
                }),
            );
        }
    };

    let mut out = vec![0u8; 4000];
    let n = match encoder.encode(&pcm, &mut out) {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OpusEncodeResponse {
                    ok: false,
                    encoded: String::new(),
                    err: Some(format!("Opus encode: {e}")),
                }),
            );
        }
    };

    (
        StatusCode::OK,
        Json(OpusEncodeResponse {
            ok: true,
            encoded: base64::engine::general_purpose::STANDARD.encode(&out[..n]),
            err: None,
        }),
    )
}

async fn handle_decoder_create(
    State(state): State<OpusState>,
    Json(req): Json<OpusDecoderCreateRequest>,
) -> impl IntoResponse {
    let sr = parse_sample_rate(req.sample_rate);
    let ch = parse_channels(req.channels);

    let (sr, ch) = match (sr, ch) {
        (Some(s), Some(c)) => (s, c),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(OpusSessionCreateResponse {
                    ok: false,
                    session_id: String::new(),
                    err: Some(format!(
                        "invalid params: sample_rate={} channels={}",
                        req.sample_rate, req.channels
                    )),
                }),
            );
        }
    };

    let decoder = match OpusDecoder::new(sr, ch) {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OpusSessionCreateResponse {
                    ok: false,
                    session_id: String::new(),
                    err: Some(format!("OpusDecoder::new: {e}")),
                }),
            );
        }
    };

    let session_id = Uuid::new_v4().to_string();
    state
        .sessions
        .lock()
        .unwrap()
        .insert(session_id.clone(), OpusSession::Decoder(decoder));

    (
        StatusCode::OK,
        Json(OpusSessionCreateResponse { ok: true, session_id, err: None }),
    )
}

async fn handle_decode(
    State(state): State<OpusState>,
    Json(req): Json<OpusDecodeRequest>,
) -> impl IntoResponse {
    let raw = match b64_decode(&req.encoded) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(OpusDecodeResponse {
                    ok: false,
                    pcm: String::new(),
                    err: Some(format!("invalid encoded base64: {e}")),
                }),
            );
        }
    };

    let mut map = state.sessions.lock().unwrap();
    let decoder = match map.get_mut(&req.session_id) {
        Some(OpusSession::Decoder(d)) => d,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(OpusDecodeResponse {
                    ok: false,
                    pcm: String::new(),
                    err: Some(format!("decoder session {} not found", req.session_id)),
                }),
            );
        }
    };

    // 120ms @ 48kHz stereo = 5760 samples max.
    let mut pcm_buf = vec![0i16; 5760 * 2];
    let packet = match Packet::try_from(raw.as_slice()) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(OpusDecodeResponse {
                    ok: false,
                    pcm: String::new(),
                    err: Some(format!("invalid Opus packet: {e}")),
                }),
            );
        }
    };
    let mut_signals = match MutSignals::try_from(pcm_buf.as_mut_slice()) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OpusDecodeResponse {
                    ok: false,
                    pcm: String::new(),
                    err: Some(format!("MutSignals: {e}")),
                }),
            );
        }
    };

    let n = match decoder.decode(Some(packet), mut_signals, false) {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OpusDecodeResponse {
                    ok: false,
                    pcm: String::new(),
                    err: Some(format!("Opus decode: {e}")),
                }),
            );
        }
    };

    // n = stereo sample pairs (each pair is 2 × i16 = 4 bytes).
    let pcm_slice = &pcm_buf[..n * 2];
    let mut pcm_bytes = Vec::with_capacity(pcm_slice.len() * 2);
    for &s in pcm_slice {
        pcm_bytes.extend_from_slice(&s.to_le_bytes());
    }

    (
        StatusCode::OK,
        Json(OpusDecodeResponse {
            ok: true,
            pcm: base64::engine::general_purpose::STANDARD.encode(&pcm_bytes),
            err: None,
        }),
    )
}

async fn handle_close(
    State(state): State<OpusState>,
    Json(req): Json<OpusCloseRequest>,
) -> impl IntoResponse {
    let removed = state
        .sessions
        .lock()
        .unwrap()
        .remove(&req.session_id);
    if removed.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(OpusCloseResponse {
                ok: false,
                err: Some(format!("session {} not found", req.session_id)),
            }),
        );
    }
    (StatusCode::OK, Json(OpusCloseResponse { ok: true, err: None }))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn parse_sample_rate(hz: u32) -> Option<OpusSampleRate> {
    match hz {
        8_000 => Some(OpusSampleRate::Hz8000),
        12_000 => Some(OpusSampleRate::Hz12000),
        16_000 => Some(OpusSampleRate::Hz16000),
        24_000 => Some(OpusSampleRate::Hz24000),
        48_000 => Some(OpusSampleRate::Hz48000),
        _ => None,
    }
}

fn parse_channels(n: u8) -> Option<OpusChannels> {
    match n {
        1 => Some(OpusChannels::Mono),
        2 => Some(OpusChannels::Stereo),
        _ => None,
    }
}

fn parse_application(s: &str) -> Option<OpusApplication> {
    match s {
        "voip" => Some(OpusApplication::Voip),
        "audio" => Some(OpusApplication::Audio),
        "low_delay" => Some(OpusApplication::LowDelay),
        _ => None,
    }
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample_rate_covers_all_valid() {
        for hz in [8000u32, 12000, 16000, 24000, 48000] {
            assert!(parse_sample_rate(hz).is_some(), "sample rate {hz} should parse");
        }
        assert!(parse_sample_rate(44100).is_none());
    }

    #[test]
    fn opus_wire_types_serialize() {
        let r = OpusEncodeResponse {
            ok: true,
            encoded: "AAAA".into(),
            err: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"encoded\":\"AAAA\""));
    }
}
