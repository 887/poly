//! Account status bar — shown at the bottom of the channel list.
//!
//! Displays the current user's info: avatar, display name, corner status badges,
//! and quick-action buttons (mic mute, audio mute, settings gear).
//! Matches the Discord-style bottom-left user panel.
//!
//! ## Corner status system (matches Bar 1 AccountIcon)
//! - **Top-right corner** — connection status ICON (⚡ connected / ↺ connecting /
//!   — disconnected / ⚠ error). Uses bright color-coded badge so the user can
//!   distinguish app connectivity from personal availability at a glance.
//! - **Bottom-right corner** — presence dot (green=online, orange=away, red=dnd,
//!   grey=appear-offline). User-settable by clicking the avatar.
//!
//! ## Account profile popup
//! Clicking the avatar opens `AccountProfilePopup` — a card showing name, status,
//! connection info, and an inline presence picker (no separate floating menu).
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.19): Account status bar

use crate::state::BatchedSignal;
use super::super::super::routes::Route;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
use poly_client::{AccountPresence, ConnectionStatus};
use poly_ui_macros::{context_menu, ui_action};

/// Snapshot of all rendering state for the account bar user panel.
#[derive(Clone, PartialEq)]
struct AccountBarUserState {
    user_name: String,
    /// Human-readable status: presence label when connected, "Offline" otherwise.
    status_text: String,
    color: String,
    first_char: String,
    avatar_url: Option<String>,
    /// CSS class for the connection dot: `"connected"` | `"connecting"` | `"disconnected"` | `"error"`.
    conn_class: &'static str,
    /// CSS class for the presence dot: `"online"` | `"away"` | `"dnd"` | `"appear-offline"`.
    presence_class: &'static str,
    /// Active account ID — needed by the presence picker to write back the chosen status.
    account_id: Option<String>,
}

fn current_account_bar_user(app_state: &AppState, chat_data: &ChatData) -> AccountBarUserState {
    let aid = app_state.nav.active_account_id.as_deref();
    let client_manager = use_context::<BatchedSignal<ClientManager>>();
    let cm = client_manager.read();

    let conn_class: &'static str = aid
        .and_then(|id| cm.connection_statuses.get(id))
        .map_or("disconnected", ConnectionStatus::css_class);
    let is_connected = conn_class == "connected";
    let presence: AccountPresence = aid
        .and_then(|id| cm.presence_statuses.get(id).copied())
        .unwrap_or(AccountPresence::Online);
    let presence_class = presence.css_class();
    // Drop read guard before accessing chat_data to avoid double-borrow.
    drop(cm);

    let session = aid.and_then(|id| chat_data.account_sessions.get(id).cloned());
    if let Some(s) = session {
        let name = s.user.display_name.clone();
        let id = s.user.id.clone();
        let status_text = if is_connected {
            presence.display_name().to_string()
        } else {
            t("user-offline")
        };
        return AccountBarUserState {
            first_char: name
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_default(),
            user_name: name,
            status_text,
            color: user_color(&id).to_string(),
            avatar_url: s.user.avatar_url.clone(),
            conn_class,
            presence_class,
            account_id: aid.map(str::to_string),
        };
    }
    AccountBarUserState {
        user_name: t("account-not-signed-in"),
        status_text: t("user-offline"),
        color: user_color("no-session").to_string(),
        first_char: "?".to_string(),
        avatar_url: None,
        conn_class: "disconnected",
        presence_class: "offline",
        account_id: None,
    }
}

/// User info section of the account bar.
///
/// Shows the avatar with corner status indicators (matching the AccountIcon in Bar 1):
/// - **Top-right**: connection status icon (⚡ live / ↺ connecting / — offline / ⚠ error)
/// - **Bottom-right**: presence colored dot (green=online, orange=away, etc.)
///
/// Clicking the avatar opens an account profile popup with the full presence picker.
/// The two-icon system is visually distinct from simple dots so users can tell at a glance
/// whether the app is live-connected vs just what their chosen presence is.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarUserInfo(user: AccountBarUserState) -> Element {
    let mut show_profile = use_signal(|| false);
    let has_account = user.account_id.is_some();

    // Icon (not a dot) for connection status — visually distinct per state.
    let conn_icon = match user.conn_class {
        "connected" => "⚡",    // lightning bolt  = live connection
        "connecting" => "↺",   // rotating arrows = syncing/connecting
        "disconnected" => "—",  // em dash         = offline by choice
        _ => "⚠",               // warning triangle = error state
    };

    rsx! {
        div { class: "account-bar-user",
            // Avatar wrapper — position:relative for corner badges.
            div {
                class: if has_account { "account-avatar account-avatar-clickable" } else { "account-avatar" },
                title: if has_account { t("account-profile-click-hint") } else { String::new() },
                onclick: move |_| {
                    if has_account {
                        show_profile.set(!show_profile());
                    }
                },
                if let Some(ref url) = user.avatar_url {
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
                // Top-right: connection icon badge (visually distinct from a dot).
                span {
                    class: "account-conn-icon account-conn-icon--{user.conn_class}",
                    title: "Connection: {user.conn_class}",
                    "{conn_icon}"
                }
                // Bottom-right: presence dot (colored, user-chosen availability).
                span {
                    class: "status-dot presence-dot {user.presence_class}",
                    title: "Presence: {user.presence_class}",
                }
            }
            div { class: "account-info",
                div { class: "account-name", "{user.user_name}" }
                div { class: "account-status-text", "{user.status_text}" }
            }
            // Account profile popup — opens when avatar is clicked.
            if *show_profile.read() {
                if let Some(ref account_id) = user.account_id {
                    AccountProfilePopup {
                        user: user.clone(),
                        account_id: account_id.clone(),
                        on_close: move |_| show_profile.set(false),
                    }
                }
            }
        }
    }
}

/// Account profile popup — shown when clicking the avatar in the account bar.
///
/// Displays the current user's own profile card (banner, avatar, name, status)
/// plus an inline presence picker to change availability without navigating away.
/// Positioned above the account bar; closes on backdrop click.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountProfilePopup(
    user: AccountBarUserState,
    account_id: String,
    on_close: EventHandler<()>,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let options: &[AccountPresence] = &[
        AccountPresence::Online,
        AccountPresence::Away,
        AccountPresence::DoNotDisturb,
        AccountPresence::AppearOffline,
    ];

    let conn_label = match user.conn_class {
        "connected" => t("account-conn-connected"),
        "connecting" => t("account-conn-connecting"),
        "disconnected" => t("account-conn-disconnected"),
        _ => t("account-conn-error"),
    };

    rsx! {
        // Semi-transparent backdrop — click anywhere outside to close.
        div {
            class: "user-popup-overlay",
            onclick: move |_| on_close.call(()),
            div {
                class: "user-popup account-profile-popup",
                onclick: move |evt| evt.stop_propagation(),
                // Banner strip
                div { class: "user-popup-banner" }
                // Avatar
                div { class: "user-popup-avatar",
                    if let Some(ref url) = user.avatar_url {
                        img {
                            class: "user-popup-avatar-image",
                            src: "{url}",
                            alt: "{user.user_name}",
                        }
                    } else {
                        div {
                            class: "user-popup-avatar-fallback",
                            style: "background-color: {user.color};",
                            "{user.first_char}"
                        }
                    }
                }
                // Name + status info
                div { class: "user-popup-info",
                    h3 { class: "user-popup-name", "{user.user_name}" }
                    // Connection status row (with icon)
                    div { class: "user-popup-conn-row",
                        span { class: "account-conn-icon account-conn-icon--{user.conn_class} small", }
                        span { class: "user-popup-conn-label", "{conn_label}" }
                    }
                    // Current presence status
                    div { class: "user-popup-status",
                        span { class: "status-dot {user.presence_class}" }
                        span { class: "user-popup-status-label", "{user.status_text}" }
                    }
                }
                // Divider
                div { class: "user-popup-divider" }
                // Presence picker (inline, not a floating menu)
                div { class: "account-profile-presence-section",
                    div { class: "presence-picker-title", "{t(\"status-picker-title\")}" }
                    for &presence in options {
                        {
                            let css = presence.css_class();
                            let label = presence.display_name().to_string();
                            let aid = account_id.clone();
                            let is_current = user.presence_class == css;
                            rsx! {
                                button {
                                    class: if is_current { "presence-picker-item active" } else { "presence-picker-item" },
                                    onclick: move |_| {
                                        let aid_c = aid.clone();
                                        client_manager.batch(move |cm| { cm.presence_statuses.insert(aid_c, presence); });
                                        on_close.call(());
                                    },
                                    span { class: "status-dot {css}" }
                                    span { class: "presence-picker-label", "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarControls(
    is_muted: bool,
    is_deafened: bool,
    app_state: BatchedSignal<AppState>,
    chat_data: BatchedSignal<ChatData>,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav = app_state.read().nav.clone();
    let backend_slug = nav
        .active_backend
        .cloned().map_or_else(|| "demo".to_string(), |backend| backend.slug().to_string());
    // Pack F (P61) — hide mic/deafen on backends with no voice support.
    let show_voice = client_manager.peek().capabilities_for_slug(&backend_slug).should_show_voice();
    let settings_target = nav.active_account_id.cloned().map(|account_id| {
        let instance_id = nav.active_instance_id.cloned().unwrap_or_else(|| "demo".to_string());
        Route::AccountSettingsRoute {
            backend: backend_slug.clone(),
            instance_id,
            account_id,
        }
    });

    rsx! {
        div { class: "account-bar-controls",
            if show_voice {
                button {
                    class: if is_muted { "account-btn active" } else { "account-btn" },
                    title: if is_muted { t("voice-unmute") } else { t("voice-mute") },
                    onclick: move |_| {
                        chat_data.batch(|cd| {
                            if let Some(ref mut vc) = cd.voice_connection {
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
                    class: if is_deafened { "account-btn active" } else { "account-btn" },
                    title: if is_deafened { t("voice-undeafen") } else { t("voice-deafen") },
                    onclick: move |_| {
                        chat_data.batch(|cd| {
                            if let Some(ref mut vc) = cd.voice_connection {
                                vc.is_deafened = !vc.is_deafened;
                            }
                        });
                    },
                    if is_deafened {
                        "🔕"
                    } else {
                        "🔊"
                    }
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
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn AccountBar() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let voice_conn = chat_data.read().voice_connection.clone();
    let st_snap = app_state.read().clone();
    let cd_snap = chat_data.read().clone();
    let user = current_account_bar_user(&st_snap, &cd_snap);

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
