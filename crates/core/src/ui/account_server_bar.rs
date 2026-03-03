//! Account server bar — per-account navigation (DMs, Notifications, Servers).
//!
//! This is the **second sidebar column** (Bar 2), shown whenever an account
//! is active (`active_account_id` is set in `NavigationState`).
//!
//! Shows:
//! 1. DMs/Friends button (account-scoped)
//! 2. Notifications button (account-scoped, with unread badge)
//! 3. Separator
//! 4. All servers for the active account
//! 5. Spacer
//! 6. Account Settings gear
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::routes::Route;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData, SettingsSection, View};
use dioxus::prelude::*;

/// Account server bar — second sidebar column, per-account.
///
/// Only rendered when `active_account_id` is `Some(...)`.
/// Shows DMs, notifications, all servers for this account, and account settings.
#[component]
pub fn AccountServerBar() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let nav = app_state.read().nav.clone();
    let active_account_id = nav.active_account_id.clone();
    let active_backend = nav.active_backend;
    let current_view = nav.view;
    let selected_server = nav.selected_server.clone();

    // If no account is active, don't render
    let Some(account_id) = active_account_id else {
        return rsx! {};
    };

    let backend_slug = active_backend
        .map(|b| b.slug().to_string())
        .unwrap_or_else(|| "demo".to_string());

    // Get all servers for this account (not just favorites)
    let all_servers = chat_data.read().servers.clone();
    let account_servers: Vec<_> = all_servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .cloned()
        .collect();

    // Count unread notifications for this account
    let notif_count = chat_data
        .read()
        .notifications
        .iter()
        .filter(|n| !n.read && n.account_id == account_id)
        .count();

    rsx! {
        nav { class: "account-server-bar",
            // DMs / Friends button — account-scoped
            AccountBarDmsButton {
                current_view,
                backend_slug: backend_slug.clone(),
                account_id: account_id.clone(),
            }

            // Notifications button — account-scoped
            AccountBarNotifsButton { current_view, notif_count }

            // Separator
            div { class: "sidebar-separator" }

            // All servers for this account
            for server in &account_servers {
                {
                    let server_id = server.id.clone();
                    let server_name = server.name.clone();
                    let backend_slug_sv = server.backend.slug().to_string();
                    let account_id_sv = server.account_id.clone();
                    let unread = server.unread_count;
                    let is_selected = selected_server.as_deref() == Some(&server_id);
                    let first_letter: String = server_name
                        .chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    let icon_color = user_color(&server_id);
                    rsx! {
                        div {
                            class: if is_selected { "server-icon active" } else { "server-icon" },
                            onclick: {
                                let sid = server_id.clone();
                                let bslug = backend_slug_sv.clone();
                                let aid = account_id_sv.clone();
                                move |_| {
                                    app_state.write().nav.selected_server = Some(sid.clone());
                                    app_state.write().nav.selected_channel = None;
                                    let sid2 = sid.clone();
                                    spawn(async move {
                                        super::server_sidebar::load_server_data(
                                                sid2,
                                                app_state,
                                                client_manager,
                                                chat_data,
                                            )
                                            .await;
                                    });
                                    navigator()
                                        .push(Route::ServerHome {
                                            backend: bslug.clone(),
                                            account_id: aid.clone(),
                                            server_id: sid.clone(),
                                        });
                                }
                            },
                            title: "{server_name}",
                            div { class: "server-icon-letter", style: "background-color: {icon_color};", "{first_letter}" }
                            if unread > 0 {
                                span { class: "badge", "{unread}" }
                            }
                        }
                    }
                }
            }

            // Spacer
            div { class: "sidebar-spacer" }

            // Account settings gear
            div {
                class: "server-icon",
                onclick: move |_| {
                    app_state.write().settings_section = SettingsSection::Accounts;
                    navigator().push(Route::SettingsRoute);
                },
                title: "{t(\"account-settings\")}",
                div { class: "icon-settings", "⚙" }
            }
        }
    }
}

/// DMs/Friends button for the account server bar.
#[component]
fn AccountBarDmsButton(current_view: View, backend_slug: String, account_id: String) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();

    rsx! {
        div {
            class: if current_view == View::DmsFriends { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                chat_data.write().current_server = None;
                chat_data.write().current_channel = None;
                chat_data.write().channels.clear();
                chat_data.write().messages.clear();
                chat_data.write().members.clear();
                navigator()
                    .push(Route::DmsHome {
                        backend: backend_slug.clone(),
                        account_id: account_id.clone(),
                    });
            },
            title: "{t(\"nav-dms\")}",
            div { class: "icon-dms", "💬" }
        }
    }
}

/// Notifications button for the account server bar.
#[component]
fn AccountBarNotifsButton(current_view: View, notif_count: usize) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    rsx! {
        div {
            class: if current_view == View::Notifications { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                app_state.write().nav.view = View::Notifications;
                let mut cd = chat_data.write();
                cd.current_server = None;
                cd.current_channel = None;
                cd.channels.clear();
                cd.messages.clear();
                cd.members.clear();
                navigator().push(Route::NotificationsRoute);
            },
            title: "{t(\"nav-notifications\")}",
            div { class: "icon-notifications", "🔔" }
            if notif_count > 0 {
                span { class: "badge", "{notif_count}" }
            }
        }
    }
}
