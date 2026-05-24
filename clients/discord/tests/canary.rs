//! Phase F.3 — Synthetic canary test for Discord anti-ban health.
//!
//! Run this test manually against a **real** throwaway Discord account to
//! verify that our fingerprint, rate-limit guardrails, and Nitro gates
//! produce no 429s or 401s during a normal "human-shaped" session.
//!
//! # Opt-in gate
//!
//! The test requires the `discord-canary` Cargo feature AND the
//! `DISCORD_CANARY_TOKEN` environment variable to be set:
//!
//! ```bash
//! DISCORD_CANARY_TOKEN=<token> \
//! DISCORD_CANARY_GUILD=<guild_id> \
//! DISCORD_CANARY_CHANNEL=<text_channel_id> \
//! cargo test -p poly-discord --test canary --features discord-canary -- --ignored
//! ```
//!
//! # Account credentials
//!
//! **NEVER commit credentials to the repository.**  Store the throwaway
//! account token in 1Password / dev-secrets and export it as
//! `DISCORD_CANARY_TOKEN` before running.
//!
//! # Weekly CI run
//!
//! This test is intended to be triggered weekly by an out-of-band scheduler
//! (not the regular CI pipeline) against a private test guild.  It detects
//! Discord-side rotation that breaks our fingerprint.  See
//! `docs/dev/discord-ban-incident.md` for the runbook to follow when the
//! canary fires.
//!
//! # What the test verifies
//!
//! 1. Login with a user token produces a valid session (no 401).
//! 2. Fetching guilds + channels does not trigger a 429 (rate-guard is
//!    working correctly).
//! 3. Sending 3 messages with realistic inter-message delays produces no
//!    4xx errors.
//! 4. `guardrail_stats()` shows `http_429 == 0` and `http_401 == 0` at
//!    the end of the session.
//! 5. The build number in use is >= `LATEST_KNOWN_STABLE_BUILD` — we never
//!    send a number lower than the floor constant.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Returns true when the canary env vars are set and the `discord-canary` feature
/// is enabled.  All tests in this file call this first and skip when false.
fn canary_enabled() -> bool {
    std::env::var("DISCORD_CANARY_TOKEN").is_ok()
}

/// Helper: read a required env var, panic with a helpful message if missing.
fn require_env(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        panic!(
            "DISCORD_CANARY: {key} is required but not set. \
             Export it before running `cargo test --features discord-canary --test canary -- --ignored`"
        )
    })
}

/// F.3 — Synthetic canary session.
///
/// Logs in, fetches guilds + channels, sends 3 messages with human-paced
/// delays, then asserts no 429s and no 401s occurred.
///
/// Marked `#[ignore]` so it does not run in regular CI — only when explicitly
/// requested with `-- --ignored`.
#[cfg(feature = "discord-canary")]
#[tokio::test]
#[ignore = "canary test: run manually with DISCORD_CANARY_TOKEN set (see test doc)"]
async fn canary_human_shaped_session_no_429_no_401() {
    if !canary_enabled() {
        eprintln!(
            "SKIP — discord canary: DISCORD_CANARY_TOKEN not set. \
             See clients/discord/tests/canary.rs for usage."
        );
        return;
    }

    let token    = require_env("DISCORD_CANARY_TOKEN");
    let guild_id = require_env("DISCORD_CANARY_GUILD");
    let chan_id   = require_env("DISCORD_CANARY_CHANNEL");

    use poly_client::{AuthCredentials, IsBackend};
    use poly_discord::DiscordClient;

    // ── Step 1: Login ──────────────────────────────────────────────────────
    let mut client = DiscordClient::new();
    client
        .authenticate(AuthCredentials::Token(token))
        .await
        .expect("canary: authenticate must succeed (no 401)");

    // ── Step 2: Fetch guilds ───────────────────────────────────────────────
    let servers = client
        .get_servers()
        .await
        .expect("canary: get_servers must succeed (no 429)");
    assert!(
        !servers.is_empty(),
        "canary: expected at least one server; token may be invalid"
    );

    // ── Step 3: Fetch channels ─────────────────────────────────────────────
    let channels = client
        .get_channels(&guild_id)
        .await
        .expect("canary: get_channels must succeed");
    assert!(
        !channels.is_empty(),
        "canary: expected at least one channel in guild {guild_id}"
    );

    // ── Step 4: Send 3 messages with human-paced delays ───────────────────
    // Intervals: 2s, 3s — well within Discord's slow-mode defaults and the
    // 5 req/s sustained rate we cap to in the token bucket.
    let messages = [
        "[canary] Poly anti-ban canary run #1 — ignore",
        "[canary] Poly anti-ban canary run #2 — ignore",
        "[canary] Poly anti-ban canary run #3 — ignore",
    ];
    let delays_ms = [2_000u64, 3_000u64];

    use poly_client::MessageContent;
    client
        .send_message(&chan_id, MessageContent::Text(messages[0].to_string()))
        .await
        .expect("canary: send_message #1 must succeed");

    for (i, (&msg, delay_ms)) in messages[1..].iter().zip(delays_ms.iter()).enumerate() {
        tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
        client
            .send_message(&chan_id, MessageContent::Text(msg.to_string()))
            .await
            .unwrap_or_else(|e| panic!("canary: send_message #{} failed: {e}", i + 2));
    }

    // ── Step 5: Assert no 429s, no 401s, build number >= floor ───────────
    let stats = client.guardrail_stats();
    assert_eq!(
        stats.http_429, 0,
        "canary: got {} HTTP 429 responses — Discord is throttling us; \
         check the rate-guard configuration and request cadence",
        stats.http_429
    );
    assert_eq!(
        stats.http_401, 0,
        "canary: got {} HTTP 401 responses — token is invalid or session expired",
        stats.http_401
    );

    // Verify build number is at or above the floor constant.
    use poly_discord::build_info::LATEST_KNOWN_STABLE_BUILD;
    let health = client.discord_health_snapshot();
    assert!(
        health.build_number_in_use >= LATEST_KNOWN_STABLE_BUILD,
        "canary: build_number_in_use ({}) is below floor constant ({}) — \
         the build-info scraper regressed or the floor was bumped above live build",
        health.build_number_in_use,
        LATEST_KNOWN_STABLE_BUILD,
    );

    eprintln!(
        "canary: PASSED — build_number={}, 2xx={}, 429={}, 401={}, 5xx={}",
        health.build_number_in_use,
        stats.http_2xx,
        stats.http_429,
        stats.http_401,
        stats.http_5xx,
    );
}

/// Stub test that always passes when the `discord-canary` feature is not enabled.
/// Prevents `cargo test --test canary` from failing with "no tests found".
#[cfg(not(feature = "discord-canary"))]
#[test]
fn canary_requires_discord_canary_feature() {
    eprintln!(
        "SKIP — canary tests require `--features discord-canary`. \
         Run with: DISCORD_CANARY_TOKEN=<token> cargo test -p poly-discord \
         --test canary --features discord-canary -- --ignored"
    );
}
