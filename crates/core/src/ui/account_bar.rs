//! Account status bar — shown at the bottom of the channel list.
//!
//! Displays the current user's info: avatar, display name, online status,
//! and quick-action buttons (mic mute, audio mute, settings gear).
//! Like Discord's bottom-left user panel.
// TODO(phase-2.5.19): Account status bar

use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData, SettingsSection, View};
use dioxus::prelude::*;

/// Account bar component.
///
/// Shows user avatar + name + status + quick controls at the
/// bottom of the channel list panel.
#[component]
pub fn AccountBar() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let voice_conn = chat_data.read().voice_connection.clone();

    // Demo user info — in real impl this comes from the active session
    let user_name = "Demo User";
    let user_id = "demo-user-self";
    let status_text = t("user-online");
    let color = user_color(user_id);
    let first_char = "D";

    let is_muted = voice_conn.as_ref().is_some_and(|vc| vc.is_muted);
    let is_deafened = voice_conn.as_ref().is_some_and(|vc| vc.is_deafened);

    rsx! {
        div { class: "account-bar",
            // User info
            div { class: "account-bar-user",
                div {
                    class: "account-avatar",
                    style: "background-color: {color};",
                    "{first_char}"
                }
                div { class: "account-info",
                    div { class: "account-name", "{user_name}" }
                    div { class: "account-status",
                        span { class: "status-dot online" }
                        span { class: "account-status-text", "{status_text}" }
                    }
                }
            }
            // Quick controls
            div { class: "account-bar-controls",
                // Mic mute toggle
                button {
                    class: if is_muted { "account-btn active" } else { "account-btn" },
                    title: if is_muted { t("voice-unmute") } else { t("voice-mute") },
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
                // Deafen toggle
                button {
                    class: if is_deafened { "account-btn active" } else { "account-btn" },
                    title: if is_deafened { t("voice-undeafen") } else { t("voice-deafen") },
                    onclick: move |_| {
                        if let Some(ref mut vc) = chat_data.write().voice_connection {
                            vc.is_deafened = !vc.is_deafened;
                        }
                    },
                    if is_deafened {
                        "🔕"
                    } else {
                        "🔊"
                    }
                }
                // Settings gear — opens Voice & Video settings directly
                button {
                    class: "account-btn",
                    title: "{t(\"nav-settings\")}",
                    onclick: move |_| {
                        app_state.write().settings_section = SettingsSection::VoiceVideo;
                        app_state.write().nav.view = View::Settings;
                    },
                    "⚙"
                }
            }
        }
    }
}
