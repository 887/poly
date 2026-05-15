//! # Voice SSE event types
//!
//! `VoiceEvent` is the SSE payload emitted by the discord plugin's own voice
//! event stream. Previously this also held wire types for the old
//! `/host/voice/*` routes; those routes have been removed and their protocol
//! logic moved to `clients/discord/src/voice_bridge.rs`.
//!
//! The types here are kept on all targets (including wasm32) so the discord
//! plugin WASM side can parse incoming VoiceEvent SSE frames.

use serde::{Deserialize, Serialize};

// ── SSE event ─────────────────────────────────────────────────────────────────

/// One SSE event yielded by the discord voice event stream.
///
/// Serialized as a JSON object with a `"type"` discriminant.
/// Browser consumers read these via `EventSource` and route based on `type`.
///
/// ## Audio path
///
/// `FrameAudio` carries decoded PCM from a remote participant. The browser
/// WASM hands the `pcm_b64` bytes to `WebAudioBackend::open_output().push()`.
///
/// ## Video path (receive)
///
/// `FrameH264` carries encoded H.264 NAL units. The browser decodes them via
/// `WebCodecs VideoDecoder`, which is more efficient than re-encoding server-side
/// to push a decoded frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceEvent {
    /// A remote participant started or stopped speaking.
    Speaking {
        user_id: String,
        is_speaking: bool,
    },
    /// A new participant has been observed in the voice channel.
    ParticipantJoin { user_id: String, ssrc: u32 },
    /// A participant left (or was last seen) in the voice channel.
    ParticipantLeave { user_id: String },
    /// Decoded PCM audio from a remote participant.
    ///
    /// `pcm_b64` is little-endian i16 samples, 48 kHz stereo, base64-encoded.
    FrameAudio {
        user_id: String,
        /// Little-endian i16 stereo PCM samples, base64-encoded.
        pcm_b64: String,
        /// Number of stereo sample pairs decoded.
        samples: u32,
    },
    /// Encoded H.264 NAL units from a remote participant.
    ///
    /// Each entry in `nal_units_b64` is one NAL unit, base64-encoded,
    /// **without** Annex-B start codes. Browser feeds these to
    /// `WebCodecs VideoDecoder.decode()`.
    FrameH264 {
        user_id: String,
        nal_units_b64: Vec<String>,
        is_keyframe: bool,
    },
    /// The voice session has been terminated.
    Disconnected { reason: String },
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn voice_event_discriminant_serializes() {
        let ev = VoiceEvent::Speaking { user_id: "u1".into(), is_speaking: true };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"speaking\""), "got: {json}");
        let parsed: VoiceEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, VoiceEvent::Speaking { .. }));
    }

    #[test]
    fn voice_event_frame_audio_round_trip() {
        let ev = VoiceEvent::FrameAudio {
            user_id: "u2".into(),
            pcm_b64: "AAAA".into(),
            samples: 960,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains("\"type\":\"frame_audio\""), "got: {json}");
        let parsed: VoiceEvent = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed, VoiceEvent::FrameAudio { samples: 960, .. }));
    }
}
