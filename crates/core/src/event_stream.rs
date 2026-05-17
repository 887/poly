//! Shared event-stream listener for all backend accounts.
//!
//! [`spawn_event_stream_listener`] starts a background Dioxus task that
//! polls a backend's [`poly_client::IsBackend::event_stream`] and routes
//! each incoming [`poly_client::ClientEvent`] into the appropriate signals.
//!
//! Previously this function lived in `crate::ui::demo` (demo-only). It is
//! now shared so that native accounts restored at startup (discord, matrix,
//! teams, stoat, …) also have their event streams polled — which is required
//! for Discord voice (gateway-bridge connect) to function.

use dioxus::prelude::*;

use crate::client_manager::{BackendHandle, BackendHandleExt, ClientManager};
use crate::state::{AccountSessions, AppState, NavState, VoiceState};
use crate::state::BatchedSignal;
use crate::ui::account::common::chat_history::{
    read_message_list_scroll_metrics, request_scroll_to_bottom,
};
use crate::ui::routes::Route;

/// Scroll threshold: if the user is within this many pixels of the bottom,
/// treat them as "at the bottom" and auto-scroll when a new message arrives.
const AUTO_SCROLL_THRESHOLD_PX: f64 = 60.0;

/// Start a background event-stream listener for a single backend account.
///
/// Spawns a Dioxus task that polls the backend's
/// [`poly_client::IsBackend::event_stream`] and processes each incoming
/// [`poly_client::ClientEvent`]:
///
/// - [`poly_client::ClientEvent::MessageReceived`] — appends the message to
///   `chat_data.messages` when the current channel is selected.
/// - [`poly_client::ClientEvent::PresenceChanged`] — updates presence on matching members.
/// - Other events are silently ignored for now.
///
/// The task exits automatically when the account is removed from
/// `client_manager` (checked after each event) so there is no orphan task.
pub(crate) fn spawn_event_stream_listener(
    account_id: String,
    backend: BackendHandle,
    app_state: BatchedSignal<AppState>,
    nav: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<crate::state::ChatViewState>,
    account_sessions: BatchedSignal<AccountSessions>,
    voice_state: BatchedSignal<VoiceState>,
) {
    use futures::StreamExt as _;
    use poly_client::ClientEvent;

    spawn(async move {
        // Acquire the stream without holding the lock for the duration of polling.
        let stream = {
            let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                Ok(g) => g,
                Err(_) => {
                    tracing::warn!("event_stream: backend read timed out acquiring event_stream for {account_id}");
                    return;
                }
            };
            guard.event_stream()
        };
        let mut stream = stream;

        tracing::debug!("Event stream started for account: {account_id}");

        while let Some(event) = stream.next().await {
            // Stop the listener when the account is removed.
            let still_active = {
                let cm = client_manager.read();
                cm.get_backend(&account_id).is_some()
            };
            if !still_active {
                break;
            }

            // lint-allow-unused: ClientEvent has dozens of variants; this
            // event handler only wires the events it cares about and intentionally
            // drops everything else (incl. future-added variants).
            #[allow(clippy::wildcard_enum_match_arm, clippy::match_same_arms)]
            match event {
                ClientEvent::MessageReceived {
                    ref channel_id,
                    ref message,
                } => {
                    let selected = nav.read().selected_channel.clone();
                    if selected.as_deref() == Some(channel_id.as_str()) {
                        // Currently viewing this channel — append message live.
                        let msg_c = message.clone();
                        chat_view_state.batch(move |cv| cv.push_message(msg_c));
                        tracing::trace!(
                            "Live message in #{channel_id}: {}",
                            message.author.display_name
                        );
                        // Auto-scroll to bottom when the user is near the tail;
                        // otherwise the Jump to Present button will appear.
                        let at_bottom = read_message_list_scroll_metrics().await.is_some_and(
                            |(scroll_top, scroll_height)| {
                                // Compute client height is not directly available here,
                                // so approximate: at bottom when scrollHeight - scrollTop
                                // is small (near zero means scrolled to very bottom).
                                // In practice scrollHeight - (scrollTop + clientHeight) < threshold.
                                // We check the simpler: scrollHeight - scrollTop <= a large value
                                // that catches "near bottom".  Use scrollHeight - scrollTop < threshold
                                // where threshold accounts for typical viewport heights.
                                scroll_height - scroll_top < AUTO_SCROLL_THRESHOLD_PX + 800.0_f64
                            },
                        );
                        if at_bottom {
                            request_scroll_to_bottom();
                        }
                    }
                    // TODO(phase-3): increment unread count for other channels
                }
                ClientEvent::PresenceChanged {
                    ref user_id,
                    status,
                } => {
                    let user_id_c = user_id.clone();
                    chat_view_state.batch(move |cv| {
                        for member in &mut cv.members {
                            if member.id == user_id_c {
                                member.presence = status;
                                break;
                            }
                        }
                    });
                }
                // lint-allow-unused: arm body is empty same as the wildcard,
                // but kept as a documentation hook for the future TODO below.
                #[allow(clippy::match_same_arms)]
                ClientEvent::TypingStarted { .. } => {
                    // TODO(phase-3): show typing indicator in chat view
                }
                ClientEvent::SidebarInvalidated => {
                    // P28 — bump the tick so `ClientSidebar`'s
                    // `use_resource` re-fetches `get_sidebar_declaration`.
                    app_state.batch(|s| {
                        s.sidebar_invalidated_tick =
                            s.sidebar_invalidated_tick.wrapping_add(1);
                    });
                }
                // D.3 — route to the incoming-call screen when a DM call rings.
                ClientEvent::IncomingCall { ref dm_id, .. } => {
                    let session = account_sessions
                        .peek()
                        .account_sessions
                        .get(&account_id)
                        .cloned();
                    if let Some(session) = session {
                        let backend_slug = session.backend.slug().to_string();
                        let instance_id = session.instance_id.clone();
                        let account_id_c = account_id.clone();
                        let dm_id_c = dm_id.clone();
                        // crate::nav! is safe inside spawn — spawned tasks run in
                        // the same component scope (caller's scope, not yet dropped).
                        crate::nav!(Route::DmIncomingCall {
                            backend: backend_slug,
                            instance_id,
                            account_id: account_id_c,
                            dm_id: dm_id_c,
                        });
                    }
                }
                // C.3 — a remote user joined a voice channel the local user is in.
                // Update the participant list so the grid renders the new tile.
                ClientEvent::VoiceUserJoined {
                    ref channel_id,
                    ref participant,
                } => {
                    let cid = channel_id.clone();
                    let p = participant.clone();
                    voice_state.batch(move |v| {
                        let list = v.voice_channel_participants.entry(cid).or_default();
                        if !list.iter().any(|existing| existing.user.id == p.user.id) {
                            list.push(p);
                        }
                    });
                }
                // C.3 — a remote user left a voice channel.
                ClientEvent::VoiceUserLeft {
                    ref channel_id,
                    ref user_id,
                } => {
                    let cid = channel_id.clone();
                    let uid = user_id.clone();
                    voice_state.batch(move |v| {
                        if let Some(list) = v.voice_channel_participants.get_mut(&cid) {
                            list.retain(|p| p.user.id != uid);
                        }
                        // Also remove from speaking map so indicator clears.
                        if let Some(speaking) = v.voice_speaking_map.get_mut(&cid) {
                            speaking.remove(&uid);
                        }
                    });
                }
                // C.3 — a voice participant's state changed (muted, deafened, stream, etc.).
                // Uses set_if_changed-equivalent logic to avoid hang class #8
                // (self-firing effects when participant state notifies subscribers).
                ClientEvent::VoiceStateUpdated {
                    ref channel_id,
                    ref participant,
                } => {
                    let cid = channel_id.clone();
                    let p = participant.clone();
                    voice_state.batch(move |v| {
                        if let Some(list) = v.voice_channel_participants.get_mut(&cid) {
                            for existing in list.iter_mut() {
                                if existing.user.id == p.user.id {
                                    // Only write when something actually changed (hang class #8).
                                    let changed = existing.is_muted != p.is_muted
                                        || existing.is_deafened != p.is_deafened
                                        || existing.is_streaming != p.is_streaming
                                        || existing.is_video_on != p.is_video_on
                                        || existing.is_speaking != p.is_speaking;
                                    if changed {
                                        *existing = p;
                                    }
                                    break;
                                }
                            }
                        }
                    });
                }
                // C.4 — update the per-channel speaking map when a remote participant
                // starts or stops speaking. Uses set_if_changed to avoid hang class #8
                // (self-firing effects on the speaking signal).
                ClientEvent::VoiceSpeakingUpdate {
                    ref channel_id,
                    ref user_id,
                    is_speaking,
                } => {
                    let channel_id_c = channel_id.clone();
                    let user_id_c = user_id.clone();
                    voice_state.batch(move |v| {
                        let entry = v.voice_speaking_map
                            .entry(channel_id_c)
                            .or_default();
                        let current = entry.get(&user_id_c).copied().unwrap_or(false);
                        // set_if_changed equivalent: only update when value actually changes.
                        if current != is_speaking {
                            entry.insert(user_id_c, is_speaking);
                        }
                    });
                }
                // Stoat (and future backends) emit this when the Bonfire/gateway WS
                // successfully authenticates. Update connection_statuses so the startup
                // overlay shows "connected" rather than "disconnected" / "cached".
                ClientEvent::ConnectionStateChanged { connected, .. } => {
                    let aid = account_id.clone();
                    client_manager.batch(move |cm| {
                        let status = if connected {
                            poly_client::ConnectionStatus::Connected
                        } else {
                            poly_client::ConnectionStatus::Disconnected
                        };
                        cm.connection_statuses.insert(aid, status);
                    });
                }
                // lint-allow-unused: ClientEvent has dozens of variants;
                // the handler only wires the explicitly handled ones.
                #[allow(clippy::wildcard_enum_match_arm)]
                _ => {}
            }
        }

        tracing::debug!("Event stream ended for account: {account_id}");
    });
}
