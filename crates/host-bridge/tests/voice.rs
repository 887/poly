//! # Voice primitive wire-type smoke tests
//!
//! The old `/host/voice/*` Discord-coupled integration test is replaced by:
//!
//! - `tests/udp.rs`         — UDP socket service
//! - `tests/codec_opus.rs`  — Opus encode/decode service
//! - `tests/aead.rs`        — AEAD encrypt/decrypt service
//!
//! This file contains only the portable wire-type round-trip tests that do not
//! require a running server (they use `serde_json` serialization only).
//!
//! The feature gate on `[[test]] required-features = ["voice"]` is kept so the
//! test is compiled only when the primitives are available.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Verify VoiceEvent SSE envelope serialization — the discord plugin emits
/// these events to the browser over its own SSE stream.
#[test]
fn voice_event_discriminant_serializes() {
    use poly_host_bridge::voice_wire::VoiceEvent;

    let ev = VoiceEvent::Speaking { user_id: "u42".into(), is_speaking: true };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("\"type\":\"speaking\""), "got: {json}");
    assert!(json.contains("\"user_id\":\"u42\""), "got: {json}");
    let parsed: VoiceEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(parsed, VoiceEvent::Speaking { .. }));
}

#[test]
fn voice_event_frame_audio_round_trip() {
    use poly_host_bridge::voice_wire::VoiceEvent;

    let ev = VoiceEvent::FrameAudio {
        user_id: "u43".into(),
        pcm_b64: "AAAA".into(),
        samples: 960,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("\"type\":\"frame_audio\""), "got: {json}");
    let parsed: VoiceEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(parsed, VoiceEvent::FrameAudio { samples: 960, .. }));
}

#[test]
fn udp_bind_response_wire() {
    use poly_host_bridge::udp::UdpBindResponse;
    let r = UdpBindResponse {
        ok: true,
        session_id: "s1".into(),
        local_port: 9999,
        err: None,
    };
    let json = serde_json::to_string(&r).unwrap();
    let parsed: UdpBindResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.local_port, 9999);
    assert!(parsed.ok);
}

#[test]
fn opus_encode_response_wire() {
    use poly_host_bridge::codec_opus::OpusEncodeResponse;
    let r = OpusEncodeResponse { ok: true, encoded: "AAAA".into(), err: None };
    let json = serde_json::to_string(&r).unwrap();
    let parsed: OpusEncodeResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.encoded, "AAAA");
}

#[test]
fn aead_create_response_wire() {
    use poly_host_bridge::aead::AeadCreateResponse;
    let r = AeadCreateResponse { ok: true, session_id: "sess-xyz".into(), err: None };
    let json = serde_json::to_string(&r).unwrap();
    let parsed: AeadCreateResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, "sess-xyz");
}
