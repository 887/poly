//! Voice bridge smoke test — integration test that requires real Discord credentials.
//!
//! # Running this test
//!
//! This test connects to a real Discord voice channel over the host-bridge.
//! It is opt-in so CI never fails without credentials:
//!
//! ```sh
//! RUN_VOICE_BRIDGE_SMOKE=1 \
//! DISCORD_TOKEN=<bot_or_user_token> \
//! DISCORD_VOICE_ENDPOINT=<voice_ws_endpoint> \
//! DISCORD_VOICE_TOKEN=<voice_token> \
//! DISCORD_VOICE_SESSION_ID=<session_id> \
//! DISCORD_USER_ID=<user_id> \
//! cargo test -p poly-host-bridge --features "voice,video" --test voice
//! ```
//!
//! The voice endpoint, token, and session_id are obtained by:
//! 1. Sending op 4 Voice State Update (guild_id, channel_id) on the main gateway.
//! 2. Receiving VOICE_STATE_UPDATE (session_id) and VOICE_SERVER_UPDATE (endpoint, token).
//!
//! See `tools/discord-voice-smoke/` for a CLI helper that automates step 1-2.
//!
//! # What this test verifies
//!
//! - `POST /host/voice/connect` performs the full WS handshake and returns a session_id.
//! - `POST /host/voice/send_audio` accepts a 1s sine wave PCM buffer (sent_bytes > 0).
//! - `POST /host/voice/disconnect` tears down the session cleanly.
//!
//! The test does NOT verify received audio (that would require another participant
//! in the voice channel). Use the CLI smoke test for a full round-trip test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_host_bridge::voice::{VoiceState, router as voice_router};
use poly_host_bridge::voice_client::{VoiceBridgeClient, VoiceConnectRequest};
use axum::Router;
use tower::ServiceExt;

/// Build a one-shot test app that mounts the voice router.
fn test_app() -> Router {
    let voice_state = VoiceState::new();
    voice_router(voice_state)
}

/// Generate a 1s 440 Hz sine wave at 48 kHz stereo (DISCORD_VOICE format).
/// Returns a flat Vec<i16> with interleaved L/R samples.
fn sine_1s_48k_stereo() -> Vec<i16> {
    const SAMPLE_RATE: usize = 48_000;
    const FREQ: f64 = 440.0;
    let n = SAMPLE_RATE; // 1 second mono
    let mut samples = Vec::with_capacity(n * 2); // stereo: *2
    for i in 0..n {
        let s = (2.0 * std::f64::consts::PI * FREQ * i as f64 / SAMPLE_RATE as f64).sin();
        let i16_sample = (s * 16_000.0) as i16; // moderate amplitude, not clipping
        samples.push(i16_sample);
        samples.push(i16_sample); // duplicate to stereo
    }
    samples
}

/// Skip the test unless `RUN_VOICE_BRIDGE_SMOKE=1` is set.
fn smoke_enabled() -> bool {
    std::env::var("RUN_VOICE_BRIDGE_SMOKE").as_deref() == Ok("1")
}

fn get_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("missing env var {key}"))
}

/// Round-trip smoke: connect → send sine audio → assert sent_bytes > 0 → disconnect.
///
/// Skipped unless RUN_VOICE_BRIDGE_SMOKE=1.
#[tokio::test]
async fn voice_bridge_connect_send_disconnect() {
    if !smoke_enabled() {
        eprintln!("voice_bridge smoke: skipped (set RUN_VOICE_BRIDGE_SMOKE=1 to run)");
        return;
    }

    let ws_endpoint = get_env("DISCORD_VOICE_ENDPOINT");
    let ws_token = get_env("DISCORD_VOICE_TOKEN");
    let ws_session_id = get_env("DISCORD_VOICE_SESSION_ID");
    let user_id = get_env("DISCORD_USER_ID");
    let channel_id = std::env::var("DISCORD_CHANNEL_ID").unwrap_or_else(|_| "0".into());
    let guild_id = std::env::var("DISCORD_GUILD_ID").ok();

    // Connect.
    let connect_req = VoiceConnectRequest {
        backend: "discord".into(),
        account_id: user_id.clone(),
        channel_id,
        ws_endpoint,
        ws_token,
        ws_session_id,
        guild_id,
        user_id,
    };

    // Use the default local bridge.
    let client = VoiceBridgeClient::default_local();
    let resp = client
        .connect(connect_req)
        .await
        .expect("voice/connect should succeed");

    assert!(!resp.session_id.is_empty(), "session_id should be non-empty");
    assert!(resp.voice_ssrc > 0, "voice_ssrc should be non-zero");
    assert_eq!(resp.video_ssrc, resp.voice_ssrc + 1, "video_ssrc = voice_ssrc + 1");

    let session_id = resp.session_id;

    // Send 1s of sine audio. The encode loop accumulates 20ms frames and sends them.
    let pcm = sine_1s_48k_stereo();
    let audio_resp = client
        .send_audio(&session_id, &pcm)
        .await
        .expect("voice/send_audio should succeed");

    assert!(
        audio_resp.sent_bytes > 0,
        "sent_bytes should be > 0 for audible PCM; got {}",
        audio_resp.sent_bytes
    );

    // Disconnect cleanly.
    client
        .disconnect(&session_id)
        .await
        .expect("voice/disconnect should succeed");
}

/// Unit-level: verify wire types serialize/deserialize correctly without a real connection.
#[test]
fn voice_wire_types_round_trip() {
    use poly_host_bridge::voice_wire::{VoiceEvent, VoiceConnectResponse};

    let resp = VoiceConnectResponse {
        ok: true,
        session_id: "test-session-123".into(),
        voice_ssrc: 12345,
        video_ssrc: 12346,
        err: None,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let parsed: VoiceConnectResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_id, "test-session-123");
    assert_eq!(parsed.voice_ssrc, 12345);
    assert_eq!(parsed.video_ssrc, 12346);

    // VoiceEvent SSE envelope.
    let ev = VoiceEvent::Speaking {
        user_id: "u42".into(),
        is_speaking: true,
    };
    let json = serde_json::to_string(&ev).unwrap();
    assert!(json.contains("\"type\":\"speaking\""), "got: {json}");
    assert!(json.contains("\"user_id\":\"u42\""), "got: {json}");

    let ev2 = VoiceEvent::FrameAudio {
        user_id: "u43".into(),
        pcm_b64: "AAAA".into(),
        samples: 960,
    };
    let json2 = serde_json::to_string(&ev2).unwrap();
    let parsed2: VoiceEvent = serde_json::from_str(&json2).unwrap();
    assert!(matches!(parsed2, VoiceEvent::FrameAudio { samples: 960, .. }));
}

/// Sine wave PCM generation helper test.
#[test]
fn sine_pcm_generation() {
    let pcm = sine_1s_48k_stereo();
    // 1s @ 48kHz stereo = 96000 samples.
    assert_eq!(pcm.len(), 96_000);
    // Should contain non-zero samples.
    let non_zero = pcm.iter().filter(|&&s| s != 0).count();
    assert!(non_zero > pcm.len() / 2, "most samples should be non-zero sine values");
}
