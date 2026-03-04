//! Account server bar — per-account navigation (DMs, Notifications, Servers).
//!
//! This is the **second sidebar column** (Bar 2), shown whenever an account
//! is active (`active_account_id` is set in `NavigationState`).
//!
//! Shows:
//! 1. DMs/Friends button (account-scoped)
//! 2. Notifications button (account-scoped, with unread badge)
//! 3. Separator
//! 4. All servers for the active account (drag-and-drop reorderable)
//! 5. Spacer
//! 6. Account Settings gear
//!
//! ## Components
//! - [`AccountServerBar`] — root, orchestrates the column
//! - [`AccountBarDmsButton`] — DMs/Friends nav button
//! - [`AccountBarNotifsButton`] — Notifications nav button with badge
//! - [`AccountServerIcon`] — single draggable server icon with full DnD logic
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::super::super::routes::Route;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData, ContextMenuState, DragSource, View};
use dioxus::prelude::*;

/// Account server bar — second sidebar column, per-account.
///
/// Only rendered when `active_account_id` is `Some(...)`.
/// Shows DMs, notifications, all servers for this account, and account settings.
#[component]
pub fn AccountServerBar() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let nav = app_state.read().nav.clone();
    let active_account_id = nav.active_account_id.clone();
    let active_backend = nav.active_backend;
    let active_instance_id = nav.active_instance_id.clone();
    let current_view = nav.view;
    let selected_server = nav.selected_server.clone();

    // If no account is active, don't render
    let Some(account_id) = active_account_id else {
        return rsx! {};
    };

    let backend_slug = active_backend
        .map(|b| b.slug().to_string())
        .unwrap_or_else(|| "demo".to_string());

    let instance_id = active_instance_id.unwrap_or_else(|| "demo".to_string());

    // Get all servers for this account (not just favorites)
    let all_servers = chat_data.read().servers.clone();
    let account_servers: Vec<_> = all_servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .cloned()
        .collect();

    // Apply per-account ordering from drag-and-drop reordering.
    // Falls back to default (insertion) order if no ordering has been set.
    let ordered_account_servers = {
        let cd = chat_data.read();
        if let Some(order) = cd.account_server_order.get(&account_id) {
            let mut ordered: Vec<_> = order
                .iter()
                .filter_map(|id| account_servers.iter().find(|s| &s.id == id))
                .cloned()
                .collect();
            // Append servers not yet in the saved order (newly joined servers)
            for s in &account_servers {
                if !order.contains(&s.id) {
                    ordered.push(s.clone());
                }
            }
            ordered
        } else {
            account_servers.clone()
        }
    };

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
                instance_id: instance_id.clone(),
                account_id: account_id.clone(),
            }

            // Notifications button — account-scoped
            AccountBarNotifsButton { current_view, notif_count }

            // Separator
            div { class: "sidebar-separator" }

            // All servers for this account (ordered by drag-and-drop if reordered).
            // Each server is its own component to keep RSX macros manageable.
            for server in ordered_account_servers {
                AccountServerIcon {
                    key: "{server.id}",
                    server_id: server.id.clone(),
                    server_name: server.name.clone(),
                    backend_slug: server.backend.slug().to_string(),
                    instance_id: instance_id.clone(),
                    account_id: server.account_id.clone(),
                    unread: server.unread_count,
                    is_selected: selected_server.as_deref() == Some(server.id.as_str()),
                }
            }

            // Spacer
            div { class: "sidebar-spacer" }

            // Account settings gear — active only when viewing account-scoped settings,
            // NOT when viewing server settings (ServerSettingsRoute sets selected_server).
            div {
                class: if current_view == View::Settings && selected_server.is_none() { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    navigator()
                        .push(Route::AccountSettingsRoute {
                            backend: backend_slug.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                        });
                },
                title: "{t(\"account-settings\")}",
                div { class: "icon-settings", "⚙" }
            }
        }
    }
}

/// A single draggable server icon in the account server bar.
///
/// Handles all drag-and-drop events, right-click context menu, and click navigation.
/// Extracted from the `AccountServerBar` for-loop to keep RSX macros small and
/// avoid Dioxus macro complexity limits inside `for` iterator blocks.
#[component]
fn AccountServerIcon(
    server_id: String,
    server_name: String,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
    unread: u32,
    is_selected: bool,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let is_drag_over = chat_data.read().drag_over_id.as_deref() == Some(server_id.as_str());
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let icon_color = user_color(&server_id);
    let item_class = match (is_selected, is_drag_over) {
        (true, true) => "server-icon active drag-over-target",
        (true, false) => "server-icon active",
        (false, true) => "server-icon drag-over-target",
        (false, false) => "server-icon",
    };

    rsx! {
        div {
            class: "{item_class}",
            draggable: "true",
            title: "{server_name}",

            // Right-click → open context menu
            oncontextmenu: {
                let sid = server_id.clone();
                let sname = server_name.clone();
                let aid = account_id.clone();
                let iid = instance_id.clone();
                let bslug = backend_slug.clone();
                move |evt: Event<MouseData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    let coords = evt.client_coordinates();
                    app_state.write().context_menu = Some(ContextMenuState {
                        x: coords.x,
                        y: coords.y,
                        server_id: sid.clone(),
                        server_name: sname.clone(),
                        account_id: aid.clone(),
                        instance_id: iid.clone(),
                        backend_slug: bslug.clone(),
                    });
                }
            },

            // Drag start — mark as dragging from Bar 2
            ondragstart: {
                let sid = server_id.clone();
                move |_| {
                    let mut cd = chat_data.write();
                    cd.dragging_server_id = Some(sid.clone());
                    cd.drag_source = DragSource::AccountServer;
                }
            },

            // Drag over this item — set as Bar 2 reorder target
            ondragover: {
                let sid = server_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    chat_data.write().drag_over_id = Some(sid.clone());
                }
            },

            // Drag leave — clear highlight if we are still the target
            ondragleave: {
                let sid = server_id.clone();
                move |_| {
                    let currently_us =
                        chat_data.read().drag_over_id.as_deref()
                        == Some(sid.as_str());
                    if currently_us {
                        chat_data.write().drag_over_id = None;
                    }
                }
            },

            // Drop on this item — reorder within Bar 2
            ondrop: {
                let tid = server_id.clone();
                let aid_drop = account_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    let mut cd = chat_data.write();
                    let dragging = cd.dragging_server_id.clone();
                    let src = cd.drag_source.clone();
                    cd.drag_over_id = None;
                    let Some(drag_id) = dragging else {
                        cd.dragging_server_id = None;
                        cd.drag_source = DragSource::None;
                        return;
                    };
                    if matches!(src, DragSource::AccountServer) && drag_id != tid {
                        let base_order: Vec<String> = cd
                            .account_server_order
                            .get(&aid_drop)
                            .cloned()
                            .unwrap_or_else(|| {
                                cd.servers
                                    .iter()
                                    .filter(|s| s.account_id == aid_drop)
                                    .map(|s| s.id.clone())
                                    .collect()
                            });
                        let mut order = base_order;
                        if !order.contains(&drag_id) {
                            order.push(drag_id.clone());
                        }
                        if let Some(from) = order.iter().position(|x| *x == drag_id) {
                            order.remove(from);
                            if let Some(to) = order.iter().position(|x| *x == tid) {
                                order.insert(to, drag_id);
                            } else {
                                order.push(drag_id);
                            }
                        }
                        cd.account_server_order.insert(aid_drop.clone(), order);
                    }
                    cd.dragging_server_id = None;
                    cd.drag_source = DragSource::None;
                }
            },

            // Drag end — always clean up
            ondragend: move |_| {
                let mut cd = chat_data.write();
                cd.dragging_server_id = None;
                cd.drag_source = DragSource::None;
                cd.drag_over_id = None;
            },

            // Click — navigate to server home
            onclick: {
                let sid = server_id.clone();
                let bslug = backend_slug.clone();
                let aid = account_id.clone();
                move |_| {
                    app_state.write().nav.selected_server = Some(sid.clone());
                    app_state.write().nav.selected_channel = None;
                    let sid2 = sid.clone();
                    spawn(async move {
                        super::super::super::favorites_sidebar::load_server_data(
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
                            instance_id: instance_id.clone(),
                            account_id: aid.clone(),
                            server_id: sid.clone(),
                        });
                }
            },

            div {
                class: "server-icon-letter",
                style: "background-color: {icon_color};",
                "{first_letter}"
            }
            if unread > 0 {
                span { class: "badge", "{unread}" }
            }
        }
    }
}

/// DMs/Friends button for the account server bar.
#[component]
fn AccountBarDmsButton(
    current_view: View,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
) -> Element {
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
                        instance_id: instance_id.clone(),
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
