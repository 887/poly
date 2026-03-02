//! Voice connection bar — persistent bar showing active voice/video connection.
//!
//! Displayed at the bottom of the channel list when the user is connected
//! to a voice or video channel. Shows:
//! - "Voice Connected" / "Video Connected" status
//! - Channel name + server name
//! - Mute mic toggle, deafen toggle, disconnect button
//!
//! Like Discord's voice status bar at the bottom-left.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.14): Voice connection bar

use crate::i18n::t;
use crate::state::ChatData;
use dioxus::prelude::*;

/// Voice connection bar component.
///
/// Rendered at the bottom of the channel list when the user is in a voice/video call.
#[component]
pub fn VoiceBar() -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let voice_conn = chat_data.read().voice_connection.clone();

    let Some(conn) = voice_conn else {
        return rsx! {};
    };

    let channel_name = conn.channel_name.clone();
    let server_name = conn.server_name.clone();
    let is_video_on = conn.is_video_on;
    let is_streaming = conn.is_streaming;

    rsx! {
        div { class: "voice-bar",
            // Top row: status/channel info + signal/hangup icons
            div { class: "voice-bar-top",
                div { class: "voice-bar-info",
                    div { class: "voice-bar-status",
                        span { class: "voice-bar-dot" }
                        span { class: "voice-bar-status-text", "{t(\"voice-connected\")}" }
                    }
                    div { class: "voice-bar-channel",
                        "{channel_name} / {server_name}"
                    }
                }
                div { class: "voice-bar-quick",
                    // Signal quality indicator (non-interactive for now)
                    span {
                        class: "voice-bar-signal",
                        title: "{t(\"voice-signal-quality\")}",
                        "📶"
                    }
                    // Hang up / disconnect
                    button {
                        class: "voice-bar-btn voice-bar-hangup",
                        title: "{t(\"voice-disconnect\")}",
                        onclick: move |_| {
                            chat_data.write().voice_connection = None;
                        },
                        "📵"
                    }
                }
            }
            // Middle row: media control buttons
            div { class: "voice-bar-media",
                // Camera toggle
                button {
                    class: if is_video_on { "voice-bar-media-btn active" } else { "voice-bar-media-btn" },
                    title: "{t(\"voice-camera\")}",
                    onclick: move |_| {
                        if let Some(ref mut vc) = chat_data.write().voice_connection {
                            vc.is_video_on = !vc.is_video_on;
                        }
                    },
                    "📹"
                }
                // Screen share toggle
                button {
                    class: if is_streaming { "voice-bar-media-btn active" } else { "voice-bar-media-btn" },
                    title: "{t(\"voice-screen-share\")}",
                    onclick: move |_| {
                        if let Some(ref mut vc) = chat_data.write().voice_connection {
                            vc.is_streaming = !vc.is_streaming;
                        }
                    },
                    "🖥"
                }
                // Activity — disabled/placeholder
                button {
                    class: "voice-bar-media-btn disabled",
                    title: "{t(\"voice-activity\")}",
                    disabled: true,
                    "🎮"
                }
                // Voiceboard — disabled/placeholder
                button {
                    class: "voice-bar-media-btn disabled",
                    title: "{t(\"voice-voiceboard\")}",
                    disabled: true,
                    "📋"
                }
            }
        }
    }
}
