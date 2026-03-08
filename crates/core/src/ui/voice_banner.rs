//! Voice connection banner — full-width bar shown when connected to any voice channel.
//!
//! Rendered at the very top of the app, spanning server sidebar, channel list,
//! and chat view. Shows:
//! - Left: participant avatars (first few) + count
//! - Center: server name / channel name (clickable — navigates to the channel)
//! - Right: mute mic, deafen, disconnect buttons
//!
//! Only rendered when `chat_data.voice_connection` is `Some`.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5): Voice connection banner — tracked in overall-plan.md

use super::routes::Route;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData};
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use dioxus::prelude::*;

#[rustfmt::skip]
#[component]
fn VoiceBannerParticipants(participants: Vec<poly_client::VoiceParticipant>) -> Element {
    let participant_count = participants.len();
    let display_participants = participants.into_iter().take(4).collect::<Vec<_>>();

    rsx! {
        div { class: "voice-banner-left",
            div { class: "voice-banner-avatars",
                for participant in &display_participants {
                    div {
                        class: "voice-banner-avatar",
                        title: "{participant.user.display_name}",
                        if let Some(url) = &participant.user.avatar_url {
                            img {
                                src: "{url}",
                                alt: "{participant.user.display_name}",
                                class: "voice-banner-avatar-image",
                            }
                        } else {
                            div {
                                class: "voice-banner-avatar-fallback",
                                style: "background-color: {user_color(&participant.user.id)};",
                                "{participant.user.display_name.chars().next().unwrap_or('?')}"
                            }
                        }
                    }
                }
            }
            if participant_count > 0 {
                span { class: "voice-banner-count", "{participant_count} {t(\"voice-in-channel\")}" }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn VoiceBannerChannelLink(
    channel_id: String,
    server_id: String,
    channel_name: String,
    server_name: String,
    backend_slug: String,
    instance_id: String,
    account_id: String,
    app_state: Signal<AppState>,
) -> Element {
    rsx! {
        button {
            class: "voice-banner-center",
            title: "{t(\"voice-go-to-channel\")}",
            onclick: move |_| {
                if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone()
                {
                    remember_message_list_scroll_position(&previous_channel_id);
                }
                app_state.write().nav.selected_server = Some(server_id.clone());
                app_state.write().nav.selected_channel = Some(channel_id.clone());
                navigator()
                    .push(Route::ServerChat {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                        server_id: server_id.clone(),
                        channel_id: channel_id.clone(),
                    });
            },
            span { class: "voice-banner-icon", "🔊" }
            span { class: "voice-banner-channel", "{channel_name}" }
            span { class: "voice-banner-server", "— {server_name}" }
        }
    }
}

#[rustfmt::skip]
#[component]
fn VoiceBannerControls(
    is_muted: bool,
    is_deafened: bool,
    mut chat_data: Signal<ChatData>,
) -> Element {
    let mute_title = if is_muted {
        t("voice-unmute-mic")
    } else {
        t("voice-mute-mic")
    };
    let deafen_title = if is_deafened {
        t("voice-undeafen")
    } else {
        t("voice-deafen")
    };

    rsx! {
        div { class: "voice-banner-controls",
            button {
                class: if is_muted { "voice-ctrl-btn muted" } else { "voice-ctrl-btn" },
                title: "{mute_title}",
                onclick: move |_| {
                    if let Some(ref mut vc) = chat_data.write().voice_connection {
                        vc.is_muted = !vc.is_muted;
                    }
                },
                if is_muted {
                    "🔇"
                } else {
                    "🎤"
                }
            }
            button {
                class: if is_deafened { "voice-ctrl-btn muted" } else { "voice-ctrl-btn" },
                title: "{deafen_title}",
                onclick: move |_| {
                    if let Some(ref mut vc) = chat_data.write().voice_connection {
                        vc.is_deafened = !vc.is_deafened;
                    }
                },
                if is_deafened {
                    "🔕"
                } else {
                    "🔔"
                }
            }
            button {
                class: "voice-ctrl-btn disconnect",
                title: "{t(\"voice-disconnect\")}",
                onclick: move |_| {
                    chat_data.write().voice_connection = None;
                },
                "📵"
            }
        }
    }
}

/// Full-width voice connection banner.
///
/// Spans all columns (server sidebar, channel list, chat area) and sits
/// at the top of the layout. Hidden when not in a voice channel.
#[rustfmt::skip]
#[component]
pub fn VoiceBanner() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let voice_conn = chat_data.read().voice_connection.clone();

    let Some(conn) = voice_conn else {
        return rsx! {};
    };

    let participants = chat_data
        .read()
        .voice_channel_participants
        .get(&conn.channel_id)
        .cloned()
        .unwrap_or_default();

    let channel_id = conn.channel_id.clone();
    let server_id = conn.server_id.clone();
    let channel_name = conn.channel_name.clone();
    let server_name = conn.server_name.clone();
    let backend_slug = conn.backend.slug().to_string();
    let account_id = conn.account_id.clone();
    let instance_id = conn.instance_id.clone();
    let is_muted = conn.is_muted;
    let is_deafened = conn.is_deafened;

    rsx! {
        div { class: "voice-banner",
            VoiceBannerParticipants { participants }
            VoiceBannerChannelLink {
                channel_id,
                server_id,
                channel_name,
                server_name,
                backend_slug,
                instance_id,
                account_id,
                app_state,
            }
            VoiceBannerControls { is_muted, is_deafened, chat_data }
        }
    }
}
