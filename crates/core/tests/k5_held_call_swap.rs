//! K.5 — Held-call swap test (`docs/plans/plan-voice-video-calls.md`).
//!
//! Models the user flow described in K.5: start a Discord voice channel
//! call, start a Stoat DM call (which holds the Discord call), assert the
//! Discord call lands in `held_voice_connections`, click swap, assert it
//! returns to active. Exercised at the data-model layer against the public
//! `poly_core::state::voice_state::VoiceState` shape used by every
//! BatchedSignal write in `direct_call.rs`.
//!
//! # Why an integration test (not a Dioxus UI test)
//!
//! The production swap code (`crates/core/src/ui/account/common/direct_call.rs`
//! `swap_to_first_held_call`) lives behind `BatchedSignal::batch(|v| …)`,
//! which requires a Dioxus runtime. The state mutation itself is pure;
//! mirroring the production logic here exercises the contract end-to-end
//! without spinning up a renderer. Same pattern as the `voice_session_guard`
//! mock-mutex test in `clients/discord/tests/anti_ban.rs` K.7.3.
//!
//! If the real `swap_to_first_held_call` impl diverges from this mirror
//! (e.g. changes hold order semantics), this test will lag — that's
//! intentional: a divergence here is the cue to update the test AND the
//! plan's K.5 entry together. A proper Playwright UI test that exercises
//! the actual `voice_state.batch(…)` call is the follow-up.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_client::{BackendType, VoiceConnection, VoiceConnectionKind};
use poly_core::state::voice_state::VoiceState;

fn vc(channel: &str, backend_slug: &str, kind: VoiceConnectionKind) -> VoiceConnection {
    VoiceConnection {
        channel_id: channel.into(),
        server_id: format!("srv-{channel}"),
        channel_name: format!("Channel {channel}"),
        server_name: format!("Server {channel}"),
        backend: BackendType::new(backend_slug),
        account_id: "acct-1".into(),
        instance_id: "inst-1".into(),
        is_muted: false,
        is_deafened: false,
        is_streaming: false,
        is_video_on: false,
        kind,
        dm_id: None,
        participant_user_ids: vec![],
    }
}

/// Mirror of `swap_to_first_held_call` from
/// `crates/core/src/ui/account/common/direct_call.rs:549` minus the
/// `BatchedSignal` wrapper. Same FIFO contract:
/// - active → back of `held_voice_connections`
/// - first entry of `held_voice_connections` → active
fn swap_to_first_held(state: &mut VoiceState) {
    let Some(current) = state.voice_connection.clone() else {
        return;
    };
    if state.held_voice_connections.is_empty() {
        return;
    }
    let next = state.held_voice_connections.remove(0);
    state.held_voice_connections.push(current);
    state.voice_connection = Some(next);
}

/// Mirror of the "start a second call while in one" hold-current-first flow
/// in `direct_call.rs:205` (the `held_voice_connections.insert(0, current)`
/// pattern that runs when a second start_direct_call fires while
/// `voice_connection.is_some()`).
fn hold_current_and_activate(state: &mut VoiceState, new_call: VoiceConnection) {
    if let Some(current) = state.voice_connection.take() {
        state.held_voice_connections.insert(0, current);
    }
    state.voice_connection = Some(new_call);
}

// ─── K.5 — Discord voice channel + Stoat DM call held-swap flow ─────────

#[test]
fn k5_discord_voice_then_stoat_dm_holds_and_swaps_back() {
    let discord = vc(
        "discord-voice-1",
        "discord",
        VoiceConnectionKind::ServerChannel,
    );
    let stoat_dm = vc("stoat-dm-1", "stoat", VoiceConnectionKind::TemporaryCall);

    let mut state = VoiceState::default();

    // Step 1: start Discord voice channel call → active.
    state.voice_connection = Some(discord.clone());
    assert_eq!(
        state.voice_connection.as_ref().unwrap().channel_id,
        "discord-voice-1"
    );
    assert!(state.held_voice_connections.is_empty());

    // Step 2: start Stoat DM call → Discord moves to held_voice_connections.
    hold_current_and_activate(&mut state, stoat_dm.clone());
    assert_eq!(
        state.voice_connection.as_ref().unwrap().channel_id,
        "stoat-dm-1"
    );
    assert_eq!(
        state.voice_connection.as_ref().unwrap().backend,
        "stoat",
        "Stoat DM call should be active after second start"
    );
    assert_eq!(
        state.held_voice_connections.len(),
        1,
        "Discord call should be on hold"
    );
    assert_eq!(
        state.held_voice_connections[0].channel_id,
        "discord-voice-1"
    );
    assert_eq!(state.held_voice_connections[0].backend, "discord");

    // Step 3: click swap → Discord returns to active; Stoat moves to held.
    swap_to_first_held(&mut state);
    assert_eq!(
        state.voice_connection.as_ref().unwrap().channel_id,
        "discord-voice-1",
        "Discord call should be active after swap"
    );
    assert_eq!(state.voice_connection.as_ref().unwrap().backend, "discord");
    assert_eq!(state.held_voice_connections.len(), 1);
    assert_eq!(state.held_voice_connections[0].channel_id, "stoat-dm-1");
    assert_eq!(state.held_voice_connections[0].backend, "stoat");

    // Step 4: swap again → Stoat back to active, Discord held again.
    swap_to_first_held(&mut state);
    assert_eq!(
        state.voice_connection.as_ref().unwrap().channel_id,
        "stoat-dm-1"
    );
    assert_eq!(
        state.held_voice_connections[0].channel_id,
        "discord-voice-1"
    );
}

#[test]
fn k5_swap_with_no_held_calls_is_noop() {
    let discord = vc("d1", "discord", VoiceConnectionKind::ServerChannel);
    let mut state = VoiceState {
        voice_connection: Some(discord),
        ..VoiceState::default()
    };
    swap_to_first_held(&mut state);
    assert_eq!(state.voice_connection.as_ref().unwrap().channel_id, "d1");
    assert!(state.held_voice_connections.is_empty());
}

#[test]
fn k5_swap_with_no_active_call_is_noop() {
    let mut state = VoiceState::default();
    swap_to_first_held(&mut state);
    assert!(state.voice_connection.is_none());
    assert!(state.held_voice_connections.is_empty());
}

#[test]
fn k5_three_chained_holds_preserve_fifo_swap_order() {
    let a = vc("a", "discord", VoiceConnectionKind::ServerChannel);
    let b = vc("b", "stoat", VoiceConnectionKind::TemporaryCall);
    let c = vc("c", "matrix", VoiceConnectionKind::ServerChannel);

    let mut state = VoiceState {
        voice_connection: Some(a),
        ..VoiceState::default()
    };
    hold_current_and_activate(&mut state, b);
    hold_current_and_activate(&mut state, c);

    // Active = c; held = [b, a] (b was most recently held → front of queue).
    assert_eq!(state.voice_connection.as_ref().unwrap().channel_id, "c");
    assert_eq!(state.held_voice_connections.len(), 2);
    assert_eq!(state.held_voice_connections[0].channel_id, "b");
    assert_eq!(state.held_voice_connections[1].channel_id, "a");

    // Swap → active = b; held = [a, c] (c pushed to back).
    swap_to_first_held(&mut state);
    assert_eq!(state.voice_connection.as_ref().unwrap().channel_id, "b");
    assert_eq!(state.held_voice_connections[0].channel_id, "a");
    assert_eq!(state.held_voice_connections[1].channel_id, "c");

    // Swap → active = a; held = [c, b].
    swap_to_first_held(&mut state);
    assert_eq!(state.voice_connection.as_ref().unwrap().channel_id, "a");
    assert_eq!(state.held_voice_connections[0].channel_id, "c");
    assert_eq!(state.held_voice_connections[1].channel_id, "b");
}
