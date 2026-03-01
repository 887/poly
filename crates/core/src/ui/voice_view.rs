//! Voice/Video channel view — participant tiles and join/disconnect controls.
//!
//! When a voice or video channel is selected in the channel list, this
//! component replaces the normal ChatView. It shows:
//! - Channel name + participant count in a header
//! - Responsive grid of participant tiles (avatar, name, status icons)
//! - "Join Voice"/"Join Video" button when not connected
//! - "Disconnect" button when connected
//!
//! Streaming participants get double-wide tiles.
// TODO(phase-2.5.14): Voice/Video channel view

use crate::i18n::t;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
use poly_client::ChannelType;

/// Voice channel view component.
///
/// Renders the voice/video call experience: participant grid,
/// join/leave buttons, and status indicators.
#[component]
pub fn VoiceChannelView() -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
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
            // ── Voice channel header ─────────────────────────────────
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

            // ── Participant grid ─────────────────────────────────────
            div { class: "voice-participants-area",
                if participants.is_empty() && !is_connected {
                    // Empty state — no one is in the channel
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
                    // Participant tiles
                    div { class: "voice-participants-grid",
                        for participant in &participants {
                            {
                                let user = &participant.user;
                                let color = user_color(&user.id);
                                let first_char: String = user
                                    .display_name
                                    .chars()
                                    .next()
                                    .map(|c| c.to_string())
                                    .unwrap_or_default();
                                let is_streaming = participant.is_streaming;
                                let tile_class = if is_streaming {
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
                                        div { class: "{speaking_class}", style: "background-color: {color};", "{first_char}" }
                                        div { class: "voice-tile-name", "{name}" }
                                        // Status icons row
                                        div { class: "voice-tile-icons", // Status icons row
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
                                        if is_streaming {
                                            div { class: "voice-stream-label", "{t(\"voice-watching-screen\")}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Join / Disconnect controls ───────────────────────────
            div { class: "voice-controls",
                if is_connected {
                    button {
                        class: "btn btn-voice-disconnect",
                        onclick: move |_| {
                            chat_data.write().voice_connection = None;
                        },
                        "{t(\"voice-disconnect\")}"
                    }
                } else {
                    button {
                        class: "btn btn-voice-join",
                        onclick: {
                            let cid = channel_id.clone();
                            move |_| {
                                if let Some(ref channel_id) = cid {
                                    let ch = chat_data.read().current_channel.clone();
                                    let srv = chat_data.read().current_server.clone();
                                    chat_data.write().voice_connection =
                                        Some(poly_client::VoiceConnection {
                                        channel_id: channel_id.clone(),
                                        server_id: srv
                                            .as_ref()
                                            .map(|s| s.id.clone())
                                            .unwrap_or_default(),
                                        channel_name: ch
                                            .as_ref()
                                            .map(|c| c.name.clone())
                                            .unwrap_or_default(),
                                        server_name: srv
                                            .as_ref()
                                            .map(|s| s.name.clone())
                                            .unwrap_or_default(),
                                        is_muted: false,
                                        is_deafened: false,
                                        is_streaming: false,
                                        is_video_on: false,
                                    });
                                }
                            }
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
}
