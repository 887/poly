//! Voice/Video channel view — participant tiles and join/disconnect controls.
//!
//! Common implementation shared across all messenger backends.
//!
//! When a voice or video channel is selected in the channel list, this
//! component replaces the normal ChatView. It shows:
//! - Channel name + participant count in a header
//! - Responsive grid of participant tiles (avatar, name, status icons)
//! - "Join Voice"/"Join Video" button when not connected
//! - "Disconnect" button when connected
//!
//! Streaming participants get double-wide tiles.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.14): Voice/Video channel view

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
use poly_client::{ChannelType, VoiceParticipant};

/// Voice channel view component.
///
/// Renders the voice/video call experience: participant grid,
/// join/leave buttons, and status indicators.
#[component]
pub fn VoiceChannelView() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let app_state: Signal<AppState> = use_context();

    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let channel_id = app_state.read().nav.selected_channel.clone();

    let participants = channel_id
        .as_deref()
        .and_then(|cid| {
            chat_data
                .read()
                .voice_channel_participants
                .get(cid)
                .cloned()
        })
        .unwrap_or_default();

    let is_connected = chat_data
        .read()
        .voice_connection
        .as_ref()
        .is_some_and(|vc| vc.channel_id == channel_id.clone().unwrap_or_default());

    let participant_count = participants.len();

    let channel_type = current_channel
        .as_ref()
        .map(|c| c.channel_type)
        .unwrap_or(ChannelType::Voice);

    let type_icon = match channel_type {
        ChannelType::Voice => "🔊",
        ChannelType::Video => "📹",
        ChannelType::Text => "#",
    };

    rsx! {
        main { class: "voice-view",
            VoiceHeader {
                current_channel: current_channel.clone(),
                current_server: current_server.clone(),
                type_icon,
                participant_count,
            }
            VoiceParticipantGrid {
                participants: participants.clone(),
                is_connected,
                channel_type,
            }
            VoiceControls {
                channel_id: channel_id.clone(),
                current_channel: current_channel.clone(),
                current_server: current_server.clone(),
                is_connected,
                channel_type,
                chat_data,
                client_manager,
                app_state,
            }
        }
    }
}

/// Header bar showing channel name, backend badge, and participant count.
#[component]
fn VoiceHeader(
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    type_icon: &'static str,
    participant_count: usize,
) -> Element {
    rsx! {
        div { class: "voice-header",
            if let Some(ref ch) = current_channel {
                span { class: "voice-channel-icon", "{type_icon}" }
                span { class: "voice-channel-name", "{ch.name}" }
                if let Some(ref server) = current_server {
                    span { class: "voice-source-badge",
                        "{backend_badge(&server.backend)} {server.backend.display_name()}"
                    }
                }
                span { class: "voice-participant-count", "👥 {participant_count}" }
            } else {
                span { class: "voice-channel-name", "{t(\"voice-no-channel\")}" }
            }
        }
    }
}

/// Grid of voice participant tiles.
#[component]
fn VoiceParticipantGrid(
    participants: Vec<VoiceParticipant>,
    is_connected: bool,
    channel_type: ChannelType,
) -> Element {
    rsx! {
        div { class: "voice-participants-area",
            if participants.is_empty() && !is_connected {
                div { class: "voice-empty",
                    div { class: "voice-empty-icon",
                        if channel_type == ChannelType::Video {
                            "📹"
                        } else {
                            "🔊"
                        }
                    }
                    h3 { "{t(\"voice-no-one-here\")}" }
                    p { "{t(\"voice-be-first\")}" }
                }
            } else {
                div { class: "voice-participants-grid",
                    for participant in &participants {
                        VoiceTile { participant: participant.clone() }
                    }
                }
            }
        }
    }
}

/// Single participant tile in the voice grid.
#[component]
fn VoiceTile(participant: VoiceParticipant) -> Element {
    let user = &participant.user;
    let color = user_color(&user.id);
    let first_char: String = user
        .display_name
        .chars()
        .next()
        .map(|c: char| c.to_string())
        .unwrap_or_default();

    let avatar_url = user.avatar_url.clone();

    let tile_class = if participant.is_streaming {
        "voice-tile voice-tile-streaming"
    } else {
        "voice-tile"
    };
    let speaking_class = if participant.is_speaking {
        "voice-avatar speaking"
    } else {
        "voice-avatar"
    };
    let name = user.display_name.clone();
    rsx! {
        div { class: "{tile_class}",
            div { class: "{speaking_class}",
                if let Some(url) = &avatar_url {
                    img {
                        src: "{url}",
                        alt: "{user.display_name}",
                        class: "voice-avatar-image",
                    }
                } else {
                    div {
                        class: "voice-avatar-fallback",
                        style: "background-color: {color};",
                        "{first_char}"
                    }
                }
            }
            div { class: "voice-tile-name", "{name}" }
            div { class: "voice-tile-icons",
                if participant.is_muted {
                    span {
                        class: "voice-status-icon muted",
                        title: "{t(\"voice-muted\")}",
                        "🔇"
                    }
                }
                if participant.is_deafened {
                    span {
                        class: "voice-status-icon deafened",
                        title: "{t(\"voice-deafened\")}",
                        "🔕"
                    }
                }
                if participant.is_streaming {
                    span {
                        class: "voice-status-icon streaming",
                        title: "{t(\"voice-streaming\")}",
                        "🖥"
                    }
                }
                if participant.is_video_on {
                    span {
                        class: "voice-status-icon video-on",
                        title: "{t(\"voice-video-on\")}",
                        "📹"
                    }
                }
            }
            if participant.is_streaming {
                div { class: "voice-stream-label", "{t(\"voice-watching-screen\")}" }
            }
        }
    }
}

/// Join / Disconnect buttons and voice control logic.
///
/// On join: fetches participant list from backend, adds local user,
/// sets `voice_connection`. On disconnect: removes local user from
/// participants and clears `voice_connection`.
#[component]
fn VoiceControls(
    channel_id: Option<String>,
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    is_connected: bool,
    channel_type: ChannelType,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
    app_state: Signal<AppState>,
) -> Element {
    rsx! {
        div { class: "voice-controls",
            if is_connected {
                button {
                    class: "btn btn-voice-disconnect",
                    onclick: move |_| {
                        // Use the voice connection's account_id to find the right session.
                        // Avoid local_session which is always the last-activated account
                        // (e.g. Cat/demo) regardless of which account owns this channel.
                        let local_id = {
                            let reader = chat_data.read();
                            reader
                                .voice_connection
                                .as_ref()
                                .and_then(|vc| reader.account_sessions.get(&vc.account_id))
                                .map(|s| s.user.id.clone())
                        };
                        let mut writer = chat_data.write();
                        if let Some(ref vc) = writer.voice_connection.clone()
                            && let Some(ref uid) = local_id
                            && let Some(ps) = writer
                                .voice_channel_participants
                                .get_mut(&vc.channel_id)
                        {
                            ps.retain(|p| &p.user.id != uid);
                        }
                        writer.voice_connection = None;
                    },
                    "{t(\"voice-disconnect\")}"
                }
            } else {
                button {
                    class: "btn btn-voice-join",
                    onclick: move |_| {
                        let Some(ref cid) = channel_id else {
                            return;
                        }; // Fetch participants via backend (no hardcoded data)
                        let cid = cid.clone();
                        let ch = chat_data.read().current_channel.clone();
                        let srv = chat_data.read().current_server.clone();
                        spawn(async move {
                            join_voice_channel(cid, ch, srv, client_manager, chat_data, app_state).await;
                        });
                    },
                    if channel_type == ChannelType::Video {
                        "{t(\"voice-join-video\")}"
                    } else {
                        "{t(\"voice-join-voice\")}"
                    }
                }
            }
        }
    }
}

/// Join a voice channel: fetch participants from backend, add local user, set connection.
async fn join_voice_channel(
    channel_id: String,
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    mut app_state: Signal<AppState>,
) {
    let server_id = app_state.read().nav.selected_server.clone();
    let Some(server_id) = server_id else { return };

    // Get backend for this server
    let backend_info = client_manager.read().get_backend_for_server(&server_id);
    let Some((voice_account_id, backend)) = backend_info else {
        return;
    };

    // Resolve the backend type from the current server for routing
    let voice_backend = current_server
        .as_ref()
        .map(|s| s.backend)
        .unwrap_or(poly_client::BackendType::Demo);

    // Fetch current participants from backend
    let mut participants = {
        let guard = backend.read().await;
        guard
            .get_voice_participants(&channel_id)
            .await
            .unwrap_or_default()
    };

    // Add the local (self) user to participants using the session for *this* account.
    // We must not fall back to `local_session` because that is always the most-recently
    // activated account (e.g. Cat/demo), whereas the voice channel may belong to a
    // different account entirely (e.g. Dog/demo2).
    let self_session = chat_data
        .read()
        .account_sessions
        .get(&voice_account_id)
        .cloned();
    if let Some(ref session) = self_session {
        let already_in = participants.iter().any(|p| p.user.id == session.user.id);
        if !already_in {
            participants.push(poly_client::VoiceParticipant {
                user: session.user.clone(),
                is_muted: false,
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            });
        }
    }

    // Store participants and set the voice connection
    chat_data
        .write()
        .voice_channel_participants
        .insert(channel_id.clone(), participants);

    chat_data.write().voice_connection = Some(poly_client::VoiceConnection {
        channel_id: channel_id.clone(),
        server_id: current_server
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or_default(),
        channel_name: current_channel
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default(),
        server_name: current_server
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        backend: voice_backend,
        account_id: voice_account_id,
        is_muted: false,
        is_deafened: false,
        is_streaming: false,
        is_video_on: false,
    });

    // Update nav state
    app_state.write().nav.selected_channel = Some(channel_id);
}
