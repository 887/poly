//! Shared voice constants, types, and utilities for Stoat voice transport.
//!
//! This module is **cfg-free** — it compiles on both native and `wasm32-unknown-unknown`.
//! It contains only pure-Rust definitions that depend on `std`, `thiserror`,
//! `serde_json`, and nothing platform-specific (`audiopus`, `tokio_tungstenite`,
//! `gloo_net`, `tokio` runtime, etc.).
//!
//! Both `clients/stoat/src/voice.rs` (native) and `clients/stoat/src/voice_wasm.rs`
//! (WASM, Phase B) import from here so that constants, the error enum, and
//! `TransmitMode` stay in one place.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

// ── Constants ─────────────────────────────────────────────────────────────────

/// 20 ms frame at 48 kHz mono = 960 i16 samples.
pub const OPUS_FRAME_SAMPLES: usize = 960;

/// Default VAD threshold (-45 dB RMS).
pub const DEFAULT_VAD_THRESHOLD_DB: f32 = -45.0;

/// Maximum decoded PCM samples per Opus frame (120ms @ 48kHz mono).
pub const OPUS_MAX_DECODE_SAMPLES: usize = 5760;

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

    #[error("audio init failed: {0}")]
    AudioInit(String),
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
    /// Returns `true` if the current mode allows transmitting the given PCM frame.
    pub fn should_transmit(&self, pcm: &[i16]) -> bool {
        match self {
            Self::Vad { threshold_db } => rms_db(pcm) >= *threshold_db,
            Self::PushToTalk { active } => active.load(Ordering::Relaxed),
        }
    }
}

/// Compute the RMS level in dBFS for an i16 PCM slice.
pub fn rms_db(pcm: &[i16]) -> f32 {
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
