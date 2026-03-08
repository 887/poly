//! Favorites bar — account icons + favorited server icons (Bar 1).
//!
//! This is the **leftmost sidebar column** (Bar 1), always visible.
//!
//! Shows:
//! 1. Account icons (top) — one per active backend account, click to switch
//!    - Shows unread badge (total DMs + friend requests + mentions)
//! 2. Separator
//! 3. Favorited server icons from ALL accounts (cross-account)
//! 4. Spacer
//! 5. Demo toggle button
//! 6. App Settings button
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::routes::Route;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData, ContextMenuState, DragSource, SettingsSection, View};
use crate::ui::account::common::chat_history::{initial_message_query, request_scroll_to_bottom};
use dioxus::prelude::*;
use poly_client::{AccountPresence, ConnectionStatus};

/// Spacer that reserves room for the native back/forward nav-bar (desktop/mobile).
/// On web, the browser provides its own back/forward buttons so no space is needed.
#[component]
#[allow(non_snake_case)]
fn NavBarSpacer() -> Element {
    #[cfg(feature = "native-nav")]
    return rsx! {
        div { class: "nav-bar-spacer" }
    };
    #[cfg(not(feature = "native-nav"))]
    rsx! {}
}

/// Favorites Bar component — **Favorites Bar** (Bar 1).
///
/// Shows: Account icons, separator, favorited server icons with
/// source badge, spacer, Demo toggle, App Settings.
#[component]
#[allow(non_snake_case)]
pub fn FavoritesBar() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let current_view = app_state.read().nav.view;
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let servers = chat_data.read().servers.clone();
    let demo_active = client_manager.read().demo_active;
    let active_account = app_state.read().nav.active_account_id.clone();

    // Collect distinct active account IDs for account icons
    let account_ids = client_manager.read().active_account_ids();

    // Only show servers that have been dragged into favorites.
    let favorited_ids = chat_data.read().favorited_server_ids.clone();
    // Preserve the order from favorited_ids list.
    let favorite_servers: Vec<_> = favorited_ids
        .iter()
        .filter_map(|id| servers.iter().find(|s| &s.id == id))
        .cloned()
        .collect();

    // Local signal for drop-zone highlight state.
    let mut drag_over = use_signal(|| false);

    rsx! {
        nav {
            class: if drag_over() { "server-sidebar drag-over" } else { "server-sidebar" },
            // Allow drops from Bar 2 server icons.
            ondragover: move |evt| {
                evt.prevent_default();
                drag_over.set(true);
            },
            ondragleave: move |_| drag_over.set(false),
            ondrop: move |evt| {
                evt.prevent_default();
                drag_over.set(false);
                let new_favorites = {
                    let mut cd = chat_data.write();
                    let drag_id = cd.dragging_server_id.clone();
                    let drag_src = cd.drag_source.clone();
                    // Per-item ondrop handles positional drops via stop_propagation.
                    // This handler catches drops on the nav background (append to end).
                    if let Some(sid) = drag_id {
                        match drag_src {
                            DragSource::AccountServer | DragSource::FavoriteServer => {
                                if !cd.favorited_server_ids.contains(&sid) {
                                    cd.favorited_server_ids.push(sid);
                                }
                            }
                            DragSource::None | DragSource::AccountIcon => {}
                        }
                    }
                    cd.dragging_server_id = None;
                    cd.drag_source = DragSource::None;
                    cd.drag_over_id = None;
                    cd.favorited_server_ids.clone()
                };
                spawn(async move {
                    persist_favorites(new_favorites).await;
                });
            },
            NavBarSpacer {}

            // ── Account icons (one per active account) ────────────────
            for aid in &account_ids {
                AccountIcon {
                    account_id: aid.clone(),
                    is_active: active_account.as_deref() == Some(aid.as_str()),
                }
            }

            // Separator (between accounts and favorites)
            if !account_ids.is_empty() {
                div { class: "sidebar-separator" }
            }

            // ── Favorited servers (dragged in from Bar 2) ─────────────
            for server in &favorite_servers {
                {
                    let instance_id = chat_data
                        .read()
                        .account_sessions
                        .get(&server.account_id)
                        .map(|s| s.instance_id.clone())
                        .unwrap_or_else(|| "demo".to_string());
                    rsx! {
                        FavoriteServerIcon {
                            server_id: server.id.clone(),
                            server_name: server.name.clone(),
                            backend_slug: server.backend.slug().to_string(),
                            instance_id,
                            account_id: server.account_id.clone(),
                            account_display_name: server.account_display_name.clone(),
                            backend_name: server.backend.display_name().to_string(),
                            unread: server.unread_count,
                            icon_url: server.icon_url.clone(),
                        }
                    }
                }
            }

            // Drop hint — shown only when no favorites yet.
            if favorite_servers.is_empty() && demo_active {
                div { class: "favorites-drop-hint",
                    span { "← Drag servers here" }
                }
            }

            // Spacer
            div { class: "sidebar-spacer" }

            // Demo toggle button
            div {
                class: if demo_active { "server-icon demo-active" } else { "server-icon demo-inactive" },
                onclick: move |_| {
                    spawn(async move {
                        let was_active = client_manager.read().demo_active;
                        toggle_demo(client_manager, chat_data, app_state).await;
                        if !was_active {
                            // Demo just turned ON — mark setup complete in memory
                            // (storage is handled inside toggle_demo). Stay on the
                            // current page; no auto-navigation.
                            app_state.write().is_setup_complete = true;
                        } else {
                            // Demo just turned OFF — any route under /demo/demo/*
                            // is now invalid. Navigate to Settings › Accounts so the
                            // user has a valid, useful landing page.
                            app_state.write().settings_section = SettingsSection::Accounts;
                            navigator().replace(Route::SettingsRoute);
                        }
                    });
                },
                title: if demo_active { t("nav-demo-active") } else { t("nav-demo") },
                div { class: "icon-demo", "🧪" }
                if demo_active {
                    span { class: "demo-dot" }
                }
            }

            // App Settings button — only "active" for app-level settings (no account scoped)
            {
                let is_app_settings = current_view == View::Settings && active_account.is_none();
                rsx! {
                    div {
                        class: if is_app_settings { "server-icon active" } else { "server-icon" },
                        onclick: move |_| {
                            navigator().push(Route::SettingsRoute);
                        },
                        title: "{t(\"nav-settings\")}",
                        div { class: "icon-settings", "⚙" }
                    }
                }
            }
        }
    }
}

/// Single account icon in the favorites bar.
///
/// Shows a colored circle with the account's emoji icon (if set in its session)
/// or first character of the account ID as fallback. Clicking navigates to that
/// account's last visited page (or DMs home if no history exists).
#[component]
fn AccountIcon(account_id: String, is_active: bool) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let app_state: Signal<AppState> = use_context();

    // Read connection and presence statuses for this account.
    let conn_class: &'static str = client_manager
        .read()
        .connection_statuses
        .get(&account_id)
        .map(ConnectionStatus::css_class)
        .unwrap_or("disconnected");
    let presence_class: &'static str = client_manager
        .read()
        .presence_statuses
        .get(&account_id)
        .copied()
        .unwrap_or(AccountPresence::Online)
        .css_class();

    let color = user_color(&account_id);

    // Determine avatar URL: real accounts use user.avatar_url; demo accounts
    // get locally bundled cat/dog images; others fall back to icon_emoji text.
    let avatar_url: Option<String> = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .and_then(|s| s.user.avatar_url.clone());

    // Use icon_emoji from session if available, else fall back to first char
    let icon_label: String = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .and_then(|s| s.icon_emoji.clone())
        .unwrap_or_else(|| {
            account_id
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default()
        });

    // Display name shown in the tooltip when hovering the account icon.
    let display_name: String = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .map(|s| s.user.display_name.clone())
        .unwrap_or_else(|| account_id.clone());

    // Count unread DMs + notifications for this account
    let dm_unreads: u32 = chat_data
        .read()
        .dm_channels
        .iter()
        .filter(|dm| dm.account_id == account_id)
        .map(|dm| dm.unread_count)
        .sum();
    let notif_unreads = chat_data
        .read()
        .notifications
        .iter()
        .filter(|n| !n.read && n.account_id == account_id)
        .count() as u32;
    let total_unreads = dm_unreads.saturating_add(notif_unreads);

    // Resolve backend slug and instance_id for routing — read from the session.
    let aid_for_click = account_id.clone();

    rsx! {
        div {
            class: if is_active { "server-icon account-icon active" } else { "server-icon account-icon" },
            onclick: move |_| {
                let aid = aid_for_click.clone();
                // Clear server/channel state — the target route will reload what's needed.
                chat_data.write().current_server = None;
                chat_data.write().current_channel = None;
                chat_data.write().channels.clear();
                chat_data.write().messages.clear();
                chat_data.write().members.clear();

                // If we have a stored last route for this account, restore it.
                // This makes account-switching feel like a true tab switch.
                let last_route_url = app_state
                    .read()
                    .nav
                    .account_last_routes
                    .get(&aid)
                    .cloned();
                if let Some(url) = last_route_url
                    && let Ok(route) = url.parse::<Route>()
                {
                    navigator().push(route);
                    return;
                }

                // No stored route — fall back to the account's DMs home.
                // Look up backend slug + instance_id from the stored session
                let (backend_slug, instance_id) = {
                    chat_data
                        .read()
                        .account_sessions
                        .get(&aid)
                        .map(|s| (s.backend.slug().to_string(), s.instance_id.clone()))
                        .unwrap_or_else(|| (
                            // Fallback: try to find from servers
                            chat_data
                                .read()
                                .servers
                                .iter()
                                .find(|s| s.account_id == aid)
                                .map(|s| s.backend.slug().to_string())
                                .unwrap_or_else(|| "demo".to_string()),
                            "demo".to_string(),
                        ))
                };
                navigator()
                    .push(Route::DmsHome {
                        backend: backend_slug,
                        instance_id,
                        account_id: aid,
                    });
            },
            title: "{display_name}",
            // Render image avatar if available (avatar_url is set by the client;
            // demo client sets it to the bundled cat/dog asset path).
            if let Some(url) = &avatar_url {
                img {
                    src: "{url}",
                    class: "server-icon-image",
                    alt: "{account_id}",
                }
            } else {
                div {
                    class: "server-icon-letter",
                    style: "background-color: {color};",
                    "{icon_label}"
                }
            }
            if total_unreads > 0 {
                span { class: "badge", "{total_unreads}" }
            }
            // Connection status dot (top-right) — shows connected/connecting/error
            span {
                class: "status-dot connection-dot {conn_class}",
                title: "Connection: {conn_class}",
            }
            // Presence/availability dot (bottom-right) — shows online/away/dnd/etc.
            span {
                class: "status-dot presence-dot {presence_class}",
                title: "Presence: {presence_class}",
            }
        }
    }
}

/// Single favorited server icon in the favorites bar.
///
/// Supports:
/// - Click to navigate to the server
/// - Right-click to open the server context menu
/// - Drag to reorder within Bar 1 or move back (drag is tracked via `DragSource::FavoriteServer`)
/// - Accept drops from Bar 2 (`DragSource::AccountServer`) for positional insertion
#[component]
fn FavoriteServerIcon(
    server_id: String,
    server_name: String,
    backend_slug: String,
    /// Federated homeserver instance ID (mirrors `:instance_id` URL segment).
    instance_id: String,
    account_id: String,
    account_display_name: String,
    backend_name: String,
    unread: u32,
    /// Optional server icon URL. When `Some`, rendered as an `<img>`; when
    /// `None`, falls back to a colored first-letter placeholder.
    icon_url: Option<String>,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let is_selected = app_state.read().nav.selected_server.as_deref() == Some(&server_id);
    let is_drag_over = chat_data.read().drag_over_id.as_deref() == Some(server_id.as_str());
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let tooltip = format!("{server_name}\n{backend_name} — {account_display_name}");
    let icon_color = user_color(&server_id);

    // Determine source badge: account's avatar URL
    let account_avatar_url: Option<String> = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .and_then(|s| s.user.avatar_url.clone());

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
            // Click → navigate to server
            onclick: {
                let sid = server_id.clone();
                let bslug = backend_slug.clone();
                let iid = instance_id.clone();
                let aid = account_id.clone();
                move |_| {
                    app_state.write().nav.selected_server = Some(sid.clone());
                    app_state.write().nav.selected_channel = None;
                    let sid2 = sid.clone();
                    spawn(async move {
                        load_server_data(sid2, app_state, client_manager, chat_data).await;
                    });
                    navigator()
                        .push(Route::ServerHome {
                            backend: bslug.clone(),
                            instance_id: iid.clone(),
                            account_id: aid.clone(),
                            server_id: sid.clone(),
                        });
                }
            },
            // Right-click → open context menu at cursor position
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
            // Drag start — mark as dragging from Bar 1
            ondragstart: {
                let sid = server_id.clone();
                move |_| {
                    let mut cd = chat_data.write();
                    cd.dragging_server_id = Some(sid.clone());
                    cd.drag_source = DragSource::FavoriteServer;
                }
            },
            // Drag over this item — highlight as drop target
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
            // Drop on this item — reorder within Bar 1, or insert from Bar 2
            ondrop: {
                let tid = server_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    // Stop bubbling so the nav's ondrop doesn't double-handle
                    evt.stop_propagation();
                    let new_favorites = {
                        let mut cd = chat_data.write();
                        let dragging = cd.dragging_server_id.clone();
                        let src = cd.drag_source.clone();
                        cd.drag_over_id = None;
                        let Some(drag_id) = dragging else {
                            cd.dragging_server_id = None;
                            cd.drag_source = DragSource::None;
                            return;
                        };
                        let target_id = tid.clone();
                        if drag_id == target_id {
                            cd.dragging_server_id = None;
                            cd.drag_source = DragSource::None;
                            return;
                        }
                        match src {
                            DragSource::FavoriteServer => {
                                // Reorder within Bar 1: move drag_id before target_id
                                if let Some(from) = cd
                                    .favorited_server_ids
                                    .iter()
                                    .position(|x| *x == drag_id)
                                {
                                    cd.favorited_server_ids.remove(from);
                                    if let Some(to) = cd
                                        .favorited_server_ids
                                        .iter()
                                        .position(|x| *x == target_id)
                                    {
                                        cd.favorited_server_ids.insert(to, drag_id);
                                    } else {
                                        cd.favorited_server_ids.push(drag_id);
                                    }
                                }
                            }
                            DragSource::AccountServer => {
                                // Insert from Bar 2 before target position
                                if !cd.favorited_server_ids.contains(&drag_id) {
                                    if let Some(to) = cd
                                        .favorited_server_ids
                                        .iter()
                                        .position(|x| *x == target_id)
                                    {
                                        cd.favorited_server_ids.insert(to, drag_id);
                                    } else {
                                        cd.favorited_server_ids.push(drag_id);
                                    }
                                }
                            }
                            DragSource::None | DragSource::AccountIcon => {}
                        }
                        cd.dragging_server_id = None;
                        cd.drag_source = DragSource::None;
                        cd.favorited_server_ids.clone()
                    };
                    spawn(async move {
                        persist_favorites(new_favorites).await;
                    });
                }
            },
            // Drag end — always clean up regardless of drop target
            ondragend: move |_| {
                let mut cd = chat_data.write();
                cd.dragging_server_id = None;
                cd.drag_source = DragSource::None;
                cd.drag_over_id = None;
            },
            title: "{tooltip}",
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
            // Source badge: show account avatar image (or fallback letter)
            if let Some(url) = &account_avatar_url {
                img {
                    src: "{url}",
                    class: "source-badge-image",
                    alt: "{account_display_name}",
                }
            } else {
                span { class: "source-badge", "A" }
            }
            if unread > 0 {
                span { class: "badge", "{unread}" }
            }
        }
    }
}

/// Toggle the demo client on/off and refresh all data.
///
/// Called from the 🧪 button click handler and by [`super::init_storage`] on
/// startup to restore a previously active demo session. Does NOT navigate —
/// the caller is responsible for routing after this returns.
pub(crate) async fn toggle_demo(
    mut client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
) {
    #[cfg(feature = "demo")]
    {
        let is_active = client_manager.read().demo_active;
        if is_active {
            client_manager.write().deactivate_demo();
            // Remove all demo data from chat data
            chat_data
                .write()
                .servers
                .retain(|s| s.backend != poly_client::BackendType::Demo);
            chat_data
                .write()
                .dm_channels
                .retain(|d| d.backend != poly_client::BackendType::Demo);
            chat_data
                .write()
                .groups
                .retain(|g| g.backend != poly_client::BackendType::Demo);
            chat_data
                .write()
                .notifications
                .retain(|n| n.backend != poly_client::BackendType::Demo);
            chat_data
                .write()
                .friends
                .retain(|u| u.backend != poly_client::BackendType::Demo);
            // Clear channels/messages if current server was demo
            chat_data.write().channels.clear();
            chat_data.write().messages.clear();
            chat_data.write().members.clear();
            chat_data.write().current_server = None;
            chat_data.write().current_channel = None;
            chat_data.write().voice_channel_participants.clear();
            chat_data.write().voice_connection = None;
            chat_data
                .write()
                .account_sessions
                .retain(|k, _| k != "demo-cat" && k != "demo-dog");
            // Remove demo server IDs from favorites (a real account's favorites are unaffected).
            {
                let demo_server_ids: Vec<String> = chat_data
                    .read()
                    .servers
                    .iter()
                    .filter(|s| s.backend == poly_client::BackendType::Demo)
                    .map(|s| s.id.clone())
                    .collect();
                chat_data
                    .write()
                    .favorited_server_ids
                    .retain(|id| !demo_server_ids.contains(id));
            }
            chat_data.write().dragging_server_id = None;
            // Persist demo_active=false and updated favorites to storage.
            if let Some(s) = crate::STORAGE.get() {
                let mut settings = s.get_app_settings().await.unwrap_or_default();
                settings.demo_active = false;
                settings.favorited_server_ids = chat_data.read().favorited_server_ids.clone();
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=false: {e}");
                }
            }
        } else {
            let session = match client_manager.write().activate_demo().await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to activate demo: {e}");
                    return;
                }
            };
            // Store the cat session keyed by "demo-cat"
            chat_data
                .write()
                .account_sessions
                .insert("demo-cat".to_string(), session);
            // Store all sessions from the client manager (includes demo-dog 🐶)
            for (account_id, sess) in &client_manager.read().sessions {
                chat_data
                    .write()
                    .account_sessions
                    .insert(account_id.clone(), sess.clone());
            }
            // Load all servers from both demo accounts
            let servers = client_manager.read().all_servers().await;
            // Pre-populate favorites with all demo servers so Bar 1 shows them immediately.
            // Users can remove entries by rearranging; dragging from Bar 2 adds to this list.
            let server_ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
            for sid in server_ids {
                if !chat_data.read().favorited_server_ids.contains(&sid) {
                    chat_data.write().favorited_server_ids.push(sid);
                }
            }
            chat_data.write().servers = servers;

            // Load DMs, groups, notifications, friends from ALL demo backends
            for demo_account_id in &["demo-cat", "demo-dog"] {
                let backend = client_manager.read().get_backend(demo_account_id);
                if let Some(backend_handle) = backend {
                    let guard = backend_handle.read().await;
                    if let Ok(dms) = guard.get_dm_channels().await {
                        chat_data.write().dm_channels.extend(dms);
                    }
                    if let Ok(groups) = guard.get_groups().await {
                        chat_data.write().groups.extend(groups);
                    }
                    if let Ok(notifs) = guard.get_notifications().await {
                        chat_data.write().notifications.extend(notifs);
                    }
                    if let Ok(friends) = guard.get_friends().await {
                        // Deduplicate friends by ID
                        for friend in friends {
                            if !chat_data.read().friends.iter().any(|f| f.id == friend.id) {
                                chat_data.write().friends.push(friend);
                            }
                        }
                    }
                    // Load voice participants for all voice channels
                    let servers_snapshot = chat_data.read().servers.clone();
                    for server in &servers_snapshot {
                        if server.account_id != *demo_account_id {
                            continue;
                        }
                        if let Ok(channels) = guard.get_channels(&server.id).await {
                            for ch in channels {
                                if matches!(
                                    ch.channel_type,
                                    poly_client::ChannelType::Voice
                                        | poly_client::ChannelType::Video
                                ) && let Ok(participants) =
                                    guard.get_voice_participants(&ch.id).await
                                    && !participants.is_empty()
                                {
                                    chat_data
                                        .write()
                                        .voice_channel_participants
                                        .insert(ch.id.clone(), participants);
                                }
                            }
                        }
                    }
                }
            }
            // Persist demo_active=true and setup_complete=true to storage so
            // the demo client is restored on next app launch without re-toggling.
            if let Some(s) = crate::STORAGE.get() {
                let mut settings = s.get_app_settings().await.unwrap_or_default();
                settings.demo_active = true;
                settings.setup_complete = true;
                settings.favorited_server_ids = chat_data.read().favorited_server_ids.clone();
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=true: {e}");
                }
            }

            // Start real-time event stream listeners for each demo account.
            // These tasks run in the background and update chat_data when
            // new messages or presence changes arrive from the demo backend.
            for demo_account_id in &["demo-cat", "demo-dog"] {
                let aid = (*demo_account_id).to_string();
                if let Some(backend) = client_manager.read().get_backend(&aid) {
                    spawn_event_stream_listener(aid, backend, app_state, chat_data, client_manager);
                }
            }
        }
    }
}

/// Load channels and select the first text channel for a server.
pub async fn load_server_data(
    server_id: String,
    mut app_state: Signal<AppState>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
) {
    chat_data.write().loading = true;

    // Find which backend owns this server
    let backend_info = client_manager.read().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_data.write().loading = false;
        return;
    };

    // Load server details
    {
        let guard = backend.read().await;
        if let Ok(server) = guard.get_server(&server_id).await {
            chat_data.write().current_server = Some(server);
        }
    }

    // Load channels
    let channels = {
        let guard = backend.read().await;
        guard.get_channels(&server_id).await.unwrap_or_default()
    };

    // Find first text channel
    let first_text_channel = channels
        .iter()
        .find(|c| c.channel_type == poly_client::ChannelType::Text)
        .cloned();

    chat_data.write().channels = channels;

    // Auto-select first text channel
    if let Some(ch) = first_text_channel {
        app_state.write().nav.selected_channel = Some(ch.id.clone());
        chat_data.write().current_channel = Some(ch.clone());

        // Load messages for first channel
        let guard = backend.read().await;
        if let Ok(messages) = guard
            .get_messages(&ch.id, initial_message_query(ch.unread_count))
            .await
        {
            chat_data.write().messages = messages;
            request_scroll_to_bottom();
        }
        // Load members
        if let Ok(members) = guard.get_channel_members(&ch.id).await {
            chat_data.write().members = members;
        }
    }

    chat_data.write().loading = false;
    // Apply any user-defined icon/banner overrides from storage.
    apply_server_icon_overrides(&mut chat_data).await;
}

/// Apply user icon and banner overrides from `AppSettings` to all servers in
/// `chat_data`.
///
/// Called after every `load_server_data` and `restore_server_channel` so that
/// overrides entered in the server settings Overview panel survive across page
/// navigations and app restarts.
///
/// No-ops silently if storage is not yet initialised.
async fn apply_server_icon_overrides(chat_data: &mut Signal<crate::state::ChatData>) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.server_icon_overrides.is_empty() && settings.server_banner_overrides.is_empty() {
        return;
    }
    let mut cd = chat_data.write();
    for server in &mut cd.servers {
        if let Some(url) = settings.server_icon_overrides.get(&server.id) {
            server.icon_url = Some(url.clone());
        }
        if let Some(url) = settings.server_banner_overrides.get(&server.id) {
            server.banner_url = Some(url.clone());
        }
    }
    if let Some(ref mut current) = cd.current_server {
        if let Some(url) = settings.server_icon_overrides.get(&current.id) {
            current.icon_url = Some(url.clone());
        }
        if let Some(url) = settings.server_banner_overrides.get(&current.id) {
            current.banner_url = Some(url.clone());
        }
    }
}
///
/// Called after every mutation of `ChatData.favorited_server_ids` to survive
/// page reloads, app restarts, and offline periods.
/// No-ops silently if storage is not yet initialised.
pub(crate) async fn persist_favorites(ids: Vec<String>) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    match s.get_app_settings().await {
        Ok(mut settings) => {
            settings.favorited_server_ids = ids;
            if let Err(e) = s.set_app_settings(&settings).await {
                tracing::warn!("Failed to persist favorites: {e}");
            }
        }
        Err(e) => tracing::warn!("Failed to read app_settings for favorites persist: {e}"),
    }
}

/// Restore a specific server channel from a URL (F5 / deep-link navigation).
///
/// Unlike [`load_server_data`] which auto-selects the first text channel,
/// this function restores the exact `channel_id` encoded in the URL.
///
/// Called from the `ServerChat` route component's `use_effect` when
/// `chat_data` is empty (i.e. the page was hard-refreshed).
pub async fn restore_server_channel(
    server_id: String,
    channel_id: String,
    mut app_state: Signal<AppState>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
) {
    chat_data.write().loading = true;

    let backend_info = client_manager.read().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_data.write().loading = false;
        return;
    };

    // Load server details
    {
        let guard = backend.read().await;
        if let Ok(server) = guard.get_server(&server_id).await {
            chat_data.write().current_server = Some(server);
        }
    }

    // Load all channels for the sidebar
    let channels = {
        let guard = backend.read().await;
        guard.get_channels(&server_id).await.unwrap_or_default()
    };

    // Locate the requested channel; fall back to first text channel if missing.
    let target = channels
        .iter()
        .find(|c| c.id == channel_id)
        .or_else(|| {
            channels
                .iter()
                .find(|c| c.channel_type == poly_client::ChannelType::Text)
        })
        .cloned();

    chat_data.write().channels = channels;

    if let Some(ch) = target {
        app_state.write().nav.selected_channel = Some(ch.id.clone());
        chat_data.write().current_channel = Some(ch.clone());

        if ch.channel_type == poly_client::ChannelType::Text {
            let guard = backend.read().await;
            if let Ok(messages) = guard
                .get_messages(&ch.id, initial_message_query(ch.unread_count))
                .await
            {
                chat_data.write().messages = messages;
                request_scroll_to_bottom();
            }
            if let Ok(members) = guard.get_channel_members(&ch.id).await {
                chat_data.write().members = members;
            }
        } else if matches!(
            ch.channel_type,
            poly_client::ChannelType::Voice | poly_client::ChannelType::Video
        ) {
            let guard = backend.read().await;
            if let Ok(participants) = guard.get_voice_participants(&ch.id).await {
                chat_data
                    .write()
                    .voice_channel_participants
                    .insert(ch.id.clone(), participants);
            }
        }
    }

    chat_data.write().loading = false;
    // Apply any user-defined icon/banner overrides from storage.
    apply_server_icon_overrides(&mut chat_data).await;
}

/// Start a background event-stream listener for a single backend account.
///
/// Spawns a Dioxus task that polls the backend's [`ClientBackend::event_stream`]
/// and processes each incoming [`poly_client::ClientEvent`]:
///
/// - [`ClientEvent::MessageReceived`] — appends the message to `chat_data.messages`
///   when the current channel is selected; otherwise updates unread count.
/// - [`ClientEvent::PresenceChanged`] — updates presence on matching members.
/// - Other events are silently ignored for now.
///
/// The task exits automatically when `client_manager.demo_active` becomes false
/// (checked after each event) so there is no orphan task after demo is toggled off.
pub(crate) fn spawn_event_stream_listener(
    account_id: String,
    backend: crate::client_manager::BackendHandle,
    app_state: Signal<AppState>,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
) {
    use futures::StreamExt as _;
    use poly_client::ClientEvent;

    spawn(async move {
        // Acquire the stream without holding the lock for the duration of polling.
        let stream = {
            let guard = backend.read().await;
            guard.event_stream()
        };
        let mut stream = stream;

        tracing::debug!("Event stream started for account: {account_id}");

        while let Some(event) = stream.next().await {
            // Stop the listener when demo is deactivated (or account removed).
            let still_active = {
                let cm = client_manager.read();
                cm.demo_active && cm.get_backend(&account_id).is_some()
            };
            if !still_active {
                break;
            }

            match event {
                ClientEvent::MessageReceived {
                    ref channel_id,
                    ref message,
                } => {
                    let selected = app_state.read().nav.selected_channel.clone();
                    if selected.as_deref() == Some(channel_id.as_str()) {
                        // Currently viewing this channel — append message live.
                        chat_data.write().messages.push(message.clone());
                        tracing::trace!(
                            "Live message in #{channel_id}: {}",
                            message.author.display_name
                        );
                    }
                    // TODO(phase-3): increment unread count for other channels
                }
                ClientEvent::PresenceChanged {
                    ref user_id,
                    status,
                } => {
                    let mut cd = chat_data.write();
                    for member in &mut cd.members {
                        if member.id == *user_id {
                            member.presence = status;
                            break;
                        }
                    }
                }
                ClientEvent::TypingStarted { .. } => {
                    // TODO(phase-3): show typing indicator in chat view
                }
                _ => {}
            }
        }

        tracing::debug!("Event stream ended for account: {account_id}");
    });
}
