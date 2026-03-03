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
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;

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
                let drag_id = chat_data.read().dragging_server_id.clone();
                if let Some(sid) = drag_id {
                    let mut cd = chat_data.write();
                    if !cd.favorited_server_ids.contains(&sid) {
                        cd.favorited_server_ids.push(sid);
                    }
                    cd.dragging_server_id = None;
                }
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
                    // Use account icon_emoji as source badge when available,
                    // falling back to the generic backend emoji.
                    let account_icon = chat_data
                        .read()
                        .account_sessions
                        .get(&server.account_id)
                        .and_then(|s| s.icon_emoji.clone())
                        .unwrap_or_else(|| backend_badge(&server.backend).to_string());
                    rsx! {
                        FavoriteServerIcon {
                            server_id: server.id.clone(),
                            server_name: server.name.clone(),
                            badge: account_icon,
                            backend_slug: server.backend.slug().to_string(),
                            account_id: server.account_id.clone(),
                            account_display_name: server.account_display_name.clone(),
                            backend_name: server.backend.display_name().to_string(),
                            unread: server.unread_count,
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
                        toggle_demo(client_manager, chat_data).await;
                        // After activating demo, mark setup complete in memory
                        // (storage persistence is handled inside toggle_demo) and
                        // navigate to the demo DMs so the user isn't left on
                        // an empty settings page.
                        if !was_active && client_manager.read().demo_active {
                            app_state.write().is_setup_complete = true;
                            navigator()
                                .push(Route::DmsHome {
                                    backend: "demo".to_string(),
                                    account_id: "demo".to_string(),
                                });
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
/// account's DMs home.
#[component]
fn AccountIcon(account_id: String, is_active: bool) -> Element {
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let color = user_color(&account_id);

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

    // Resolve backend slug for routing
    let aid_for_click = account_id.clone();

    rsx! {
        div {
            class: if is_active { "server-icon account-icon active" } else { "server-icon account-icon" },
            onclick: move |_| {
                let aid = aid_for_click.clone();
                let cm = client_manager.read();
                // Look up backend type for this account
                let backend_slug = {
                    // Check all servers to find backend type, or just use
                    // a known mapping for demo
                    if aid == "demo" {
                        "demo".to_string()
                    } else {
                        // Try to find from servers
                        chat_data
                            .read()
                            .servers
                            .iter()
                            .find(|s| s.account_id == aid)
                            .map(|s| s.backend.slug().to_string())
                            .unwrap_or_else(|| "demo".to_string())
                    }
                };
                drop(cm);
                chat_data.write().current_server = None;
                chat_data.write().current_channel = None;
                chat_data.write().channels.clear();
                chat_data.write().messages.clear();
                chat_data.write().members.clear();
                navigator()
                    .push(Route::DmsHome {
                        backend: backend_slug,
                        account_id: aid,
                    });
            },
            title: "{account_id}",
            div {
                class: "server-icon-letter",
                style: "background-color: {color};",
                "{icon_label}"
            }
            if total_unreads > 0 {
                span { class: "badge", "{total_unreads}" }
            }
        }
    }
}

/// Single favorited server icon in the favorites bar.
#[component]
fn FavoriteServerIcon(
    server_id: String,
    server_name: String,
    badge: String,
    backend_slug: String,
    account_id: String,
    account_display_name: String,
    backend_name: String,
    unread: u32,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let is_selected = app_state.read().nav.selected_server.as_deref() == Some(&server_id);
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let tooltip = format!("{server_name}\n{backend_name} — {account_display_name}");
    let icon_color = user_color(&server_id);

    rsx! {
        div {
            class: if is_selected { "server-icon active" } else { "server-icon" },
            onclick: {
                let sid = server_id.clone();
                let bslug = backend_slug.clone();
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
                            account_id: aid.clone(),
                            server_id: sid.clone(),
                        });
                }
            },
            title: "{tooltip}",
            div {
                class: "server-icon-letter",
                style: "background-color: {icon_color};",
                "{first_letter}"
            }
            span { class: "source-badge", "{badge}" }
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
            chat_data.write().local_session = None;
            chat_data
                .write()
                .account_sessions
                .retain(|k, _| k != "demo" && k != "demo2");
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
            // Persist demo_active=false so the next launch skips demo restore.
            if let Some(s) = crate::STORAGE.get() {
                let mut settings = s.get_app_settings().await.unwrap_or_default();
                settings.demo_active = false;
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
            chat_data.write().local_session = Some(session.clone());
            // Store the cat session keyed by "demo"
            chat_data
                .write()
                .account_sessions
                .insert("demo".to_string(), session);
            // Store all sessions from the client manager (includes demo2 🐶)
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
            for demo_account_id in &["demo", "demo2"] {
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
                if let Err(e) = s.set_app_settings(&settings).await {
                    tracing::warn!("Failed to persist demo_active=true: {e}");
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
            .get_messages(&ch.id, poly_client::MessageQuery::default())
            .await
        {
            chat_data.write().messages = messages;
        }
        // Load members
        if let Ok(members) = guard.get_channel_members(&ch.id).await {
            chat_data.write().members = members;
        }
    }

    chat_data.write().loading = false;
}
