//! Account server bar — per-account navigation (DMs, Notifications, Servers).
//!
//! This is the **second sidebar column** (Bar 2), shown whenever an account
//! is active (`active_account_id` is set in `NavigationState`).
//!
//! Shows:
//! 1. Conversations button (account-scoped)
//! 2. Friends/Ignore/Blocks management button (account-scoped)
//! 3. Notifications button (account-scoped, with unread badge)
//! 4. Separator
//! 5. All servers for the active account (drag-and-drop reorderable)
//! 5. Spacer
//!
//! ## Components
//! - [`AccountServerBar`] — root, orchestrates the column
//! - [`AccountBarDmsButton`] — conversations nav button
//! - [`AccountBarFriendsButton`] — friends/blocked management nav button
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
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::main_layout::{close_mobile_drawer, mobile_left_drawer_open};
use dioxus::prelude::*;

/// Compute the display-ordered server list for an account, respecting saved drag-drop ordering.
fn get_ordered_servers(
    chat_data: &ChatData,
    account_id: &str,
    account_servers: &[poly_client::Server],
) -> Vec<poly_client::Server> {
    if let Some(order) = chat_data.account_server_order.get(account_id) {
        let mut ordered: Vec<_> = order
            .iter()
            .filter_map(|id| account_servers.iter().find(|s| &s.id == id))
            .cloned()
            .collect();
        for s in account_servers {
            if !order.contains(&s.id) {
                ordered.push(s.clone());
            }
        }
        ordered
    } else {
        account_servers.to_vec()
    }
}

/// Compute the new server order after a Bar-2 drag-and-drop reorder.
fn compute_bar2_reorder(
    existing: Option<&Vec<String>>,
    all_servers: &[poly_client::Server],
    account_id: &str,
    drag_id: &str,
    target_id: &str,
) -> Vec<String> {
    let mut order: Vec<String> = existing.cloned().unwrap_or_else(|| {
        all_servers
            .iter()
            .filter(|s| s.account_id == account_id)
            .map(|s| s.id.clone())
            .collect()
    });
    if !order.contains(&drag_id.to_string()) {
        order.push(drag_id.to_string());
    }
    if let Some(from) = order.iter().position(|x| x == drag_id) {
        order.remove(from);
        if let Some(to) = order.iter().position(|x| x == target_id) {
            order.insert(to, drag_id.to_string());
        } else {
            order.push(drag_id.to_string());
        }
    }
    order
}

/// Apply a Bar-2 drop event: update `ChatData` with the new server order.
fn apply_bar2_drop(cd: &mut ChatData, drag_id: &str, target_id: &str, account_id: &str) {
    let order = compute_bar2_reorder(
        cd.account_server_order.get(account_id),
        &cd.servers,
        account_id,
        drag_id,
        target_id,
    );
    cd.account_server_order
        .insert(account_id.to_string(), order);
}

/// Persist the full per-account Bar-2 server order map to `AppSettings`.
///
/// Called after every reorder so the user's drag-drop order survives
/// page reloads, app restarts, and offline periods.
pub(crate) async fn persist_account_server_order(
    order: std::collections::HashMap<String, Vec<String>>,
) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    match s.get_app_settings().await {
        Ok(mut settings) => {
            settings.account_server_order = order;
            if let Err(e) = s.set_app_settings(&settings).await {
                tracing::warn!("Failed to persist account_server_order: {e}");
            }
        }
        Err(e) => tracing::warn!("Failed to read app_settings for Bar 2 order persist: {e}"),
    }
}

/// Account server bar — second sidebar column, per-account.
///
/// Only rendered when `active_account_id` is `Some(...)`.
/// Shows DMs, notifications, and all servers for this account.
#[rustfmt::skip]
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
        get_ordered_servers(&cd, &account_id, &account_servers)
    };

    // Count notifications for this account
    let notif_count = chat_data
        .read()
        .notifications
        .iter()
        .filter(|n| n.account_id == account_id)
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

            AccountBarFriendsButton {
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
                    unread: if server.backend.is_forum() { 0 } else { server.unread_count },
                    mention: if server.backend.is_forum() { 0 } else { server.mention_count },
                    is_selected: selected_server.as_deref() == Some(server.id.as_str()),
                    icon_url: server.icon_url.clone(),
                }
            }

            // Separator + "+" button to join/create a new server/guild.
            // Shown for all backends so the affordance is always discoverable.
            div { class: "sidebar-separator" }
            CreateServerButton { account_id: account_id.clone() }

            // Spacer keeps the icon rail aligned above the shared bottom account bar.
            div { class: "sidebar-spacer" }
        }
    }
}

/// A single draggable server icon in the account server bar.
///
/// Handles all drag-and-drop events, right-click context menu, and click navigation.
/// Extracted from the `AccountServerBar` for-loop to keep RSX macros small and
/// avoid Dioxus macro complexity limits inside `for` iterator blocks.
#[rustfmt::skip]
#[component]
fn AccountServerIcon(
    server_id: String,
    server_name: String,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
    unread: u32,
    /// Number of @mention notifications (shown as red badge).
    mention: u32,
    is_selected: bool,
    /// Optional server icon URL. When `Some`, rendered as an `<img>`; when
    /// `None`, falls back to a colored first-letter placeholder.
    icon_url: Option<String>,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let is_drag_over = chat_data.read().drag_over_id.as_deref() == Some(server_id.as_str());
    let item_class = match (is_selected, is_drag_over) {
        (true, true) => "server-icon active drag-over-target",
        (true, false) => "server-icon active",
        (false, true) => "server-icon drag-over-target",
        (false, false) => "server-icon",
    };

    // Pre-build closures to keep the RSX block compact.
    let sid_ctx = server_id.clone();
    let sname_ctx = server_name.clone();
    let aid_ctx = account_id.clone();
    let iid_ctx = instance_id.clone();
    let bslug_ctx = backend_slug.clone();
    let on_context_menu = move |evt: Event<MouseData>| {
        evt.prevent_default();
        evt.stop_propagation();
        let coords = evt.client_coordinates();
        app_state.write().context_menu = Some(ContextMenuState {
            x: coords.x,
            y: coords.y,
            server_id: sid_ctx.clone(),
            server_name: sname_ctx.clone(),
            account_id: aid_ctx.clone(),
            instance_id: iid_ctx.clone(),
            backend_slug: bslug_ctx.clone(),
        });
    };

    let sid_ds = server_id.clone();
    let on_drag_start = move |_: Event<DragData>| {
        let mut cd = chat_data.write();
        cd.dragging_server_id = Some(sid_ds.clone());
        cd.drag_source = DragSource::AccountServer;
    };

    let sid_do = server_id.clone();
    let on_drag_over = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        chat_data.write().drag_over_id = Some(sid_do.clone());
    };

    let sid_dl = server_id.clone();
    let on_drag_leave = move |_: Event<DragData>| {
        if chat_data.read().drag_over_id.as_deref() == Some(sid_dl.as_str()) {
            chat_data.write().drag_over_id = None;
        }
    };

    let tid = server_id.clone();
    let aid_drop = account_id.clone();
    let on_drop = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        let snapshot = {
            let mut cd = chat_data.write();
            let dragging = cd.dragging_server_id.clone();
            let src = cd.drag_source.clone();
            cd.drag_over_id = None;
            let Some(drag_id) = dragging else {
                cd.dragging_server_id = None;
                cd.drag_source = DragSource::None;
                return;
            };
            let did_reorder =
                matches!(src, DragSource::AccountServer) && drag_id != tid;
            if did_reorder {
                apply_bar2_drop(&mut cd, &drag_id, &tid, &aid_drop);
            }
            cd.dragging_server_id = None;
            cd.drag_source = DragSource::None;
            did_reorder.then(|| cd.account_server_order.clone())
        };
        if let Some(order) = snapshot {
            spawn(async move {
                persist_account_server_order(order).await;
            });
        }
    };

    let on_drag_end = move |_: Event<DragData>| {
        let mut cd = chat_data.write();
        cd.dragging_server_id = None;
        cd.drag_source = DragSource::None;
        cd.drag_over_id = None;
    };

    let sid_click = server_id.clone();
    let bslug_click = backend_slug.clone();
    let aid_click = account_id.clone();
    let on_click = move |_: Event<MouseData>| {
        let preserve_drawer_context = mobile_left_drawer_open();
        if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone() {
            remember_message_list_scroll_position(&previous_channel_id);
        }
        app_state.write().nav.selected_server = Some(sid_click.clone());
        app_state.write().nav.selected_channel = None;
        // Clear per-server transient data synchronously before the route change so
        // that `ServerHome` never sees stale `current_channel` / `current_server`
        // from a previous server or from demo data.  Without this, the stale channel
        // type can flip `ServerHome` into rendering `VoiceChannelView` even before
        // `load_server_data` fires, which requests audio permission and hard-crashes
        // Chromium on Linux.
        {
            let mut cd = chat_data.write();
            cd.current_server = None;
            cd.current_channel = None;
            cd.channels = Vec::new();
            cd.members = Vec::new();
            cd.messages = Vec::new();
        }
        if !preserve_drawer_context {
            let sid2 = sid_click.clone();
            spawn(async move {
                super::super::super::favorites_sidebar::load_server_data(
                    sid2,
                    app_state,
                    client_manager,
                    chat_data,
                )
                .await;
            });
        }
        navigator().push(Route::ServerHome {
            backend: bslug_click.clone(),
            instance_id: instance_id.clone(),
            account_id: aid_click.clone(),
            server_id: sid_click.clone(),
        });
    };

    rsx! {
        div {
            class: "{item_class}",
            draggable: "true",
            title: "{server_name}",
            oncontextmenu: on_context_menu,
            ondragstart: on_drag_start,
            ondragover: on_drag_over,
            ondragleave: on_drag_leave,
            ondrop: on_drop,
            ondragend: on_drag_end,
            onclick: on_click,

            ServerIconDisplay {
                icon_url: icon_url.clone(),
                server_name: server_name.clone(),
                server_id: server_id.clone(),
                unread,
                mention,
            }
        }
    }
}

/// Renders the visual content of a server icon: image (or letter fallback) plus notification badges.
///
/// Shows a red `@{mention}` badge for direct @mentions, and a small unread dot
/// when there are unread messages but no direct mentions.
#[rustfmt::skip]
#[component]
fn ServerIconDisplay(
    icon_url: Option<String>,
    server_name: String,
    server_id: String,
    unread: u32,
    /// Number of @mention notifications. When > 0, shows a red @badge.
    mention: u32,
) -> Element {
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let icon_color = user_color(&server_id);
    rsx! {
        if let Some(ref url) = icon_url {
            img {
                class: "server-icon-image",
                src: "{url}",
                alt: "{server_name}",
            }
        } else {
            div {
                class: "server-icon-letter",
                style: "background-color: {icon_color};",
                "{first_letter}"
            }
        }
        // @mention badge (red): only for direct @mentions.
        if mention > 0 {
            span { class: "badge mention-count-badge", "@{mention}" }
        } else if unread > 0 {
            // Unread-but-not-mentioned: show count (same as favorites bar for consistency)
            span { class: "badge mention-count-badge", "{unread}" }
        }
    }
}

/// Conversations button for the account server bar.
#[rustfmt::skip]
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
                if current_view == View::DmsFriends {
                    return;
                }
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

/// Friends / ignore / blocked management button for the account server bar.
#[rustfmt::skip]
#[component]
fn AccountBarFriendsButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    rsx! {
        div {
            class: if current_view == View::Friends { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Friends {
                    return;
                }
                app_state.write().nav.view = View::Friends;
                chat_data.write().current_server = None;
                chat_data.write().current_channel = None;
                chat_data.write().channels.clear();
                chat_data.write().messages.clear();
                chat_data.write().members.clear();
                navigator().push(Route::FriendsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            title: "{t(\"nav-friends\")}",
            div { class: "icon-dms", "👥" }
        }
    }
}

/// Notifications button for the account server bar.
#[rustfmt::skip]
#[component]
fn AccountBarNotifsButton(current_view: View, notif_count: usize) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let backend_slug = app_state
        .read()
        .nav
        .active_backend
        .as_ref()
        .map_or_else(|| "demo".to_string(), |backend| backend.slug().to_string());
    let instance_id = app_state
        .read()
        .nav
        .active_instance_id
        .clone()
        .unwrap_or_else(|| "demo".to_string());
    let account_id = app_state
        .read()
        .nav
        .active_account_id
        .clone()
        .unwrap_or_default();

    rsx! {
        div {
            class: if current_view == View::Notifications { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Notifications {
                    return;
                }
                app_state.write().nav.view = View::Notifications;
                let mut cd = chat_data.write();
                cd.current_server = None;
                cd.current_channel = None;
                cd.channels.clear();
                cd.messages.clear();
                cd.members.clear();
                navigator().push(Route::NotificationsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            title: "{t(\"nav-notifications\")}",
            div { class: "icon-notifications", "🔔" }
            if notif_count > 0 {
                span { class: "badge", "{notif_count}" }
            }
        }
    }
}

/// "+" button that lets Poly accounts create a new server/guild.
///
/// Navigates to the full-page Create Server route where FavoritesBar and
/// AccountServerBar remain visible. The inline form was replaced by the
/// full-page route to match the Settings/Signup page pattern.
#[rustfmt::skip]
#[component]
fn CreateServerButton(account_id: String) -> Element {
    let app_state: Signal<AppState> = use_context();
    let backend_slug = app_state
        .read()
        .nav
        .active_backend
        .as_ref()
        .map(|b| b.slug().to_string())
        .unwrap_or_else(|| "poly".to_string());
    let instance_id = app_state
        .read()
        .nav
        .active_instance_id
        .clone()
        .unwrap_or_default();

    rsx! {
        button {
            class: "create-server-pill",
            title: "{t(\"create-server-btn\")}",
            onclick: move |_| {
                navigator().push(Route::CreateServerRoute {
                    backend:     backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id:  account_id.clone(),
                });
                close_mobile_drawer();
            },
            "+"
        }
    }
}
