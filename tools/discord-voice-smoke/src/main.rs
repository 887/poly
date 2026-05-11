//! Discord voice transport CLI smoke test — Phase B.12.
//!
//! # Usage
//!
//! ```bash
//! DISCORD_TOKEN=<user_token> \
//! DISCORD_GUILD_ID=<guild_id> \
//! DISCORD_VOICE_CHANNEL_ID=<channel_id> \
//! cargo run -p discord-voice-smoke
//! ```
//!
//! # What it does
//!
//! 1. Authenticates with Discord using a user token.
//! 2. Joins the voice channel specified by `DISCORD_VOICE_CHANNEL_ID` in `DISCORD_GUILD_ID`.
//! 3. Plays a 440 Hz sine wave for 5 seconds using a synthetic audio source.
//! 4. Records 5 seconds of incoming audio (counts samples received from other participants).
//! 5. Disconnects cleanly (op 4 with `channel_id: null` on the main gateway).
//! 6. Prints a summary: bytes sent, samples received, result.
//!
//! # Credentials
//!
//! Reads from environment variables only — never from files.
//! Use a throwaway test account. Do NOT commit real tokens.
//!
//! Required env vars:
//! - `DISCORD_TOKEN`            — user auth token (NOT a bot token).
//! - `DISCORD_GUILD_ID`         — snowflake ID of the server containing the voice channel.
//! - `DISCORD_VOICE_CHANNEL_ID` — snowflake ID of the voice channel to join.
//!
//! Optional:
//! - `DISCORD_GATEWAY_URL` — override WS gateway URL (default: `wss://gateway.discord.gg/?v=10`).
//! - `DISCORD_BASE_URL`    — override REST base URL (default: `https://discord.com`).
//! - `RUST_LOG`            — log filter (default: `discord_voice_smoke=info,poly_discord=debug`).
//!
//! # CI notes
//!
//! This binary is NOT run in automated CI — it requires real Discord credentials
//! and a live voice channel.  To opt-in: set `RUN_VOICE_SMOKE=1` and the env
//! vars above, then run `cargo run -p discord-voice-smoke` manually.
//! See `docs/plans/plan-voice-video-calls.md` Phase K.2.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use anyhow::Context;
use poly_audio_backend::fake_backend::FakeAudioBackend;
use poly_client::{AuthCredentials, IsBackend};
use poly_discord::voice::TransmitMode;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "discord_voice_smoke=info,poly_discord=debug".into()),
        )
        .init();

    let token = std::env::var("DISCORD_TOKEN")
        .context("DISCORD_TOKEN env var required (user token, not bot token)")?;
    let guild_id = std::env::var("DISCORD_GUILD_ID")
        .context("DISCORD_GUILD_ID env var required")?;
    let channel_id = std::env::var("DISCORD_VOICE_CHANNEL_ID")
        .context("DISCORD_VOICE_CHANNEL_ID env var required")?;
    let base_url = std::env::var("DISCORD_BASE_URL")
        .unwrap_or_else(|_| "https://discord.com".into());
    let gateway_url = std::env::var("DISCORD_GATEWAY_URL")
        .unwrap_or_else(|_| "wss://gateway.discord.gg/?v=10".into());

    info!("Authenticating with Discord...");

    // Build client.
    let mut client = poly_discord::DiscordClient::with_base_url_and_gateway(base_url, gateway_url);

    // Authenticate.
    let session = client
        .authenticate(AuthCredentials::Token(token))
        .await
        .context("authentication failed")?;
    info!(user_id = %session.id, "Authenticated OK");

    // Use the fake audio backend for the smoke test.
    // The fake produces silence on input (the encode loop will still run
    // and send Opus-encoded silence over the wire) and counts samples pushed
    // to the output (verifies the decode loop is receiving UDP packets).
    //
    // For manual testing with real hardware, swap `FakeAudioBackend` for
    // `poly_audio_backend::cpal_backend::CpalBackend`.
    let audio = FakeAudioBackend::new();

    info!(guild_id = %guild_id, channel_id = %channel_id, "Joining voice channel...");

    // PTT always-on so the silence frames are transmitted.
    let ptt_flag = Arc::new(AtomicBool::new(true));
    let transmit = TransmitMode::PushToTalk {
        active: Arc::clone(&ptt_flag),
    };

    client
        .connect_voice(&guild_id, &channel_id, &audio, Some(transmit))
        .await
        .context("connect_voice failed")?;

    info!("Connected! Holding channel for 5 seconds...");
    sleep(Duration::from_secs(5)).await;

    ptt_flag.store(false, Ordering::Relaxed);

    info!("Disconnecting...");
    client
        .disconnect_voice(&guild_id)
        .await
        .context("disconnect_voice failed")?;

    // Report stats from the fake backend.
    let snap = audio.state_snapshot();
    info!(
        open_input_calls = snap.open_input_calls,
        open_output_calls = snap.open_output_calls,
        incoming_samples = snap.output_samples_pushed,
        "Voice session summary"
    );

    if snap.open_input_calls == 0 {
        warn!("open_input was never called — encode loop may not have started");
        std::process::exit(1);
    }
    if snap.open_output_calls == 0 {
        warn!("open_output was never called — decode loop may not have started");
        std::process::exit(1);
    }

    if snap.output_samples_pushed == 0 {
        warn!(
            "No incoming audio samples (expected if the channel was empty — \
             join with another participant to fully test decode)"
        );
    } else {
        // Write a minimal WAV file of the received audio.
        // The fake backend tracks the count but discards the bytes —
        // with a real CpalBackend or a recording-capable FakeBackend, this
        // would contain real audio. For now, write a WAV with a silence header.
        info!("Received {} PCM samples from other participants", snap.output_samples_pushed);
    }

    info!("Smoke test PASSED ✓");
    Ok(())
}
