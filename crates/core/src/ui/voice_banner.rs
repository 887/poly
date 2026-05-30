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
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{NavState, VoiceState};
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::account::common::device_picker::DevicePickerToggle;
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
    /// Toggle camera on/off.
    ///
    /// Dispatches on `BackendCapabilities.video_capture`:
    /// - `Full` — toggles `VoiceState.is_video_on`; downstream observers
    ///   (VoiceChatBar JS_START_CAMERA / JS_STOP_CAMERA on web shell,
    ///   `DiscordVoiceBridgeClient::start_video_capture` on native wasm32)
    ///   react to the signal change. (Phase Y.4)
    /// - `None` — shows a "voice-video-coming-soon-camera" toast.
    ToggleCamera,
    /// Toggle screen share on/off.
    ///
    /// Dispatches on `BackendCapabilities.video_capture` (same field as camera):
    /// - `Full` — toggles `VoiceState.is_streaming`.
    /// - `None` — shows a "voice-video-coming-soon-screen" toast.
    ToggleScreenShare,
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
                // C.5 — update local state then signal the backend gateway.
                let conn_snapshot = {
                    let mut new_muted = false;
                    voice_state.batch(|v| {
                        if let Some(ref mut vc) = v.voice_connection {
                            vc.is_muted = !vc.is_muted;
                            new_muted = vc.is_muted;
                        }
                    });
                    voice_state.peek().voice_connection.as_ref().map(|vc| {
                        (vc.server_id.clone(), vc.channel_id.clone(), new_muted, vc.is_deafened)
                    })
                };
                if let (Some((server_id, channel_id, self_mute, self_deaf)), Some(cm)) = (
                    conn_snapshot,
                    dioxus::prelude::try_consume_context::<BatchedSignal<ClientManager>>(),
                ) {
                    spawn(async move {
                        if let Some((_account_id, backend)) =
                            cm.peek().get_backend_for_server(&server_id)
                            && let Ok(guard) = backend
                                .read_with_timeout(std::time::Duration::from_secs(5))
                                .await
                            {
                                let _ = guard
                                    .set_voice_mute(&server_id, &channel_id, self_mute, self_deaf)
                                    .await;
                            }
                    });
                }
            }
            Self::ToggleDeafen => {
                // C.5 — update local state then signal the backend gateway.
                let conn_snapshot = {
                    let mut new_deaf = false;
                    voice_state.batch(|v| {
                        if let Some(ref mut vc) = v.voice_connection {
                            vc.is_deafened = !vc.is_deafened;
                            new_deaf = vc.is_deafened;
                        }
                    });
                    voice_state.peek().voice_connection.as_ref().map(|vc| {
                        (vc.server_id.clone(), vc.channel_id.clone(), vc.is_muted, new_deaf)
                    })
                };
                if let (Some((server_id, channel_id, self_mute, self_deaf)), Some(cm)) = (
                    conn_snapshot,
                    dioxus::prelude::try_consume_context::<BatchedSignal<ClientManager>>(),
                ) {
                    spawn(async move {
                        if let Some((_account_id, backend)) =
                            cm.peek().get_backend_for_server(&server_id)
                            && let Ok(guard) = backend
                                .read_with_timeout(std::time::Duration::from_secs(5))
                                .await
                            {
                                let _ = guard
                                    .set_voice_mute(&server_id, &channel_id, self_mute, self_deaf)
                                    .await;
                            }
                    });
                }
            }
            Self::ToggleCamera => {
                // Phase E / OCP: dispatch on BackendCapabilities.video_capture rather
                // than a backend-slug ladder. Adding a new backend that supports video
                // only requires setting `video_capture: VideoCaptureCapability::Full`
                // in its capability declaration — no edit here needed.
                let (backend_slug, video_cap) = {
                    let vc_ref = voice_state.peek();
                    let vc = vc_ref.voice_connection.as_ref();
                    let slug = vc.map(|c| c.backend.slug().to_string()).unwrap_or_default();
                    let cap = dioxus::prelude::try_consume_context::<BatchedSignal<ClientManager>>()
                        .map_or(poly_client::VideoCaptureCapability::None, |cm| cm.peek().capabilities_for_slug(&slug).video_capture);
                    (slug, cap)
                };
                match video_cap {
                    poly_client::VideoCaptureCapability::Full => {
                        // Backend supports video — toggle signal; downstream observers
                        // (VoiceChatBar JS_START_CAMERA / JS_STOP_CAMERA on web shell,
                        // DiscordVoiceBridgeClient::start_video_capture on native wasm32)
                        // react to `is_video_on`. Phase Y.4 wiring point.
                        let next_on = voice_state
                            .peek()
                            .voice_connection
                            .as_ref()
                            .is_some_and(|vc| !vc.is_video_on);
                        tracing::debug!(
                            target: "poly_core::voice_banner",
                            backend = %backend_slug,
                            next_on,
                            "ToggleCamera → start/stop_video_capture wiring point (Phase Y.4)"
                        );
                        voice_state.batch(|v| {
                            if let Some(ref mut vc) = v.voice_connection {
                                vc.is_video_on = !vc.is_video_on;
                            }
                        });
                    }
                    poly_client::VideoCaptureCapability::None => {
                        // Backend does not support video capture — show a "coming soon" toast.
                        if let Some(toast_queue) = dioxus::prelude::try_consume_context::<
                            dioxus::prelude::Signal<Vec<crate::ui::client_ui::toast::ToastMessage>>,
                        >() {
                            crate::ui::client_ui::toast::push_toast(
                                toast_queue,
                                crate::ui::client_ui::toast::ToastMessage::new(
                                    "voice-video-coming-soon-camera",
                                    poly_client::ToastTone::Info,
                                ),
                            );
                        }
                    }
                }
            }
            Self::ToggleScreenShare => {
                // Phase E / OCP: same pattern as ToggleCamera — dispatch on
                // BackendCapabilities.video_capture, not a backend-slug ladder.
                let video_cap = {
                    let vc_ref = voice_state.peek();
                    let slug = vc_ref
                        .voice_connection
                        .as_ref()
                        .map(|c| c.backend.slug().to_string())
                        .unwrap_or_default();
                    dioxus::prelude::try_consume_context::<BatchedSignal<ClientManager>>()
                        .map_or(poly_client::VideoCaptureCapability::None, |cm| cm.peek().capabilities_for_slug(&slug).video_capture)
                };
                match video_cap {
                    poly_client::VideoCaptureCapability::Full => {
                        voice_state.batch(|v| {
                            if let Some(ref mut vc) = v.voice_connection {
                                vc.is_streaming = !vc.is_streaming;
                            }
                        });
                    }
                    poly_client::VideoCaptureCapability::None => {
                        if let Some(toast_queue) = dioxus::prelude::try_consume_context::<
                            dioxus::prelude::Signal<Vec<crate::ui::client_ui::toast::ToastMessage>>,
                        >() {
                            crate::ui::client_ui::toast::push_toast(
                                toast_queue,
                                crate::ui::client_ui::toast::ToastMessage::new(
                                    "voice-video-coming-soon-screen",
                                    poly_client::ToastTone::Info,
                                ),
                            );
                        }
                    }
                }
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
    channel_name: String,
    server_name: String,
    connection_kind: VoiceConnectionKind,
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
            onclick: move |_| crate::dispatch_action!(VoiceBannerAction::GoToChannel, nav_state, navigator()),
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
    /// Whether our camera is currently on.
    ///
    /// Phase E: reflects `VoiceState.is_video_on` signal state, which is kept
    /// in sync with the actual JS camera stream by `VoiceChatBar` in voice_view.rs.
    is_video_on: bool,
    /// Whether we are currently screen sharing.
    ///
    /// Phase E: reflects `VoiceState.is_streaming` signal state.
    is_streaming: bool,
    held_count: usize,
) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
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
    let camera_title = if is_video_on {
        t("voice-video-toggle-camera")
    } else {
        t("voice-video-toggle-camera")
    };
    let screen_title = t("voice-video-toggle-screen");

    rsx! {
        div { class: "voice-banner-controls",
            if held_count > 0 {
                button {
                    class: "voice-ctrl-btn",
                    title: t("voice-swap-held-call"),
                    onclick: move |_| crate::dispatch_action!(VoiceBannerAction::SwapHeldCall, nav_state, navigator()),
                    "🔁"
                }
            }
            button {
                class: if is_muted { "voice-ctrl-btn muted" } else { "voice-ctrl-btn" },
                title: "{mute_title}",
                onclick: move |_| crate::dispatch_action!(VoiceBannerAction::ToggleMute, nav_state, navigator()),
                if is_muted {
                    "🔇"
                } else {
                    "🎤"
                }
            }
            button {
                class: if is_deafened { "voice-ctrl-btn muted" } else { "voice-ctrl-btn" },
                title: "{deafen_title}",
                onclick: move |_| crate::dispatch_action!(VoiceBannerAction::ToggleDeafen, nav_state, navigator()),
                if is_deafened {
                    "🔕"
                } else {
                    "🔔"
                }
            }
            // Phase E: camera toggle. Dispatches ToggleCamera which handles
            // backend-specific toast notifications (Stoat/Teams) and Discord
            // signal state (VoiceChatBar handles the actual JS getUserMedia call).
            button {
                class: if is_video_on { "voice-ctrl-btn active" } else { "voice-ctrl-btn" },
                title: "{camera_title}",
                onclick: move |_| crate::dispatch_action!(VoiceBannerAction::ToggleCamera, nav_state, navigator()),
                "📹"
            }
            // Phase E: screen-share toggle. Same dispatch pattern as camera.
            button {
                class: if is_streaming { "voice-ctrl-btn active" } else { "voice-ctrl-btn" },
                title: "{screen_title}",
                onclick: move |_| crate::dispatch_action!(VoiceBannerAction::ToggleScreenShare, nav_state, navigator()),
                "🖥"
            }
            // J.1 — device picker gear icon (Phase J of plan-voice-video-calls.md)
            DevicePickerToggle {}
            button {
                class: "voice-ctrl-btn disconnect",
                title: "{t(\"voice-disconnect\")}",
                onclick: move |_| crate::dispatch_action!(VoiceBannerAction::Disconnect, nav_state, navigator()),
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
    let voice_state: BatchedSignal<VoiceState> = use_context();

    let voice_conn = voice_state.read().voice_connection.clone(); // poly-lint: allow render-time-read — drives conditional render; subscription IS the intent

    let Some(conn) = voice_conn else {
        return rsx! {};
    };

    let participants = voice_state
        .read() // poly-lint: allow render-time-read — drives participant avatar list; subscription IS the intent
        .voice_channel_participants
        .get(&conn.channel_id)
        .cloned()
        .unwrap_or_default();

    let channel_name = conn.channel_name.clone();
    let server_name = conn.server_name.clone();
    let is_muted = conn.is_muted;
    let is_deafened = conn.is_deafened;
    let is_video_on = conn.is_video_on;
    let is_streaming = conn.is_streaming;
    let held_count = voice_state.read().held_voice_connections.len(); // poly-lint: allow render-time-read — drives held-call swap button visibility; subscription IS the intent
    let connection_kind = conn.kind;

    let banner_class = if conn.kind == VoiceConnectionKind::TemporaryCall {
        "voice-banner voice-banner--temporary-call"
    } else {
        "voice-banner"
    };

    rsx! {
        div { class: "{banner_class}",
            VoiceBannerParticipants { participants, connection_kind }
            VoiceBannerChannelLink { channel_name, server_name, connection_kind }
            VoiceBannerControls { is_muted, is_deafened, is_video_on, is_streaming, held_count }
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
        let _ = VoiceBannerAction::ToggleCamera;
        let _ = VoiceBannerAction::ToggleScreenShare;
        let _ = VoiceBannerAction::Disconnect;
        let _ = VoiceBannerAction::GoToChannel;
        let _ = VoiceBannerAction::SwapHeldCall;
    }
}

