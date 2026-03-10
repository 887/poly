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
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
struct AccountBarUserState {
    user_name: String,
    status_text: String,
    color: String,
    first_char: String,
    avatar_url: Option<String>,
}

fn current_account_bar_user(app_state: &AppState, chat_data: &ChatData) -> AccountBarUserState {
    let aid = app_state.nav.active_account_id.as_deref();
    let session = aid.and_then(|aid| chat_data.account_sessions.get(aid).cloned());
    let cm_signal = dioxus::prelude::use_context::<
        dioxus::prelude::Signal<crate::client_manager::ClientManager>,
    >();
    let is_offline = if let Some(aid) = aid {
        matches!(
            cm_signal.read().connection_statuses.get(aid),
            Some(poly_client::ConnectionStatus::Error(_))
                | Some(poly_client::ConnectionStatus::Disconnected)
        )
    } else {
        false
    };
    if let Some(s) = session {
        let name = s.user.display_name.clone();
        let id = s.user.id.clone();
        return AccountBarUserState {
            first_char: name
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default(),
            user_name: name,
            status_text: if is_offline {
                t("user-offline")
            } else {
                t("user-online")
            },
            color: user_color(&id).to_string(),
            avatar_url: s.user.avatar_url.clone(),
        };
    }
    AccountBarUserState {
        user_name: t("account-not-signed-in"),
        status_text: t("user-offline"),
        color: user_color("no-session").to_string(),
        first_char: "?".to_string(),
        avatar_url: None,
    }
}

#[rustfmt::skip]
#[component]
fn AccountBarUserInfo(user: AccountBarUserState) -> Element {
    rsx! {
        div { class: "account-bar-user",
            div { class: "account-avatar",
                if let Some(url) = &user.avatar_url {
                    img {
                        src: "{url}",
                        alt: "{user.user_name}",
                        class: "account-avatar-image",
                    }
                } else {
                    div {
                        class: "account-avatar-fallback",
                        style: "background-color: {user.color};",
                        "{user.first_char}"
                    }
                }
            }
            div { class: "account-info",
                div { class: "account-name", "{user.user_name}" }
                div { class: "account-status",
                    span { class: "status-dot online" }
                    span { class: "account-status-text", "{user.status_text}" }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn AccountBarControls(
    is_muted: bool,
    is_deafened: bool,
    app_state: Signal<AppState>,
    mut chat_data: Signal<ChatData>,
) -> Element {
    let nav = app_state.read().nav.clone();
    let settings_target = nav.active_account_id.clone().map(|account_id| {
        let backend = nav
            .active_backend
            .map(|backend| backend.slug().to_string())
            .unwrap_or_else(|| "demo".to_string());
        let instance_id = nav.active_instance_id.unwrap_or_else(|| "demo".to_string());
        Route::AccountSettingsRoute {
            backend,
            instance_id,
            account_id,
        }
    });

    rsx! {
        div { class: "account-bar-controls",
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
            button {
                class: "account-btn",
                disabled: settings_target.is_none(),
                title: "{t(\"account-settings\")}",
                onclick: move |_| {
                    if let Some(route) = settings_target.clone() {
                        navigator().push(route);
                    }
                },
                "⚙"
            }
        }
    }
}

/// Account bar component.
///
/// Shows user avatar + name + status + quick controls at the
/// bottom of the channel list panel.
#[rustfmt::skip]
#[component]
pub fn AccountBar() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let voice_conn = chat_data.read().voice_connection.clone();
    let user = current_account_bar_user(&app_state.read(), &chat_data.read());

    let is_muted = voice_conn.as_ref().is_some_and(|vc| vc.is_muted);
    let is_deafened = voice_conn.as_ref().is_some_and(|vc| vc.is_deafened);

    rsx! {
        div { class: "account-bar",
            AccountBarUserInfo { user }
            AccountBarControls {
                is_muted,
                is_deafened,
                app_state,
                chat_data,
            }
        }
    }
}
