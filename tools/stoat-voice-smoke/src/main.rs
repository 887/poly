//! Stoat voice transport CLI smoke test — Phase F / K.3 of `docs/plans/plan-voice-video-calls.md`.
//!
//! # What it tests
//!
//! 1. Starts the test-stoat mock server on a random available port.
//! 2. Authenticates as the "stoat" fixture user.
//! 3. Connects voice via `StoatClient::connect_voice` (which internally calls
//!    `POST /channels/CHVOICE001/join_call` and connects the Vortex WS).
//! 4. Waits 2 seconds — enough for the mock to inject the fake raccoon participant.
//! 5. Asserts: `FakeAudioBackend.open_input_calls == 1` (encode loop started).
//! 6. Asserts: `FakeAudioBackend.open_output_calls == 1` (decode loop started).
//! 7. Asserts: at least one `VoiceUserJoined` event was received.
//! 8. Disconnects cleanly.
//!
//! # Usage
//!
//! ```bash
//! RUN_STOAT_VOICE_SMOKE=1 cargo run -p poly-stoat-voice-smoke
//! ```
//!
//! Without `RUN_STOAT_VOICE_SMOKE=1` the binary exits 0 immediately (compile-only check).
//!
//! # CI
//!
//! Always compiles (no external credentials). Actual execution is gated by the env var.
//! `TEST_HARNESS.md` step 8 references this smoke (K.3).

// lint-allow-unused: smoke-test binary; unwrap/expect are fine in test binaries; panic not used
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use anyhow::Context;
use poly_audio_backend::fake_backend::FakeAudioBackend;
use poly_client::{AuthCredentials, ClientEvent, IsBackend};
use poly_stoat::voice_common::TransmitMode;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

/// Seed voice channel ID (matches test-stoat state.rs seed data).
const VOICE_CHANNEL_ID: &str = "CHVOICE001";

/// Test user credentials (from test-stoat seed data).
const TEST_USER: &str = "stoat";
const TEST_PASS: &str = "testpass123";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "stoat_voice_smoke=info,poly_stoat=debug".into()),
        )
        .init();

    if std::env::var("RUN_STOAT_VOICE_SMOKE").unwrap_or_default() != "1" {
        info!("RUN_STOAT_VOICE_SMOKE != 1 — skipping smoke test (compile-only check passed)");
        return Ok(());
    }

    run_smoke().await
}

async fn run_smoke() -> anyhow::Result<()> {
    info!("Starting test-stoat mock server...");

    // Spin up the test-stoat mock on a random port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind test-stoat listener")?;
    let port = listener.local_addr()?.port();
    let base_url = format!("http://127.0.0.1:{port}");

    let state = Arc::new(poly_test_stoat::StoatState::new());
    state.seed();
    let router = poly_test_stoat::router(Arc::clone(&state));
    tokio::spawn(async move {
        axum::serve(listener, router).await.expect("test-stoat serve");
    });

    // Give the server a moment to start.
    sleep(Duration::from_millis(50)).await;

    info!(base_url = %base_url, "test-stoat ready");

    // Authenticate.
    let mut client = poly_stoat::StoatClient::with_base_url(&base_url)
        .context("StoatClient::with_base_url")?;

    let session = client
        .authenticate(AuthCredentials::EmailPassword {
            email: TEST_USER.into(),
            password: TEST_PASS.into(),
        })
        .await
        .context("authenticate")?;
    info!(user_id = %session.id, "Authenticated as stoat");

    // Build audio backend (fake — no real mic/speaker needed for smoke).
    let audio = FakeAudioBackend::new();
    let (ev_tx, mut ev_rx) = mpsc::channel::<ClientEvent>(64);

    // PTT always-on so silence frames are transmitted.
    let ptt_flag = Arc::new(AtomicBool::new(true));
    let transmit = TransmitMode::PushToTalk {
        active: Arc::clone(&ptt_flag),
    };

    // Connect voice (join_call REST + Vortex WS + Opus loops).
    info!(channel_id = VOICE_CHANNEL_ID, "Connecting voice...");
    client
        .connect_voice(VOICE_CHANNEL_ID, &audio, Some(transmit), ev_tx)
        .await
        .context("connect_voice")?;

    info!("Voice connected! Holding for 2 seconds...");
    sleep(Duration::from_secs(2)).await;

    ptt_flag.store(false, Ordering::Relaxed);

    // Collect events that arrived.
    let mut participant_join_events = 0usize;
    while let Ok(ev) = ev_rx.try_recv() {
        match ev {
            ClientEvent::VoiceUserJoined { channel_id, participant } => {
                info!(channel_id = %channel_id, user = %participant.user.id, "VoiceUserJoined received");
                participant_join_events += 1;
            }
            ClientEvent::VoiceSpeakingUpdate { user_id, is_speaking, .. } => {
                info!(user_id = %user_id, is_speaking, "VoiceSpeakingUpdate received");
            }
            other => {
                info!("Other event: {other:?}");
            }
        }
    }

    info!("Disconnecting...");
    client.disconnect_voice().await;

    // Verify audio backend usage.
    let snap = audio.state_snapshot();
    info!(
        open_input_calls = snap.open_input_calls,
        open_output_calls = snap.open_output_calls,
        incoming_samples = snap.output_samples_pushed,
        participant_join_events,
        "Voice session summary"
    );

    // Assertions.
    let mut failed = false;

    if snap.open_input_calls == 0 {
        warn!("FAIL: open_input was never called — encode loop may not have started");
        failed = true;
    }
    if snap.open_output_calls == 0 {
        warn!("FAIL: open_output was never called — decode loop may not have started");
        failed = true;
    }
    if participant_join_events == 0 {
        warn!(
            "FAIL: no VoiceUserJoined events received — the mock raccoon participant \
             should have joined within 100ms"
        );
        failed = true;
    }

    if failed {
        std::process::exit(1);
    }

    info!("Smoke test PASSED ✓");
    Ok(())
}
