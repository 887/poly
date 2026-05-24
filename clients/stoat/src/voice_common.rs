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

// ── Vortex wire-format extension (A.5) ───────────────────────────────────────
//
// As of A.5 (Vortex protocol extension for video — see plan-stoat-video-wasm.md),
// all WASM-side Vortex binary WS frames carry a 1-byte stream-kind discriminator
// BEFORE the 8-byte user_id prefix:
//
//     [kind:1][user_id:8 ASCII null-padded][payload:rest]
//       ^         ^                              ^
//       |         |                              └── opus (audio) or H.264 NAL (video)
//       |         └── local sender uses 8 NUL bytes (matches voice.rs:393)
//       └── 0x00 = audio (Opus), 0x01 = video (H.264 NAL)
//
// Backward compatibility: legacy frames (no kind byte, just `[uid:8][opus]`)
// are still parseable — `parse_inbound_frame` detects them when bytes[0] is
// >= 0x20 (Vortex user_ids are ULID ASCII, always >= 0x20). Frames whose first
// byte is 0x00 or 0x01 are unambiguously new-format because no valid user_id
// character collides with those values.
//
// These helpers live in voice_common.rs (not voice_wasm.rs) so the tests run
// on native too — voice_wasm.rs is wasm32-gated.

/// Vortex frame stream-kind discriminator (Poly A.5 extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Opus audio frame, payload = opus bytes.
    Audio = 0x00,
    /// H.264 video frame, payload = one or more NAL units (Annex B or FU-A
    /// fragment — see `video_common::fragment_nal_units_to_fua`).
    Video = 0x01,
}

/// Build an outbound binary frame: `[kind:1][8 NUL bytes][payload]`.
///
/// Local sender always uses 8 NUL bytes for the user_id — matches native
/// voice.rs:393. The Vortex server stamps the real user_id on its way out to
/// other peers.
#[must_use]
pub fn build_outbound_frame(kind: FrameKind, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(1 + 8 + payload.len());
    frame.push(kind as u8);
    frame.extend_from_slice(&[0u8; 8]);
    frame.extend_from_slice(payload);
    frame
}

/// Parsed inbound frame.
pub struct ParsedFrame<'a> {
    pub kind: FrameKind,
    pub user_id: String,
    pub payload: &'a [u8],
}

/// Parse an inbound binary frame, tolerating legacy `[uid:8][opus]` format.
///
/// Returns `None` if the frame is too short to be a valid frame in either
/// format.
#[must_use]
pub fn parse_inbound_frame(bytes: &[u8]) -> Option<ParsedFrame<'_>> {
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0];
    // New format: kind byte at [0].
    if (first == 0x00 || first == 0x01) && bytes.len() >= 9 {
        let kind = if first == 0x00 { FrameKind::Audio } else { FrameKind::Video };
        let user_id = std::str::from_utf8(&bytes[1..9])
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();
        return Some(ParsedFrame {
            kind,
            user_id,
            payload: &bytes[9..],
        });
    }
    // Legacy format: [uid:8][opus]. Only valid if first byte is ASCII-ish
    // (Vortex user_ids are ULID-shaped ASCII, always >= 0x20).
    if bytes.len() > 8 && first >= 0x20 {
        let user_id = std::str::from_utf8(&bytes[..8])
            .unwrap_or("")
            .trim_end_matches('\0')
            .to_string();
        return Some(ParsedFrame {
            kind: FrameKind::Audio,
            user_id,
            payload: &bytes[8..],
        });
    }
    None
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
// lint-allow-unused: test module uses unwrap/expect/panic per project policy
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn build_audio_frame_has_kind_zero_and_8_nul_uid() {
        let frame = build_outbound_frame(FrameKind::Audio, &[0xAA, 0xBB, 0xCC]);
        assert_eq!(frame[0], 0x00, "kind byte = audio");
        assert_eq!(&frame[1..9], &[0u8; 8], "8 NUL bytes for local user_id");
        assert_eq!(&frame[9..], &[0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn build_video_frame_has_kind_one() {
        let frame = build_outbound_frame(FrameKind::Video, b"NAL");
        assert_eq!(frame[0], 0x01, "kind byte = video");
        assert_eq!(&frame[1..9], &[0u8; 8]);
        assert_eq!(&frame[9..], b"NAL");
    }

    #[test]
    fn parse_round_trips_audio_frame() {
        let frame = build_outbound_frame(FrameKind::Audio, b"opus");
        let parsed = parse_inbound_frame(&frame).expect("parse failed");
        assert_eq!(parsed.kind, FrameKind::Audio);
        assert_eq!(parsed.user_id, "");
        assert_eq!(parsed.payload, b"opus");
    }

    #[test]
    fn parse_round_trips_video_frame() {
        let frame = build_outbound_frame(FrameKind::Video, b"h264");
        let parsed = parse_inbound_frame(&frame).expect("parse failed");
        assert_eq!(parsed.kind, FrameKind::Video);
        assert_eq!(parsed.user_id, "");
        assert_eq!(parsed.payload, b"h264");
    }

    #[test]
    fn parse_new_format_extracts_user_id() {
        // [kind=0][uid=STOAT01\0][payload]
        let mut frame = vec![0x00];
        frame.extend_from_slice(b"STOAT01\0");
        frame.extend_from_slice(b"opus-bytes");
        let parsed = parse_inbound_frame(&frame).expect("parse failed");
        assert_eq!(parsed.kind, FrameKind::Audio);
        assert_eq!(parsed.user_id, "STOAT01");
        assert_eq!(parsed.payload, b"opus-bytes");
    }

    #[test]
    fn parse_legacy_format_falls_back_to_audio() {
        // Legacy: [uid:8 ASCII][opus]. First byte ASCII so we recognize legacy.
        let mut frame = Vec::new();
        frame.extend_from_slice(b"STOAT012");
        frame.extend_from_slice(b"opus-payload");
        let parsed = parse_inbound_frame(&frame).expect("parse failed");
        assert_eq!(parsed.kind, FrameKind::Audio);
        assert_eq!(parsed.user_id, "STOAT012");
        assert_eq!(parsed.payload, b"opus-payload");
    }

    #[test]
    fn parse_rejects_too_short() {
        assert!(parse_inbound_frame(&[]).is_none());
        assert!(parse_inbound_frame(&[0x00]).is_none());
        assert!(parse_inbound_frame(&[0x00, b'X']).is_none());
    }

    #[test]
    fn parse_rejects_short_legacy_frame() {
        // First byte ASCII but length <= 8 — neither legacy nor new format.
        let frame = b"STOAT01".to_vec();
        assert!(parse_inbound_frame(&frame).is_none());
    }

    #[test]
    fn parse_kind_zero_is_new_format_not_legacy() {
        // [0x00][8 NUL][opus] — must parse as new audio (local-loopback frame),
        // NOT legacy (whose first byte would never be 0x00 for an ASCII uid).
        let frame = build_outbound_frame(FrameKind::Audio, b"local-mic");
        let parsed = parse_inbound_frame(&frame).expect("parse failed");
        assert_eq!(parsed.kind, FrameKind::Audio);
        assert_eq!(parsed.user_id, "", "local NUL uid trims to empty");
        assert_eq!(parsed.payload, b"local-mic");
    }
}
