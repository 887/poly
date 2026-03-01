//! Server sidebar — favorited server icons with source badges.
//!
//! This is the "favorites bar" — a vertical icon strip showing:
//! 1. DMs/Friends button (aggregated across all accounts)
//! 2. Notifications button
//! 3. Separator
//! 4. Favorited server icons from ALL active backends (each showing source badge)
//! 5. Spacer
//! 6. Demo toggle button (above settings)
//! 7. Settings button
//!
//! Each server icon shows the backend type badge (🧪/🟣/🔵/🟢/🟡)
//! so the user always knows which service a server comes from.
// TODO(phase-2.5.3): Demo toggle + TODO(phase-2.5.4): Wire to backends

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;

/// Server sidebar component.
///
/// Shows: DMs icon, Notifications icon, favorited server icons with
/// source badge overlay and account badge overlay, Demo toggle, Settings.
#[component]
pub fn ServerSidebar() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let current_view = app_state.read().nav.view;
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();

    let servers = chat_data.read().servers.clone();
    let demo_active = client_manager.read().demo_active;

    rsx! {
        nav { class: "server-sidebar",

            // DMs / Friends button
            div {
                class: if current_view == View::DmsFriends { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    // Clear server-specific data FIRST (separate writes to trigger
                    // individual signal notifications per field change).
                    chat_data.write().current_server = None;
                    chat_data.write().current_channel = None;
                    chat_data.write().channels.clear();
                    chat_data.write().messages.clear();
                    chat_data.write().members.clear();
                    // Then switch the view
                    app_state.write().nav.view = View::DmsFriends;
                    app_state.write().nav.selected_server = None;
                    app_state.write().nav.selected_channel = None;
                },
                title: "{t(\"nav-dms\")}",
                div { class: "icon-dms", "💬" }
            }

            // Notifications button
            div {
                class: if current_view == View::Notifications { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    app_state.write().nav.view = View::Notifications;
                    // Clear server-specific data
                    let mut cd = chat_data.write();
                    cd.current_server = None;
                    cd.current_channel = None;
                    cd.channels.clear();
                    cd.messages.clear();
                    cd.members.clear();
                },
                title: "{t(\"nav-notifications\")}",
                div { class: "icon-notifications", "🔔" }
                        // TODO(phase-2.5.8): Show aggregated unread count badge
            }

            // Separator
            div { class: "sidebar-separator" }

            // Favorited servers from all active backends
            for server in &servers {
                {
                    let server_id = server.id.clone();
                    let server_name = server.name.clone();
                    let badge = backend_badge(&server.backend);
                    let backend_name = server.backend.display_name();
                    let account_name = server.account_display_name.clone();
                    let unread = server.unread_count;
                    let is_selected = app_state.read().nav.selected_server.as_deref()
                        == Some(&server_id);
                    let first_letter: String = server_name
                        .chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    let tooltip = format!("{server_name}\n{backend_name} — {account_name}");
                    let icon_color = user_color(&server_id);
                    rsx! {
                        div {
                            class: if is_selected { "server-icon active" } else { "server-icon" },
                            onclick: {
                                let server_id_click = server_id.clone();
                                move |_| {
                                    app_state.write().push_nav_history();
                                    app_state.write().nav.view = View::Server;
                                    app_state.write().nav.selected_server = Some(server_id_click.clone());
                                    app_state.write().nav.selected_channel = None;
                                    // Load channels for this server
                                    let sid = server_id_click.clone();
                                    spawn(async move {
                                        load_server_data(sid, app_state, client_manager, chat_data).await; // Unread badge  Unread badge // Unread badge
                                    });
                                }
                            },
                            title: "{tooltip}",
                            div { class: "server-icon-letter", style: "background-color: {icon_color};", "{first_letter}" }
                            // Source badge (backend type)
                            span { class: "source-badge", "{badge}" }
                            // Unread badge
                            if unread > 0 {
                                span { class: "badge", "{unread}" }
                            }
                        }
                    }
                }
            }

            // Spacer to push bottom controls down
            div { class: "sidebar-spacer" }

            // Demo toggle button
            div {
                class: if demo_active { "server-icon demo-active" } else { "server-icon demo-inactive" },
                onclick: move |_| {
                    spawn(async move {
                        toggle_demo(client_manager, chat_data).await;
                    });
                },
                title: if demo_active { t("nav-demo-active") } else { t("nav-demo") },
                div { class: "icon-demo", "🧪" }
                if demo_active {
                    span { class: "demo-dot" }
                }
            }

            // Settings button
            div {
                class: if current_view == View::Settings { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    app_state.write().nav.view = View::Settings;
                },
                title: "{t(\"nav-settings\")}",
                div { class: "icon-settings", "⚙" }
            }
        }
    }
}

/// Toggle the demo client on/off and refresh all data.
async fn toggle_demo(mut client_manager: Signal<ClientManager>, mut chat_data: Signal<ChatData>) {
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
        } else {
            if let Err(e) = client_manager.write().activate_demo().await {
                tracing::error!("Failed to activate demo: {e}");
                return;
            }
            // Load all demo data into chat data
            let servers = client_manager.read().all_servers().await;
            chat_data.write().servers = servers;

            // Load DMs, groups, notifications, friends from demo backend
            let backend = client_manager.read().get_backend("demo");
            if let Some(backend_handle) = backend {
                let guard = backend_handle.read().await;
                if let Ok(dms) = guard.get_dm_channels().await {
                    chat_data.write().dm_channels = dms;
                }
                if let Ok(groups) = guard.get_groups().await {
                    chat_data.write().groups = groups;
                }
                if let Ok(notifs) = guard.get_notifications().await {
                    chat_data.write().notifications = notifs;
                }
                if let Ok(friends) = guard.get_friends().await {
                    chat_data.write().friends = friends;
                }
            }

            // Load voice channel participants from demo data
            chat_data.write().voice_channel_participants =
                poly_demo::data::demo_voice_participants();
        }
    }
}

/// Load channels and select the first text channel for a server.
async fn load_server_data(
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
