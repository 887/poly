//! Account status bar — shown at the bottom of the channel list.
//!
//! Displays the current user's info: avatar, display name, online status,
//! and quick-action buttons (mic mute, audio mute, settings gear).
//! Like Discord's bottom-left user panel.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.19): Account status bar

use super::super::super::routes::Route;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData, SettingsSection};
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

    // Get the session for the current server's account.
    // In multi-account mode, the account bar shows the user for the account
    // that owns the currently-selected server.
    let session = {
        let reader = chat_data.read();
        reader
            .current_server
            .as_ref()
            .and_then(|server| reader.account_sessions.get(&server.account_id).cloned())
    };

    // Use session data if available, otherwise show fallback
    let (user_name, _user_id, status_text, color, first_char, avatar_url) =
        if let Some(ref s) = session {
            let name = s.user.display_name.clone();
            let id = s.user.id.clone();
            let fc: String = name
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default();
            (
                name,
                id.clone(),
                t("user-online"),
                user_color(&id),
                fc,
                s.user.avatar_url.clone(),
            )
        } else {
            (
                t("account-not-signed-in"),
                "no-session".to_string(),
                t("user-offline"),
                user_color("no-session"),
                "?".to_string(),
                None,
            )
        };

    let is_muted = voice_conn.as_ref().is_some_and(|vc| vc.is_muted);
    let is_deafened = voice_conn.as_ref().is_some_and(|vc| vc.is_deafened);

    rsx! {
        div { class: "account-bar",
            // User info
            div { class: "account-bar-user",
                div { class: "account-avatar",
                    if let Some(url) = &avatar_url {
                        img {
                            src: "{url}",
                            alt: "{user_name}",
                            class: "account-avatar-image",
                        }
                    } else {
                        div {
                            class: "account-avatar-fallback",
                            style: "background-color: {color};",
                            "{first_char}"
                        }
                    }
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
                        navigator().push(Route::SettingsRoute);
                    },
                    "⚙"
                }
            }
        }
    }
}
