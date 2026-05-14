//! # Voice bridge wire types
//!
//! Request/response structs and SSE event enum for the `/host/voice/*` routes.
//!
//! These types are available on **all targets** including `wasm32-unknown-unknown`.
//! The server-side handlers (`voice.rs`) and the typed client (`voice_client.rs`)
//! both import from here so the protocol can never drift.

use serde::{Deserialize, Serialize};

// ── Route constants ────────────────────────────────────────────────────────────

pub const ROUTE_VOICE_CONNECT: &str = "/host/voice/connect";
pub const ROUTE_VOICE_DISCONNECT: &str = "/host/voice/disconnect";
pub const ROUTE_VOICE_SEND_AUDIO: &str = "/host/voice/send_audio";
pub const ROUTE_VOICE_SEND_VIDEO: &str = "/host/voice/send_video";
pub const ROUTE_VOICE_SET_MUTE: &str = "/host/voice/set_mute";
/// Path pattern for the SSE stream. Caller replaces `:session_id` manually.
pub const ROUTE_VOICE_EVENTS_PATTERN: &str = "/host/voice/events/:session_id";

// ── Connect ───────────────────────────────────────────────────────────────────

/// Request body for `POST /host/voice/connect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConnectRequest {
    /// Backend identifier (e.g. `"discord"`).
    pub backend: String,
    /// Account ID on that backend.
    pub account_id: String,
    /// Voice channel ID to join.
    pub channel_id: String,
    /// Voice WebSocket endpoint (from `VOICE_SERVER_UPDATE.endpoint`).
    pub ws_endpoint: String,
    /// Voice WebSocket token (from `VOICE_SERVER_UPDATE.token`).
    pub ws_token: String,
    /// Voice session ID (from `VOICE_STATE_UPDATE.session_id`).
    pub ws_session_id: String,
    /// Guild ID (server). `None` for DM calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    /// Local user's Discord user ID.
    pub user_id: String,
}

/// Response body for `POST /host/voice/connect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConnectResponse {
    pub ok: bool,
    /// Opaque session identifier — pass to all subsequent endpoints.
    #[serde(default)]
    pub session_id: String,
    /// Audio SSRC assigned by Discord.
    #[serde(default)]
    pub voice_ssrc: u32,
    /// Video SSRC (audio_ssrc + 1 per Discord convention).
    #[serde(default)]
    pub video_ssrc: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── Disconnect ────────────────────────────────────────────────────────────────

/// Request body for `POST /host/voice/disconnect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceDisconnectRequest {
    pub session_id: String,
}

/// Response body for `POST /host/voice/disconnect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceDisconnectResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── Send audio ────────────────────────────────────────────────────────────────

/// Request body for `POST /host/voice/send_audio`.
///
/// `pcm_b64` is a standard-base64 encoding of a little-endian `i16` array
/// at 48 kHz stereo (matching `AudioFormat::DISCORD_VOICE`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendAudioRequest {
    pub session_id: String,
    /// Little-endian i16 PCM samples, base64-encoded.
    pub pcm_b64: String,
}

/// Response body for `POST /host/voice/send_audio`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendAudioResponse {
    pub ok: bool,
    /// UDP bytes sent (0 when muted or below VAD threshold).
    #[serde(default)]
    pub sent_bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── Send video ────────────────────────────────────────────────────────────────

/// One video frame — mirrors `VideoFrame` from `poly-video-backend`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFrameWire {
    pub width: u32,
    pub height: u32,
    /// Pixel format: `"bgra"`, `"yuv420p"`, or `"nv12"`.
    pub format: String,
    /// Frame data, base64-encoded (standard alphabet).
    pub data_b64: String,
}

/// Request body for `POST /host/voice/send_video`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendVideoRequest {
    pub session_id: String,
    pub frame: VideoFrameWire,
}

/// Response body for `POST /host/voice/send_video`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendVideoResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── Set mute ──────────────────────────────────────────────────────────────────

/// Request body for `POST /host/voice/set_mute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMuteRequest {
    pub session_id: String,
    pub muted: bool,
    pub deafened: bool,
}

/// Response body for `POST /host/voice/set_mute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMuteResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── SSE event ─────────────────────────────────────────────────────────────────

/// One SSE event pushed to the browser over `GET /host/voice/events/:session_id`.
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
/// to push a decoded frame. The browser renders the decoded `VideoFrame` to a
/// `<canvas>` element.
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
    /// **without** Annex-B start codes.  Browser feeds these to
    /// `WebCodecs VideoDecoder.decode()`.
    FrameH264 {
        user_id: String,
        nal_units_b64: Vec<String>,
        is_keyframe: bool,
    },
    /// The voice session has been terminated (either by the client calling
    /// `disconnect`, or by server-side orphan GC).
    Disconnected { reason: String },
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn voice_event_discriminant_serializes() {
        let ev = VoiceEvent::Speaking {
            user_id: "u1".into(),
            is_speaking: true,
        };
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
