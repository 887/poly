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

use crate::state::BatchedSignal;
use super::super::super::routes::Route;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AccountSessions, AppState, ChatAction, ChatLists, ChatViewState, ContextMenuState, DragSource, DragState, NavState, UiOverlays, View};
use crate::ui::context_menu::menus::server_icon_entry_at;
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::favorites_sidebar::SidebarTooltip;
use crate::ui::main_layout::{close_mobile_drawer, mobile_left_drawer_open};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Compute the display-ordered server list for an account, respecting saved drag-drop ordering.
fn get_ordered_servers(
    account_sessions: &AccountSessions,
    account_id: &str,
    account_servers: &[poly_client::Server],
) -> Vec<poly_client::Server> {
    if let Some(order) = account_sessions.account_server_order.get(account_id) {
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

/// Apply a Bar-2 drop event: update `AccountSessions` with the new server order.
fn apply_bar2_drop(
    account_sessions: &mut AccountSessions,
    servers: &[poly_client::Server],
    drag_id: &str,
    target_id: &str,
    account_id: &str,
) {
    let order = compute_bar2_reorder(
        account_sessions.account_server_order.get(account_id),
        servers,
        account_id,
        drag_id,
        target_id,
    );
    account_sessions.account_server_order
        .insert(account_id.to_string(), order);
}

/// Account server bar — second sidebar column, per-account.
///
/// Only rendered when `active_account_id` is `Some(...)`.
/// Shows DMs, notifications, and all servers for this account.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn AccountServerBar() -> Element {
    let _app_state: BatchedSignal<AppState> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();

    let (active_account_id, active_backend, active_instance_id, current_view, selected_server) = {
        let nav = nav_state.read();
        (
            nav.active_account_id.cloned(),
            nav.active_backend.cloned(),
            nav.active_instance_id.cloned(),
            *nav.view,
            nav.selected_server.cloned(),
        )
    };

    // If no account is active, don't render
    let Some(account_id) = active_account_id else {
        return rsx! {};
    };

    let backend_slug = active_backend.map_or_else(|| "demo".to_string(), |b| b.slug().to_string());

    let instance_id = active_instance_id.unwrap_or_else(|| "demo".to_string());

    // Get all servers for this account (not just favorites)
    let all_servers = chat_lists.read().servers.clone();
    let account_servers: Vec<_> = all_servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .cloned()
        .collect();

    // Apply per-account ordering from drag-and-drop reordering.
    // Falls back to default (insertion) order if no ordering has been set.
    let ordered_account_servers = {
        let as_ = account_sessions.read();
        get_ordered_servers(&as_, &account_id, &account_servers)
    };

    // Count unread notifications for this account
    let notif_count = chat_lists
        .read()
        .notifications
        .iter()
        .filter(|n| !n.read && n.account_id == account_id)
        .count();

    // Pack F (P57) — capability-gate the per-account nav buttons. HN / Lemmy /
    // GitHub declare `dms/friends/notifications = None` and must not render
    // these buttons; Discord / Stoat / Matrix keep the full row.
    let caps = client_manager.peek().capabilities_for_slug(&backend_slug);
    let show_dms = caps.should_show_dms();
    let show_friends = caps.should_show_friends();
    let show_notifs = caps.should_show_notifications();
    let show_discover = caps.should_show_discover();

    // Forum-layout backends (Lemmy) store subscribed communities as servers in
    // `chat_data.servers`, populated at login/restore time. If the list is empty
    // for this account (e.g. first load before restore completes, or after
    // subscribing to a new community), trigger a background refresh so Bar-2
    // icons appear without requiring an app restart.
    //
    // Uses `use_resource` keyed on account_id so it re-fires when the user
    // switches accounts. Runs for ALL backends so chat-style accounts also
    // refresh on first mount (e.g. if restore was slow).
    // lint-allow-unused: use_resource returns a Resource handle that owns the
    // spawned future; we deliberately discard the handle so the resource lives
    // for the component's lifetime via Dioxus' runtime.
    #[allow(clippy::let_underscore_future, clippy::let_underscore_must_use)]
    let _ = {
        let account_id_res = account_id.clone();
        let backend_slug_res = backend_slug.clone();
        use_resource(move || {
            let account_id = account_id_res.clone();
            let backend_slug = backend_slug_res.clone();
            async move {
                let client_manager: BatchedSignal<ClientManager> =
                    match try_consume_context() {
                        Some(cm) => cm,
                        None => return,
                    };
                let servers = match client_manager.peek().with_backend_timeout(
                    &account_id,
                    std::time::Duration::from_secs(10),
                    async |b| b.get_servers().await,
                ).await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if servers.is_empty() {
                    return;
                }
                // Only add servers not already present (avoid duplicates on
                // repeated renders). The `account_id` field on each Server is
                // set by the backend so the AccountServerBar filter picks them up.
                let chat_lists: BatchedSignal<ChatLists> = match try_consume_context() {
                    Some(cl) => cl,
                    None => return,
                };
                let account_sessions: BatchedSignal<AccountSessions> = match try_consume_context() {
                    Some(as_) => as_,
                    None => return,
                };
                let is_forum = client_manager
                    .peek()
                    .capabilities_for_slug(&backend_slug)
                    .is_forum_layout();
                chat_lists.batch(move |cl| {
                    for srv in servers {
                        if !cl.servers.iter().any(|s| s.id == srv.id) {
                            cl.push_server(srv);
                        }
                    }
                });
                // Forum backends: ensure all servers are in favorited_server_ids
                // so they also appear in Bar-1 (favorites sidebar).
                if is_forum {
                    let srv_ids: Vec<String> = chat_lists.read()
                        .servers
                        .iter()
                        .filter(|s| s.account_id == account_id)
                        .map(|s| s.id.clone())
                        .collect();
                    account_sessions.batch(move |as_| {
                        for srv in srv_ids {
                            if !as_.favorited_server_ids.contains(&srv) {
                                as_.favorited_server_ids.push(srv);
                            }
                        }
                    });
                }
            }
        })
    };

    rsx! {
        nav { class: "account-server-bar",
            // Per-account Overview — first item in Bar 2 for every backend.
            // Routes to /{backend}/{instance}/{account}/overview which renders
            // the plugin-supplied get_account_overview_view ViewDescriptor.
            AccountBarOverviewButton {
                current_view,
                backend_slug: backend_slug.clone(),
                instance_id: instance_id.clone(),
                account_id: account_id.clone(),
            }

            // DMs / Friends button — account-scoped
            if show_dms {
                AccountBarDmsButton {
                    current_view,
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                }
            }

            if show_friends {
                AccountBarFriendsButton {
                    current_view,
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                }
            }

            // Notifications button — account-scoped
            if show_notifs {
                AccountBarNotifsButton { current_view, notif_count }
            }

            // Discover Communities button — only for backends with community_search support
            if show_discover {
                AccountBarDiscoverButton {
                    current_view,
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                }
            }

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
                    mention: server.mention_count,
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
#[ui_action(inherit)]
#[context_menu(inherit)]
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
    let _app_state: BatchedSignal<AppState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();
    let _client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let drag_state: BatchedSignal<DragState> = use_context();

    let is_drag_over = drag_state.read().drag_over_id.as_deref() == Some(server_id.as_str());
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
        ui_overlays.batch(|o| {
            o.context_menu_stack.push(server_icon_entry_at(
                ContextMenuState {
                    x: coords.x,
                    y: coords.y,
                    server_id: sid_ctx.clone(),
                    server_name: sname_ctx.clone(),
                    account_id: aid_ctx.clone(),
                    instance_id: iid_ctx.clone(),
                    backend_slug: bslug_ctx.clone(),
                },
                coords.x,
                coords.y,
            ));
        });
    };

    let sid_ds = server_id.clone();
    let on_drag_start = move |_: Event<DragData>| {
        drag_state.batch(|d| {
            d.dragging_server_id = Some(sid_ds.clone());
            d.drag_source = DragSource::AccountServer;
        });
    };

    let sid_do = server_id.clone();
    let on_drag_over = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        drag_state.batch(|d| d.drag_over_id = Some(sid_do.clone()));
    };

    let sid_dl = server_id.clone();
    let on_drag_leave = move |_: Event<DragData>| {
        if drag_state.read().drag_over_id.as_deref() == Some(sid_dl.as_str()) {
            drag_state.batch(|d| d.drag_over_id = None);
        }
    };

    let tid = server_id.clone();
    let aid_drop = account_id.clone();
    let on_drop = move |evt: Event<DragData>| {
        evt.prevent_default();
        evt.stop_propagation();
        // Snapshot and clear drag state before mutating chat_data.
        let (drag_id, src) = drag_state.batch(|d| {
            let drag_id = d.dragging_server_id.clone();
            let src = d.drag_source.clone();
            *d = DragState::default();
            (drag_id, src)
        });
        let Some(drag_id) = drag_id else {
            return;
        };
        if matches!(src, DragSource::AccountServer) && drag_id != tid {
            let servers_snapshot = chat_lists.read().servers.clone();
            account_sessions.batch(|as_| apply_bar2_drop(as_, &servers_snapshot, &drag_id, &tid, &aid_drop));
        }
    };

    let on_drag_end = move |_: Event<DragData>| {
        drag_state.batch(|d| { *d = DragState::default(); });
    };

    let sid_click = server_id.clone();
    let bslug_click = backend_slug.clone();
    let aid_click = account_id.clone();
    let on_click = move |_: Event<MouseData>| {
        if let Some(previous_channel_id) = nav_state.read().selected_channel.cloned() {
            remember_message_list_scroll_position(&previous_channel_id);
        }
        // Clear per-server transient data synchronously before the route change so
        // that `ServerHome` never sees stale `current_channel` / `current_server`
        // from a previous server or from demo data. Without this, the stale channel
        // type can flip `ServerHome` into rendering `VoiceChannelView` even before
        // `load_server_data` fires, which requests audio permission and hard-crashes
        // Chromium on Linux.
        chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
        chat_lists.batch(|cl| cl.set_channels(Vec::new()));
        // NOTE: do NOT spawn `load_server_data` here even when not in mobile
        // drawer context. `ServerHome::use_effect` (via `use_spawn_once`)
        // already kicks off the load when the route mounts. Spawning a second
        // copy from this click handler caused the Teams server-switch hang
        // (2026-04-25): two concurrent loaders racing on the same `chat_data`
        // signal compounded with the loading=true→false toggles from each,
        // re-firing every chat_data subscriber and starving the WASM
        // scheduler. The `mobile_left_drawer_open()` distinction also needs
        // to live in the route effect itself, not here, so there's exactly
        // one source of truth for "what gets loaded for this server".
        crate::nav!(Route::ServerHome {
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
            SidebarTooltip {
                line1: server_name.clone(),
                line2: Some(backend_slug.clone()),
                line3: None,
            }
        }
    }
}

/// Renders the visual content of a server icon: image (or letter fallback) plus notification badges.
///
/// Shows a red `@{mention}` badge for direct @mentions, and a small unread dot
/// when there are unread messages but no direct mentions.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
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

/// Per-account Overview button — first item in the AccountServerBar.
///
/// Routes to `Route::ServerOverviewRoute` which renders the plugin-supplied
/// `get_account_overview_view` ViewDescriptor inside the standard layout
/// (channel sidebar always present).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarOverviewButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    rsx! {
        div {
            class: if current_view == View::Overview { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Overview {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                navigator()
                    .push(Route::ServerOverviewRoute {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                    });
            },
            div { class: "icon-overview", "🏠" }
            SidebarTooltip {
                line1: t("account-bar-overview-tooltip"),
                line2: None,
                line3: None,
            }
        }
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarDmsButton(
    current_view: View,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    rsx! {
        div {
            class: if current_view == View::DmsFriends { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::DmsFriends {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                navigator()
                    .push(Route::DmsHome {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                    });
            },
            div { class: "icon-dms", "💬" }
            SidebarTooltip {
                line1: t("nav-dms"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// Friends / ignore / blocked management button for the account server bar.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarFriendsButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let _app_state: BatchedSignal<AppState> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    rsx! {
        div {
            class: if current_view == View::Friends { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Friends {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                crate::nav!(Route::FriendsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            div { class: "icon-dms", "👥" }
            SidebarTooltip {
                line1: t("nav-friends"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// Notifications button for the account server bar.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarNotifsButton(current_view: View, notif_count: usize) -> Element {
    let _app_state: BatchedSignal<AppState> = use_context();
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let backend_slug = nav
        .read()
                .active_backend
        .cloned()
        .map_or_else(|| "demo".to_string(), |backend| backend.slug().to_string());
    let instance_id = nav
        .read()
                .active_instance_id
        .cloned()
        .unwrap_or_else(|| "demo".to_string());
    let account_id = nav
        .read()
                .active_account_id
        .cloned()
        .unwrap_or_default();

    rsx! {
        div {
            class: if current_view == View::Notifications { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Notifications {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                crate::nav!(Route::NotificationsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            div { class: "icon-notifications", "🔔" }
            if notif_count > 0 {
                span { class: "badge", "{notif_count}" }
            }
            SidebarTooltip {
                line1: t("nav-notifications"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// Discover Communities button — navigates to the Discover route.
///
/// Only rendered when `caps.should_show_discover()` is true (Lemmy, Reddit).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AccountBarDiscoverButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    rsx! {
        div {
            class: if current_view == View::DiscoverCommunities { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::DiscoverCommunities {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                crate::nav!(Route::DiscoverRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            div { class: "icon-discover", "🔍" }
            SidebarTooltip {
                line1: t("nav-discover"),
                line2: None,
                line3: None,
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
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn CreateServerButton(account_id: String) -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let backend_slug = nav
        .read()
                .active_backend
        .cloned().map_or_else(|| "poly".to_string(), |b| b.slug().to_string());
    let instance_id = nav
        .read()
                .active_instance_id
        .cloned()
        .unwrap_or_default();

    rsx! {
        button {
            class: "create-server-pill",
            title: "{t(\"create-server-btn\")}",
            onclick: move |_| {
                crate::nav!(Route::CreateServerRoute {
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
