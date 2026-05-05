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

use crate::state::BatchedSignal;
use super::routes::Route;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, NavState, VoiceState};
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::account::common::direct_call::{disconnect_active_call, swap_to_first_held_call};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::VoiceConnectionKind;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the voice connection banner.
#[derive(Debug, Clone)]
pub enum VoiceBannerAction {
    /// Toggle microphone mute.
    ToggleMute,
    /// Toggle deafen.
    ToggleDeafen,
    /// Disconnect from voice.
    Disconnect,
    /// Navigate to the voice channel.
    GoToChannel,
    /// Swap to first held call.
    SwapHeldCall,
}

impl UiAction for VoiceBannerAction {
    fn apply(self, cx: ActionCx<'_>) {
        let Some(voice_state) = dioxus::prelude::try_consume_context::<BatchedSignal<VoiceState>>()
        else {
            return;
        };
        match self {
            Self::ToggleMute => {
                voice_state.batch(|v| {
                    if let Some(ref mut vc) = v.voice_connection {
                        vc.is_muted = !vc.is_muted;
                    }
                });
            }
            Self::ToggleDeafen => {
                voice_state.batch(|v| {
                    if let Some(ref mut vc) = v.voice_connection {
                        vc.is_deafened = !vc.is_deafened;
                    }
                });
            }
            Self::Disconnect => {
                disconnect_active_call(voice_state);
            }
            Self::GoToChannel => {
                let conn = voice_state.peek().voice_connection.clone();
                let Some(conn) = conn else { return; };
                let Some(nav) = cx.navigator else { return; };
                if conn.kind == poly_client::VoiceConnectionKind::TemporaryCall {
                    if let Some(dm_id) = conn.dm_id {
                        nav.push(Route::DmChat {
                            backend: conn.backend.slug().to_string(),
                            instance_id: conn.instance_id,
                            account_id: conn.account_id,
                            dm_id,
                        });
                    }
                } else {
                    let nav_state: BatchedSignal<NavState> =
                        dioxus::prelude::try_consume_context().unwrap_or_else(|| {
                            BatchedSignal::from_signal(dioxus::prelude::Signal::new(
                                NavState::default(),
                            ))
                        });
                    if let Some(prev_channel_id) = nav_state.read().selected_channel.cloned() { // poly-lint: allow render-time-read — inside apply() event handler, not a render fn
                        remember_message_list_scroll_position(&prev_channel_id);
                    }
                    nav.push(Route::ServerChat {
                        backend: conn.backend.slug().to_string(),
                        instance_id: conn.instance_id,
                        account_id: conn.account_id,
                        server_id: conn.server_id,
                        channel_id: conn.channel_id,
                    });
                }
            }
            Self::SwapHeldCall => {
                swap_to_first_held_call(voice_state);
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn VoiceBannerParticipants(
    participants: Vec<poly_client::VoiceParticipant>,
    connection_kind: VoiceConnectionKind,
) -> Element {
    let participant_count = participants.len();
    let display_participants = participants.into_iter().take(4).collect::<Vec<_>>();
    let participant_label = if connection_kind == VoiceConnectionKind::TemporaryCall {
        t("voice-in-call")
    } else {
        t("voice-in-channel")
    };

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
                span { class: "voice-banner-count", "{participant_count} {participant_label}" }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn VoiceBannerChannelLink(
    channel_id: String,
    server_id: String,
    channel_name: String,
    server_name: String,
    backend_slug: String,
    instance_id: String,
    account_id: String,
    connection_kind: VoiceConnectionKind,
    dm_id: Option<String>,
    app_state: BatchedSignal<AppState>,
) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    rsx! {
        button {
            class: "voice-banner-center",
            title: if connection_kind == VoiceConnectionKind::TemporaryCall {
                t("voice-go-to-conversation")
            } else {
                t("voice-go-to-channel")
            },
            onclick: move |_| {
                if connection_kind == VoiceConnectionKind::TemporaryCall {
                    if let Some(dm_id) = dm_id.clone() {
                        crate::nav!(Route::DmChat {
                            backend: backend_slug.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                            dm_id,
                        });
                    }
                } else {
                    if let Some(previous_channel_id) = nav_state.read().selected_channel.cloned()
                    {
                        remember_message_list_scroll_position(&previous_channel_id);
                    }
                    navigator()
                        .push(Route::ServerChat {
                            backend: backend_slug.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                            server_id: server_id.clone(),
                            channel_id: channel_id.clone(),
                        });
                }
            },
            span { class: "voice-banner-icon", "🔊" }
            span { class: "voice-banner-channel", "{channel_name}" }
            span { class: "voice-banner-server", "— {server_name}" }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn VoiceBannerControls(
    is_muted: bool,
    is_deafened: bool,
    held_count: usize,
    voice_state: BatchedSignal<VoiceState>,
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
            if held_count > 0 {
                button {
                    class: "voice-ctrl-btn",
                    title: t("voice-swap-held-call"),
                    onclick: move |_| swap_to_first_held_call(voice_state),
                    "🔁"
                }
            }
            button {
                class: if is_muted { "voice-ctrl-btn muted" } else { "voice-ctrl-btn" },
                title: "{mute_title}",
                onclick: move |_| {
                    voice_state.batch(|v| {
                        if let Some(ref mut vc) = v.voice_connection {
                            vc.is_muted = !vc.is_muted;
                        }
                    });
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
                    voice_state.batch(|v| {
                        if let Some(ref mut vc) = v.voice_connection {
                            vc.is_deafened = !vc.is_deafened;
                        }
                    });
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
                    disconnect_active_call(voice_state);
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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(VoiceBannerAction)]
#[component]
pub fn VoiceBanner() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();

    let voice_conn = voice_state.read().voice_connection.clone();

    let Some(conn) = voice_conn else {
        return rsx! {};
    };

    let participants = voice_state
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
    let held_count = voice_state.read().held_voice_connections.len();
    let connection_kind = conn.kind;
    let dm_id = conn.dm_id.clone();

    let banner_class = if conn.kind == VoiceConnectionKind::TemporaryCall {
        "voice-banner voice-banner--temporary-call"
    } else {
        "voice-banner"
    };

    rsx! {
        div { class: "{banner_class}",
            VoiceBannerParticipants { participants, connection_kind }
            VoiceBannerChannelLink {
                channel_id,
                server_id,
                channel_name,
                server_name,
                backend_slug,
                instance_id,
                account_id,
                connection_kind,
                dm_id,
                app_state,
            }
            VoiceBannerControls { is_muted, is_deafened, held_count, voice_state }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn voice_banner_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<VoiceBannerAction>();
        let _ = VoiceBannerAction::ToggleMute;
        let _ = VoiceBannerAction::ToggleDeafen;
        let _ = VoiceBannerAction::Disconnect;
        let _ = VoiceBannerAction::GoToChannel;
        let _ = VoiceBannerAction::SwapHeldCall;
    }
}
