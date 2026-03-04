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

    // Get the session for the active account using the nav state's active_account_id.
    // This is set reliably by sync_route_to_app_state for ALL account-scoped routes
    // (ServerHome, ServerChat, ServerSettingsRoute, AccountSettingsRoute, etc.).
    // We cannot use chat_data.current_server because it is only populated when browsing
    // channels, not when viewing settings pages.
    let session = {
        let app = app_state.read();
        let data = chat_data.read();
        app.nav
            .active_account_id
            .as_deref()
            .and_then(|aid| data.account_sessions.get(aid).cloned())
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
                // Disabled when viewing server settings (selected_server is set)
                button {
                    class: "account-btn",
                    disabled: app_state.read().nav.selected_server.is_some(),
                    title: "{t(\"nav-settings\")}",
                    onclick: move |_| {
                        if app_state.read().nav.selected_server.is_none() {
                            app_state.write().settings_section = SettingsSection::VoiceVideo;
                            navigator().push(Route::SettingsRoute);
                        }
                    },
                    "⚙"
                }
            }
        }
    }
}
