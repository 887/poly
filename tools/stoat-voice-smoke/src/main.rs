//! Stoat voice transport CLI smoke test skeleton — Phase K.3 of
//! `docs/plans/plan-voice-video-calls.md`.
//!
//! # Current status
//!
//! Phase F (Stoat voice gateway) has not yet shipped.  This binary is a
//! placeholder that exits 0 with a clear status message.  Once Phase F
//! lands, this skeleton should be replaced with a real smoke test that:
//!
//! 1. Connects to the local `test-stoat` fixture (`servers/test-stoat/`, port 9101).
//! 2. Authenticates with a seeded test account (no real credentials required —
//!    the fixture accepts any well-formed token).
//! 3. Joins a voice channel present in the fixture's seed data.
//! 4. Plays 2 s of silence via `FakeAudioBackend`.
//! 5. Verifies that `open_input_calls >= 1` (Stoat voice encode loop started).
//! 6. Disconnects cleanly.
//! 7. Exits 0 on success, non-zero on any error.
//!
//! # Usage (Phase F+)
//!
//! ```bash
//! RUN_STOAT_VOICE_SMOKE=1 cargo run -p stoat-voice-smoke
//! ```
//!
//! Unlike the Discord smoke test, this smoke test does NOT require external
//! credentials — the `test-stoat` fixture is self-contained.  It will be
//! always-on in CI once Phase F ships.
//!
//! See `TEST_HARNESS.md` step 8 for the harness integration.

// TODO(Phase-F): Remove this once Stoat voice transport is implemented.
// The real smoke test will depend on:
//   - poly-stoat with voice feature (Phase F.3 or F.4)
//   - poly-audio-backend (FakeAudioBackend for the test)
//   - The test-stoat fixture running on port 9101
//   - StoatClient::connect_voice / disconnect_voice (Phase F.8)

fn main() {
    // Phase F is not yet shipped.  Print a clear status and exit 0 so that
    // TEST_HARNESS.md step 8 passes on branches where Phase F hasn't landed.
    println!("stoat voice not yet implemented (Phase F)");
    println!("SKIP — Phase F (Stoat voice gateway) has not shipped yet.");
    println!("       When Phase F lands, replace this skeleton with a real smoke test.");
    println!("       See docs/plans/plan-voice-video-calls.md Phase K.3 for the spec.");
    std::process::exit(0);
}
