//! Channel list — categories and channels for the selected server.
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific channel list decorations live in per-backend
//! directories (`demo/`, `stoat/`, etc.).
//!
//! Delegates to sub-components to stay under 150-line component size limit:
//! - `ServerBanner`: displays server name or "Direct Messages" title
//! - `DMFriendsView`: DM + group + friends unified list with search
//! - `ServerChannelView`: server categories and channels

use super::super::super::routes::Route;
use super::chat_history::{
    initial_message_query, read_channel_view_anchor, remember_message_list_scroll_position,
    request_restore_scroll_position_or_bottom, request_restore_to_anchor,
};
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChannelContextMenuState, ChatData, View};
use crate::ui::main_layout::close_mobile_drawer;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use poly_client::{Channel, ChannelType, DmChannel, Server, User, VoiceParticipant};

fn dm_last_incoming_timestamp(dm: &DmChannel) -> Option<DateTime<Utc>> {
    dm.last_message
        .as_ref()
        .filter(|message| message.author.id == dm.user.id)
        .map(|message| message.timestamp)
}

fn group_last_incoming_timestamp(
    group: &poly_client::Group,
    active_user_id: Option<&str>,
) -> Option<DateTime<Utc>> {
    group
        .last_message
        .as_ref()
        .filter(|message| active_user_id.is_none_or(|user_id| message.author.id != user_id))
        .map(|message| message.timestamp)
}

/// Main channel list component — delegates to sub-views based on current view.
/// Load messages, members, and voice participants for a channel.
async fn load_channel_data(
    channel_id: String,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
) {
    chat_data.write().loading = true;

    let unread_count = chat_data
        .read()
        .current_channel
        .as_ref()
        .filter(|channel| channel.id == channel_id)
        .map_or(0, |channel| channel.unread_count);

    // Get selected server to find the right backend
    let server_id = app_state.read().nav.selected_server.clone();
    let Some(server_id) = server_id else {
        chat_data.write().loading = false;
        return;
    };

    let backend_info = client_manager.read().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_data.write().loading = false;
        return;
    };

    let channel_type = chat_data
        .read()
        .current_channel
        .as_ref()
        .map(|ch| ch.channel_type);

    let guard = backend.read().await;

    match channel_type {
        Some(poly_client::ChannelType::Voice) | Some(poly_client::ChannelType::Video) => {
            // Voice/video channel — load participant list from backend
            if let Ok(participants) = guard.get_voice_participants(&channel_id).await {
                chat_data
                    .write()
                    .voice_channel_participants
                    .insert(channel_id.clone(), participants);
            }
        }
        _ => {
            // Text channel — load messages and members.
            // If a scrollend-saved anchor exists for this channel, load around that
            // message so the user returns to approximately where they were reading.
            let anchor = read_channel_view_anchor(&channel_id).await;
            let query = if let Some((_, ref msg_id, _)) = anchor {
                poly_client::MessageQuery {
                    around: Some(msg_id.clone()),
                    limit: Some(initial_message_query(unread_count).limit.unwrap_or(36)),
                    ..Default::default()
                }
            } else {
                initial_message_query(unread_count)
            };
            if let Ok(messages) = guard.get_messages(&channel_id, query).await {
                let mut data = chat_data.write();
                data.messages = messages;
                data.messages_loaded_via_anchor = anchor.is_some();
                drop(data);
                if let Some((ref element_id, _, offset_px)) = anchor {
                    request_restore_to_anchor(&channel_id, element_id, offset_px);
                } else {
                    request_restore_scroll_position_or_bottom(&channel_id);
                }
            }
            if let Ok(members) = guard.get_channel_members(&channel_id).await {
                chat_data.write().members = members;
            }
        }
    }

    chat_data.write().loading = false;
}
/// Load messages for a DM or group channel using the account backend directly
/// (does not require a selected server).
async fn load_dm_messages(
    channel_id: String,
    account_id: String,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
) {
    chat_data.write().loading = true;
    chat_data.write().messages = Vec::new();
    chat_data.write().members = Vec::new();

    let unread_count = chat_data
        .read()
        .current_channel
        .as_ref()
        .filter(|channel| channel.id == channel_id)
        .map_or(0, |channel| channel.unread_count);

    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        chat_data.write().loading = false;
        return;
    };

    let guard = backend.read().await;
    if let Ok(messages) = guard
        .get_messages(&channel_id, initial_message_query(unread_count))
        .await
    {
        chat_data.write().messages = messages;
        request_restore_scroll_position_or_bottom(&channel_id);
    }
    if let Ok(members) = guard.get_channel_members(&channel_id).await {
        chat_data.write().members = members;
    }

    chat_data.write().loading = false;
}

fn activate_dm_channel(
    dm: DmChannel,
    instance_id: String,
    mut app_state: Signal<AppState>,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
    nav: crate::ui::dioxus_router::Navigator,
) {
    if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone() {
        remember_message_list_scroll_position(&previous_channel_id);
    }

    app_state.write().nav.selected_server = None;
    app_state.write().nav.selected_channel = Some(dm.id.clone());
    app_state.write().nav.dm_right_sidebar_visible = false;
    chat_data.write().active_group_members = Vec::new();
    chat_data.write().current_channel = Some(Channel {
        id: dm.id.clone(),
        name: dm.user.display_name.clone(),
        channel_type: ChannelType::Text,
        server_id: String::new(),
        unread_count: dm.unread_count,
        mention_count: 0,
        last_message_id: dm.last_message.as_ref().map(|message| message.id.clone()),
    });
    chat_data.write().current_server = None;

    let channel_id = dm.id.clone();
    let account_id = dm.account_id.clone();
    spawn(async move {
        load_dm_messages(channel_id, account_id, client_manager, chat_data).await;
    });

    nav.push(Route::DmChat {
        backend: dm.backend.slug().to_string(),
        instance_id,
        account_id: dm.account_id.clone(),
        dm_id: dm.id.clone(),
    });
    close_mobile_drawer();
}

fn active_account_context(
    app_state: Signal<AppState>,
    chat_data: Signal<ChatData>,
) -> Option<(String, String)> {
    let account_id = app_state.read().nav.active_account_id.clone()?;
    let instance_id = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .map(|session| session.instance_id.clone())
        .or_else(|| app_state.read().nav.active_instance_id.clone())
        .unwrap_or_default();
    Some((account_id, instance_id))
}

/// Open or create a direct message for the current active account, then
/// navigate using the real DM channel ID returned by the backend.
pub(crate) fn open_direct_message_from_active_account(
    user_id: String,
    app_state: Signal<AppState>,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
    nav: crate::ui::dioxus_router::Navigator,
) {
    let Some((account_id, instance_id)) = active_account_context(app_state, chat_data) else {
        tracing::warn!("open_direct_message_from_active_account: no active account");
        return;
    };

    let existing_dm = {
        let chat_data_read = chat_data.read();
        chat_data_read
            .dm_channels
            .iter()
            .find(|dm| dm.account_id == account_id && dm.user.id == user_id)
            .cloned()
    };

    if let Some(existing_dm) = existing_dm {
        activate_dm_channel(
            existing_dm,
            instance_id,
            app_state,
            chat_data,
            client_manager,
            nav,
        );
        return;
    }

    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        tracing::warn!(
            "open_direct_message_from_active_account: no backend found for account {}",
            account_id
        );
        return;
    };

    spawn(async move {
        let opened_dm = {
            let guard = backend.read().await;
            match guard.open_direct_message_channel(&user_id).await {
                Ok(dm) => dm,
                Err(err) => {
                    tracing::warn!(
                        "open_direct_message_from_active_account: failed to open DM for user {} on account {}: {}",
                        user_id,
                        account_id,
                        err
                    );
                    return;
                }
            }
        };

        let mut chat_data_write = chat_data.write();
        chat_data_write.dm_channels.retain(|dm| {
            !(dm.account_id == account_id && (dm.id == opened_dm.id || dm.user.id == user_id))
        });
        chat_data_write.dm_channels.push(opened_dm.clone());
        drop(chat_data_write);

        activate_dm_channel(
            opened_dm,
            instance_id,
            app_state,
            chat_data,
            client_manager,
            nav,
        );
    });
}

/// Single connected voice participant entry.
#[rustfmt::skip]
#[component]
pub fn ChannelList() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let current_view = app_state.read().nav.view;
    let current_server = chat_data.read().current_server.clone();
    let visible_category_ids = use_signal(Vec::<String>::new);

    rsx! {
        aside { class: "channel-list",
            ServerBanner {
                current_view,
                current_server: current_server.clone(),
                visible_category_ids,
            }

            div { class: "channel-entries",
                if current_view == View::DmsFriends {
                    DMFriendsView {}
                } else if current_server.is_some() {
                    ServerChannelView { visible_category_ids }
                } else {
                    div { class: "channel-empty",
                        p { "{t(\"chat-no-messages\")}" }
                    }
                }
            }
        }
    }
}

/// Discord-style server banner — top of the channel list sidebar.
///
/// Shows:
/// - **DMs view:** simple "Direct Messages" heading.
/// - **Server view:**
///   - Optional full-width banner image (when `server.banner_url` is `Some`).
///   - Header bar with a clickable server-name button (opens dropdown) and an
///     inline invite-people button on the right.
///   - Dropdown menu: Server Settings, ──, Invite People, Notification
///     Settings, ──, Leave Server.
///
/// The dropdown is closed by clicking the transparent `.context-menu-backdrop`
/// overlay that covers the full viewport beneath the panel.
// DECISION(DX): reuses the context-menu-backdrop/context-menu CSS pattern
// established in phase-2.10 so we don't need new z-index layers.
#[rustfmt::skip]
#[component]
fn ServerBanner(
    current_view: View,
    current_server: Option<Server>,
    visible_category_ids: Signal<Vec<String>>,
) -> Element {
    let app_state: Signal<AppState> = use_context();
    let mut dropdown_open = use_signal(|| false);
    let mut channels_roles_open = use_signal(|| false);

    // Derive route-construction fields from AppState before entering RSX so
    // that we don't hold a borrow of `app_state` inside closures that also
    // mutate `dropdown_open`.
    let instance_id = app_state
        .read()
        .nav
        .active_instance_id
        .clone()
        .unwrap_or_default();
    let account_id = app_state
        .read()
        .nav
        .active_account_id
        .clone()
        .unwrap_or_default();
    let server_id = app_state
        .read()
        .nav
        .selected_server
        .clone()
        .unwrap_or_default();

    // Backend slug comes from the Server struct itself (always consistent with
    // what was used to navigate here).
    let backend_slug = current_server
        .as_ref()
        .map(|s| s.backend.slug().to_string())
        .unwrap_or_default();
    let supports_channels_roles = current_server
        .as_ref()
        .is_some_and(|server| server.backend.to_string() == "demo");

    rsx! {
        div { class: "server-banner-sidebar",
            // ── Transparent click-catcher to close the dropdown ──────────────
            if *dropdown_open.read() {
                div {
                    class: "context-menu-backdrop",
                    onclick: move |_| dropdown_open.set(false),
                }
            }

            if current_view == View::DmsFriends {
                // ── DMs / Friends view: plain heading ────────────────────────
                div { class: "server-banner-header",
                    h3 { "{t(\"nav-dms\")}" }
                }
            } else if let Some(ref server) = current_server {
                // ── Server view ──────────────────────────────────────────────
                if let Some(ref url) = server.banner_url {
                    div { class: "server-banner-hero",
                        img {
                            class: "server-banner-img",
                            src: "{url}",
                            alt: "",
                            draggable: false,
                        }
                        div { class: "server-banner-overlay",
                            div { class: "server-banner-header server-banner-header-overlay",
                                button {
                                    class: "server-name-trigger",
                                    onclick: move |_| {
                                        let open = *dropdown_open.read();
                                        dropdown_open.set(!open);
                                    },
                                    span { class: "server-name-text", "{server.name}" }
                                    if *dropdown_open.read() {
                                        span { class: "server-name-chevron", "▴" }
                                    } else {
                                        span { class: "server-name-chevron", "▾" }
                                    }
                                }
                            }
                            if supports_channels_roles {
                                button {
                                    class: "server-channels-roles-btn",
                                    onclick: move |_| {
                                        let open = *channels_roles_open.read();
                                        channels_roles_open.set(!open);
                                    },
                                    span { class: "server-channels-roles-icon", "☰" }
                                    span { "{t(\"server-banner-channels-roles\")}" }
                                }
                            }
                        }
                    }
                } else {
                    div { class: "server-banner-header",
                        button {
                            class: "server-name-trigger",
                            onclick: move |_| {
                                let open = *dropdown_open.read();
                                dropdown_open.set(!open);
                            },
                            span { class: "server-name-text", "{server.name}" }
                            if *dropdown_open.read() {
                                span { class: "server-name-chevron", "▴" }
                            } else {
                                span { class: "server-name-chevron", "▾" }
                            }
                        }
                    }
                    if supports_channels_roles {
                        div { class: "server-banner-secondary-action",
                            button {
                                class: "server-channels-roles-btn server-channels-roles-btn-flat",
                                onclick: move |_| {
                                    let open = *channels_roles_open.read();
                                    channels_roles_open.set(!open);
                                },
                                span { class: "server-channels-roles-icon", "☰" }
                                span { "{t(\"server-banner-channels-roles\")}" }
                            }
                        }
                    }
                }

                // Dropdown panel (positioned absolutely over the sidebar).
                if *dropdown_open.read() {
                    nav { class: "server-dropdown-menu",
                        Link {
                            class: "server-dropdown-item",
                            to: Route::ServerSettingsRoute {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                            },
                            onclick: move |_| dropdown_open.set(false),
                            "{t(\"server-banner-settings\")}"
                        }
                        div { class: "context-menu-separator" }
                        button {
                            class: "server-dropdown-item",
                            onclick: move |_| {
                                // TODO(phase-3): open Invite People modal.
                                tracing::info!("Invite People clicked — placeholder");
                                dropdown_open.set(false);
                            },
                            "{t(\"server-banner-invite\")}"
                        }
                        button {
                            class: "server-dropdown-item",
                            onclick: move |_| {
                                // TODO(phase-3): open per-server notification settings.
                                tracing::info!("Notification Settings clicked — placeholder");
                                dropdown_open.set(false);
                            },
                            "{t(\"server-banner-notif-settings\")}"
                        }
                        div { class: "context-menu-separator" }
                        button {
                            class: "server-dropdown-item server-dropdown-item-danger",
                            onclick: move |_| {
                                // TODO(phase-3): hook to the leave-server confirmation flow.
                                tracing::info!("Leave Server clicked — placeholder");
                                dropdown_open.set(false);
                            },
                            "{t(\"server-banner-leave\")}"
                        }
                    }
                }

                if supports_channels_roles && *channels_roles_open.read() {
                    ChannelsRolesPanel { server: server.clone(), visible_category_ids }
                }
            } else {
                // ── Fallback (no server selected) ────────────────────────────
                div { class: "server-banner-header",
                    h3 { "{t(\"nav-dms\")}" }
                }
            }
        }
    }
}

/// DMs and Friends view — action shortcuts plus unified list of DMs + groups.
#[rustfmt::skip]
#[component]
fn DMFriendsView() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    // Only show DMs and groups belonging to the currently active account.
    let active_account_id = app_state.read().nav.active_account_id.clone();
    let active_user_id = active_account_id.as_ref().and_then(|account_id| {
        chat_data
            .read()
            .account_sessions
            .get(account_id)
            .map(|session| session.user.id.clone())
    });
    let new_conversation_label = t("dm-new-conversation");
    let saved_messages_label = t("dm-saved-messages");
    let dm_channels: Vec<_> = chat_data
        .read()
        .dm_channels
        .iter()
        .filter(|dm| {
            active_account_id.as_deref() == Some(&dm.account_id)
                && active_user_id.as_deref() != Some(dm.user.id.as_str())
        })
        .cloned()
        .collect();
    let groups: Vec<_> = chat_data
        .read()
        .groups
        .iter()
        .filter(|g| active_account_id.as_deref() == Some(&g.account_id))
        .cloned()
        .collect();
    let selected_channel = app_state.read().nav.selected_channel.clone();

    // Sort DMs by the latest incoming message from the other participant.
    let mut sorted_dms = dm_channels.clone();
    sorted_dms.sort_by(|a, b| {
        dm_last_incoming_timestamp(b)
            .cmp(&dm_last_incoming_timestamp(a))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.user.display_name.cmp(&b.user.display_name))
    });

    // Sort groups by the latest incoming message from another member.
    let mut sorted_groups = groups.clone();
    sorted_groups.sort_by(|a, b| {
        group_last_incoming_timestamp(b, active_user_id.as_deref())
            .cmp(&group_last_incoming_timestamp(a, active_user_id.as_deref()))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.name.cmp(&b.name))
    });

    // Pre-compute instance_ids for DMs and groups (cannot use let inside RSX for-loops)
    let dm_instance_ids: Vec<String> = sorted_dms
        .iter()
        .map(|dm| {
            chat_data
                .read()
                .account_sessions
                .get(&dm.account_id)
                .map(|s| s.instance_id.clone())
                .unwrap_or_default()
        })
        .collect();
    let group_instance_ids: Vec<String> = sorted_groups
        .iter()
        .map(|g| {
            chat_data
                .read()
                .account_sessions
                .get(&g.account_id)
                .map(|s| s.instance_id.clone())
                .unwrap_or_default()
        })
        .collect();

    rsx! {
        // New conversation button
        button {
            class: "dm-friends-row-btn",
            onclick: move |_| {
                let (backend_slug, instance_id, account_id) = {
                    let nav = &app_state.read().nav;
                    match (
                        nav.active_backend.clone(),
                        nav.active_instance_id.clone(),
                        nav.active_account_id.clone(),
                    ) {
                        (Some(b), Some(iid), Some(id)) => (b.slug().to_string(), iid, id),
                        _ => ("demo".to_string(), "demo".to_string(), "demo-cat".to_string()),
                    }
                };
                navigator()
                    .push(Route::NewConversationRoute {
                        backend: backend_slug,
                        instance_id,
                        account_id,
                    });
                close_mobile_drawer();
            },
            span { class: "dm-friends-row-icon", "✚" }
            span { class: "dm-friends-row-label", "{new_conversation_label}" }
        }

        button {
            class: "dm-friends-row-btn",
            onclick: move |_| {
                let (backend_slug, instance_id, account_id) = {
                    let nav = &app_state.read().nav;
                    match (
                        nav.active_backend.clone(),
                        nav.active_instance_id.clone(),
                        nav.active_account_id.clone(),
                    ) {
                        (Some(b), Some(iid), Some(id)) => (b.slug().to_string(), iid, id),
                        _ => ("demo".to_string(), "demo".to_string(), "demo-cat".to_string()),
                    }
                };
                navigator().push(Route::SavedItemsRoute {
                    backend: backend_slug,
                    instance_id,
                    account_id,
                });
                close_mobile_drawer();
            },
            span { class: "dm-friends-row-icon", "🔖" }
            span { class: "dm-friends-row-label", "{saved_messages_label}" }
        }

        // Unified DM + Group list
        div { class: "dm-unified-list",
            for (dm , dm_iid) in sorted_dms.iter().zip(dm_instance_ids.iter()) {
                DMChannelItem {
                    channel_id: dm.id.clone(),
                    display_name: dm.user.display_name.clone(),
                    user_id: dm.user.id.clone(),
                    unread: dm.unread_count,
                    is_active: selected_channel.as_deref() == Some(&dm.id),
                    account_id: dm.account_id.clone(),
                    backend_slug: dm.backend.slug().to_string(),
                    instance_id: dm_iid.clone(),
                    avatar_url: dm.user.avatar_url.clone(),
                    presence: dm.user.presence,
                }
            }

            for (group , group_iid) in sorted_groups.iter().zip(group_instance_ids.iter()) {
                GroupChannelItem {
                    group_id: group.id.clone(),
                    group_name: group.name.clone(),
                    members: group.members.clone(),
                    is_active: selected_channel.as_deref() == Some(&group.id),
                    account_id: group.account_id.clone(),
                    backend_slug: group.backend.slug().to_string(),
                    instance_id: group_iid.clone(),
                }
            }
        }
    }
}

/// Server channel view — categories and channels.
#[rustfmt::skip]
#[component]
fn ServerChannelView(visible_category_ids: Signal<Vec<String>>) -> Element {
    let app_state: Signal<AppState> = use_context();
    let _client_manager: Signal<ClientManager> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let channels = chat_data.read().channels.clone();
    let current_server = chat_data.read().current_server.clone();

    // Derive route construction fields for InlineCreateChannel.
    let instance_id = app_state.read().nav.active_instance_id.clone().unwrap_or_default();
    let account_id  = app_state.read().nav.active_account_id.clone().unwrap_or_default();

    if let Some(ref server) = current_server {
        // Collect all channel IDs that are already assigned to a category.
        let categorized_ids: Vec<String> = server
            .categories
            .iter()
            .flat_map(|cat| cat.channel_ids.iter().cloned())
            .collect();

        // Uncategorized: channels loaded from the server but not in any category.
        let uncategorized_ids: Vec<String> = channels
            .iter()
            .filter(|ch| !categorized_ids.contains(&ch.id))
            .map(|ch| ch.id.clone())
            .collect();

        // Backend slug for route construction.
        let backend_slug = server.backend.slug().to_string();
        let is_hn = server.backend.slug() == "hackernews";
        let is_github = server.backend.slug() == "github";
        // Read-only and demo backends do not support channel creation.
        let can_create = server.backend != "demo" && !is_hn && !is_github;
        let server_id = server.id.clone();

        // Is the current channel a (Lemmy-style) forum channel?
        // HackerNews uses its own sidebar, so it is excluded here.
        let current_ch_type = chat_data.read().current_channel.as_ref()
            .map(|ch| ch.channel_type);
        let is_forum = matches!(current_ch_type, Some(ChannelType::Forum));
        let current_channel_id = chat_data.read().current_channel.as_ref()
            .map(|ch| ch.id.clone())
            .unwrap_or_default();

        let current_route = use_route::<Route>();
        let on_comments = matches!(current_route, Route::ForumCommentsRoute { .. });

        rsx! {
            // Discord-style categories: shown for all backends, including HN.
            // Hidden only for Lemmy/forum backends whose sidebar replaces categories.
            if !is_forum {
                if !uncategorized_ids.is_empty() {
                    CategorySection {
                        cat_name: t("channel-list-text-channels"),
                        cat_channel_ids: uncategorized_ids,
                        channels: channels.clone(),
                    }
                }
                for category in &server.categories {
                    if visible_category_ids.read().is_empty()
                        || visible_category_ids.read().contains(&category.id)
                    {
                        CategorySection {
                            cat_name: category.name.clone(),
                            cat_channel_ids: category.channel_ids.clone(),
                            channels: channels.clone(),
                        }
                    }
                }
                // HN-specific footer: Algolia search link.
                if is_hn {
                    a {
                        class: "hn-algolia-link",
                        href: "https://hn.algolia.com/",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "🔍 Search on Algolia"
                    }
                }
            }
            // Forum (Lemmy-style) sidebar: Posts/Comments tabs + scope filters.
            if is_forum {
                div { class: "forum-sidebar-controls",
                    // Posts / Comments nav tabs
                    div { class: "forum-nav-tabs",
                        Link {
                            class: if !on_comments { "forum-nav-tab active" } else { "forum-nav-tab" },
                            to: Route::ServerChat {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                                channel_id: current_channel_id.clone(),
                            },
                            "Posts"
                        }
                        Link {
                            class: if on_comments { "forum-nav-tab active" } else { "forum-nav-tab" },
                            to: Route::ForumCommentsRoute {
                                backend: backend_slug.clone(),
                                instance_id: instance_id.clone(),
                                account_id: account_id.clone(),
                                server_id: server_id.clone(),
                                channel_id: current_channel_id.clone(),
                            },
                            "Comments"
                        }
                    }
                    // Scope filter — stacked vertically
                    button { class: "forum-filter-btn active forum-filter-full", "Subscribed" }
                    button { class: "forum-filter-btn forum-filter-full", "Local" }
                    button { class: "forum-filter-btn forum-filter-full", "All" }
                    // Show hidden toggle
                    button { class: "forum-filter-btn forum-filter-full forum-filter-text",
                        title: "Toggle hidden posts",
                        "Show hidden posts"
                    }
                    Link {
                        class: "forum-create-post-btn",
                        to: Route::CreateForumPostRoute {
                            backend: backend_slug,
                            instance_id,
                            account_id,
                            server_id,
                            channel_id: current_channel_id,
                        },
                        span { "+" }
                        span { "Create Post" }
                    }
                }
            } else if can_create {
                // "+ New Channel" link → full-page CreateChannelRoute (non-demo, non-HN only).
                Link {
                    class: "channel-create-btn",
                    to: Route::CreateChannelRoute {
                        backend: backend_slug,
                        instance_id,
                        account_id,
                        server_id,
                    },
                    span { class: "channel-create-btn-icon", "+" }
                    span { "{t(\"create-channel-btn\")}" }
                }
            }
        }
    } else {
        rsx! {}
    }
}

/// Demo-only panel to opt into category visibility, inspired by Discord's
/// Channels & Roles onboarding surface.
#[rustfmt::skip]
#[component]
fn ChannelsRolesPanel(server: Server, mut visible_category_ids: Signal<Vec<String>>) -> Element {
    let all_ids: Vec<String> = server.categories.iter().map(|c| c.id.clone()).collect();

    rsx! {
        div { class: "server-channels-roles-panel",
            div { class: "server-channels-roles-panel-header",
                h4 { "{t(\"server-banner-channels-roles\")}" }
                span { class: "server-channels-roles-subtitle", "{t(\"server-banner-browse-channels\")}" }
            }
            div { class: "server-channels-roles-list",
                for category in &server.categories {
                    {
                        let checked = visible_category_ids.read().is_empty()
                            || visible_category_ids.read().contains(&category.id);
                        let category_id = category.id.clone();
                        let all_ids_for_toggle = all_ids.clone();
                        rsx! {
                            label { class: "server-channels-role-row",
                                input {
                                    r#type: "checkbox",
                                    checked,
                                    onchange: move |evt| {
                                        let mut next = if visible_category_ids.read().is_empty() {
                                            all_ids_for_toggle.clone()
                                        } else {
                                            visible_category_ids.read().clone()
                                        };
                                        if evt.checked() {
                                            if !next.contains(&category_id) {
                                                next.push(category_id.clone());
                                            }
                                        } else {
                                            next.retain(|id| id != &category_id);
                                        }
                                        visible_category_ids.set(next);
                                    },
                                }
                                div { class: "server-channels-role-copy",
                                    span { class: "server-channels-role-name", "{category.name}" }
                                    span { class: "server-channels-role-meta",
                                        "{category.channel_ids.len()} {t(\"server-banner-channel-count\")}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Single DM channel item.
#[rustfmt::skip]
#[component]
fn DMChannelItem(
    channel_id: String,
    display_name: String,
    user_id: String,
    unread: u32,
    is_active: bool,
    account_id: String,
    /// Backend slug for routing (e.g. `"demo"`, `"stoat"`).
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    /// Optional avatar URL for the DM user.
    #[props(into)]
    avatar_url: Option<String>,
    /// Presence status for the status dot.
    presence: poly_client::PresenceStatus,
) -> Element {
    use crate::state::chat_data::user_color;
    use poly_client::PresenceStatus;
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let color = user_color(&user_id);
    let first_char: String = display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let presence_dot_class: &'static str = match presence {
        PresenceStatus::Online => "presence-dot online",
        PresenceStatus::Idle => "presence-dot idle",
        PresenceStatus::DoNotDisturb => "presence-dot dnd",
        PresenceStatus::Offline | PresenceStatus::Invisible => "",
    };

    rsx! {
        div {
            class: if is_active { "channel-item active" } else { "channel-item" },
            onclick: move |_| {
                if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone()
                {
                    remember_message_list_scroll_position(&previous_channel_id); // Clear group member list — this is an individual DM.
                }
                app_state.write().nav.selected_channel = Some(channel_id.clone());
                chat_data.write().active_group_members = Vec::new();
                app_state.write().nav.dm_right_sidebar_visible = false;
                chat_data.write().current_channel = Some(Channel {
                    id: channel_id.clone(),
                    name: display_name.clone(),
                    channel_type: ChannelType::Text,
                    server_id: String::new(),
                    unread_count: unread,
                    mention_count: 0,
                    last_message_id: None,
                });
                chat_data.write().current_server = None;
                let cid = channel_id.clone();
                let aid = account_id.clone();
                spawn(async move {
                    load_dm_messages(cid, aid, client_manager, chat_data).await;
                });
                navigator()
                    .push(Route::DmChat {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                        dm_id: channel_id.clone(),
                    });
                close_mobile_drawer();
            },
            div { class: "dm-avatar-wrap",
                div { class: "dm-avatar-small", style: "background-color: {color};",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "dm-avatar-img",
                            src: "{url}",
                            alt: "{first_char}",
                        }
                    } else {
                        "{first_char}"
                    }
                }
                if !presence_dot_class.is_empty() {
                    span { class: "{presence_dot_class}" }
                }
            }
            span { class: "channel-name", "{display_name}" }
            if unread > 0 {
                span { class: "unread-badge", "{unread}" }
            }
        }
    }
}

/// Single group channel item.
#[rustfmt::skip]
#[component]
fn GroupChannelItem(
    group_id: String,
    group_name: Option<String>,
    members: Vec<User>,
    is_active: bool,
    account_id: String,
    /// Backend slug for routing (e.g. `"demo"`, `"stoat"`).
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let display_name = group_name.unwrap_or_else(|| {
        members
            .iter()
            .map(|m| m.display_name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let member_count = members.len();

    rsx! {
        div {
            class: if is_active { "channel-item active" } else { "channel-item" },
            onclick: move |_| {
                if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone()
                {
                    remember_message_list_scroll_position(&previous_channel_id); // Populate group members for the DM member sidebar.
                } // Synthesize a Channel so ChatView can display the group header
                app_state.write().nav.selected_channel = Some(group_id.clone());
                chat_data.write().active_group_members = members.clone();
                chat_data.write().current_channel = Some(Channel {
                    id: group_id.clone(),
                    name: display_name.clone(),
                    channel_type: ChannelType::Text,
                    server_id: String::new(),
                    unread_count: 0,
                    mention_count: 0,
                    last_message_id: None,
                });
                chat_data.write().current_server = None;
                let cid = group_id.clone();
                let aid = account_id.clone();
                spawn(async move {
                    load_dm_messages(cid, aid, client_manager, chat_data).await;
                });
                navigator()
                    .push(Route::DmChat {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                        dm_id: group_id.clone(),
                    });
                close_mobile_drawer();
            },
            span { class: "channel-icon", "👥" }
            span { class: "channel-name", "{display_name}" }
            span { class: "dm-member-count", "({member_count})" }
        }
    }
}

/// Friend contact in search results.
#[rustfmt::skip]
#[component]
fn FriendItem(display_name: String, user_id: String) -> Element {
    use crate::state::chat_data::user_color;

    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();

    let color = user_color(&user_id);
    let first_char: String = display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();

    rsx! {
        div {
            class: "channel-item",
            onclick: {
                let target_user_id = user_id.clone();
                move |_| {
                    open_direct_message_from_active_account(
                        target_user_id.clone(),
                        app_state,
                        chat_data,
                        client_manager,
                        nav,
                    );
                }
            },
            div { class: "dm-avatar-small", style: "background-color: {color};", "{first_char}" }
            span { class: "channel-name", "{display_name}" }
        }
    }
}

/// Category header + channels within the category.
///
/// Clicking the category header toggles collapse/expand of its channel list.
#[rustfmt::skip]
#[component]
fn CategorySection(
    cat_name: String,
    cat_channel_ids: Vec<String>,
    channels: Vec<Channel>,
) -> Element {
    let mut collapsed = use_signal(|| false);
    let is_collapsed = *collapsed.read();

    rsx! {
        div { class: "channel-category",
            div {
                class: "category-header",
                onclick: move |_| collapsed.set(!is_collapsed),
                span { class: if is_collapsed { "category-chevron collapsed" } else { "category-chevron" },
                    "▾"
                }
                span { class: "category-name", "{cat_name}" }
            }
            if !is_collapsed {
                for ch_id in &cat_channel_ids {
                    {
                        if let Some(channel) = channels.iter().find(|c| &c.id == ch_id).cloned() {
                            rsx! {
                                ChannelItemRow { channel }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }
            }
        }
    }
}

/// Single server channel row (with voice participants if applicable).
#[rustfmt::skip]
#[component]
fn ChannelItemRow(channel: Channel) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let selected_channel = app_state.read().nav.selected_channel.clone();
    let ch_id = channel.id.clone();
    let ch_name = channel.name.clone();
    let ch_type = channel.channel_type;
    let unread = channel.unread_count;
    let mention = channel.mention_count;
    let server_id_for_menu = channel.server_id.clone();
    let channel_for_click = channel.clone();
    let ch_id_for_menu = ch_id.clone();
    let ch_name_for_menu = ch_name.clone();
    let is_active = selected_channel.as_deref() == Some(&ch_id);
    let account_id_for_menu = app_state.read().nav.active_account_id.clone().unwrap_or_default();
    let backend_slug_for_menu = app_state
        .read()
        .nav
        .active_backend
        .as_ref()
        .map(|b| b.slug().to_string())
        .unwrap_or_else(|| "demo".to_string());
    let instance_id_for_menu = app_state.read().nav.active_instance_id.clone().unwrap_or_default();

    // Long-press detection for mobile (touchstart → 500 ms → context menu).
    // A monotonically-increasing generation counter lets the touchend/touchmove
    // handlers cancel an in-flight timer without needing a JS clearTimeout handle.
    let mut touch_cancel_gen: Signal<u32> = use_signal(|| 0);

    let type_icon = match ch_type {
        ChannelType::Text => "#",
        ChannelType::Voice => "🔊",
        ChannelType::Video => "📹",
        ChannelType::Forum | ChannelType::HackerNews => "📋",
        ChannelType::Code => "📁",
    };

    // Active wins over unread; unread class makes the channel name bold.
    let channel_class = if is_active {
        "channel-item active"
    } else if unread > 0 {
        "channel-item unread"
    } else {
        "channel-item"
    };

    let voice_participants = if matches!(ch_type, ChannelType::Voice | ChannelType::Video) {
        chat_data
            .read()
            .voice_channel_participants
            .get(&ch_id)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Pre-clone menu data for each closure that needs it.
    // (oncontextmenu and ontouchstart both need their own owned copies.)
    let ch_id_ctx = ch_id_for_menu.clone();
    let ch_name_ctx = ch_name_for_menu.clone();
    let account_id_ctx = account_id_for_menu.clone();
    let server_id_ctx = server_id_for_menu.clone();
    let instance_id_ctx = instance_id_for_menu.clone();
    let backend_slug_ctx = backend_slug_for_menu.clone();

    rsx! {
        div {
            class: "{channel_class}",
            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
                let coords = evt.client_coordinates();
                app_state.write().channel_context_menu = Some(ChannelContextMenuState {
                    x: coords.x,
                    y: coords.y,
                    channel_id: ch_id_for_menu.clone(),
                    channel_name: ch_name_for_menu.clone(),
                    account_id: account_id_for_menu.clone(),
                    server_id: server_id_for_menu.clone(),
                    instance_id: instance_id_for_menu.clone(),
                    backend_slug: backend_slug_for_menu.clone(),
                });
            },
            // Mobile long-press: open context menu after 500 ms of sustained touch.
            ontouchstart: {
                let ch_id_ts = ch_id_ctx.clone();
                let ch_name_ts = ch_name_ctx.clone();
                let account_id_ts = account_id_ctx.clone();
                let server_id_ts = server_id_ctx.clone();
                let instance_id_ts = instance_id_ctx.clone();
                let backend_slug_ts = backend_slug_ctx.clone();
                move |evt: TouchEvent| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    // Grab the first touch point's client coordinates.
                    let (x, y) = evt.touches()
                        .first()
                        .map(|t| {
                            let c = t.client_coordinates();
                            (c.x, c.y)
                        })
                        .unwrap_or((0.0, 0.0));

                    // Advance the generation so any previous pending timer is invalidated.
                    let touch_gen = {
                        let next = touch_cancel_gen.peek().wrapping_add(1);
                        touch_cancel_gen.set(next);
                        next
                    };

                    let ch_id_ts = ch_id_ts.clone();
                    let ch_name_ts = ch_name_ts.clone();
                    let account_id_ts = account_id_ts.clone();
                    let server_id_ts = server_id_ts.clone();
                    let instance_id_ts = instance_id_ts.clone();
                    let backend_slug_ts = backend_slug_ts.clone();

                    spawn(async move {
                        // Wait 500 ms via JS setTimeout.
                        let mut eval = dioxus::prelude::document::eval(
                            "setTimeout(() => dioxus.send(true), 500)"
                        );
                        let Ok(true) = eval.recv::<bool>().await else {
                            return;
                        };
                        // If the generation hasn't changed, the touch was not cancelled.
                        if *touch_cancel_gen.peek() != touch_gen {
                            return;
                        }
                        app_state.write().channel_context_menu = Some(ChannelContextMenuState {
                            x,
                            y,
                            channel_id: ch_id_ts,
                            channel_name: ch_name_ts,
                            account_id: account_id_ts,
                            server_id: server_id_ts,
                            instance_id: instance_id_ts,
                            backend_slug: backend_slug_ts,
                        });
                    });
                }
            },
            // Cancel the long-press timer on release, movement, or cancel.
            ontouchend: move |evt| {
                evt.stop_propagation();
                let next = touch_cancel_gen.peek().wrapping_add(1);
                touch_cancel_gen.set(next);
            },
            ontouchmove: move |evt| {
                evt.stop_propagation();
                let next = touch_cancel_gen.peek().wrapping_add(1);
                touch_cancel_gen.set(next);
            },
            ontouchcancel: move |evt| {
                evt.stop_propagation();
                let next = touch_cancel_gen.peek().wrapping_add(1);
                touch_cancel_gen.set(next);
            },
            onclick: move |_| {
                if let Some(previous_channel_id) = app_state.read().nav.selected_channel.clone()
                {
                    remember_message_list_scroll_position(&previous_channel_id);
                }
                app_state.write().nav.selected_channel = Some(ch_id.clone());
                chat_data.write().current_channel = Some(channel_for_click.clone());
                // Persist last visited channel for this server (fire-and-forget).
                let server_id_for_persist = channel.server_id.clone();
                let channel_id_for_persist = ch_id.clone();
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get() {
                        let _ = storage
                            .set_last_channel_for_server(&server_id_for_persist, &channel_id_for_persist)
                            .await;
                    }
                });
                let cid = ch_id.clone();
                spawn(async move {
                    load_channel_data(cid, client_manager, chat_data, app_state).await;
                });
                let server_id = app_state.read().nav.selected_server.clone().unwrap_or_default();
                let (backend_slug, instance_id, account_id) = {
                    let nav = &app_state.read().nav;
                    match (
                        nav.active_backend.clone(),
                        nav.active_instance_id.clone(),
                        nav.active_account_id.clone(),
                    ) {
                        (Some(b), Some(iid), Some(id)) => (b.slug().to_string(), iid, id),
                        _ => ("demo".to_string(), "demo".to_string(), "demo-cat".to_string()),
                    }
                };
                navigator()
                    .push(Route::ServerChat {
                        backend: backend_slug,
                        instance_id,
                        account_id,
                        server_id,
                        channel_id: ch_id.clone(),
                    });
                close_mobile_drawer();
            },
            span { class: "channel-icon", "{type_icon}" }
            span { class: "channel-name", "{ch_name}" }
            // @mention badge (red) — only for direct @mentions, not general unread.
            // Plain unread is conveyed via the "unread" CSS class (bold channel name).
            if mention > 0 {
                span { class: "mention-badge", "@{mention}" }
            }
        }
        if !voice_participants.is_empty() {
            div { class: "voice-channel-users",
                for vp in &voice_participants {
                    VoiceParticipantEntry { participant: vp.clone() }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn VoiceParticipantEntry(participant: VoiceParticipant) -> Element {
    use crate::state::chat_data::user_color;

    let vp_name = participant.user.display_name.clone();
    let vp_id = participant.user.id.clone();
    let vp_color = user_color(&vp_id);
    let vp_first: String = vp_name
        .chars()
        .next()
        .map(|c: char| c.to_string())
        .unwrap_or_default();
    let vp_avatar_url = participant.user.avatar_url.clone();

    rsx! {
        div { class: "voice-user-entry",
            div { class: "voice-user-avatar",
                if let Some(url) = &vp_avatar_url {
                    img {
                        src: "{url}",
                        alt: "{vp_name}",
                        class: "voice-user-avatar-image",
                    }
                } else {
                    div {
                        style: "background-color: {vp_color};",
                        class: "voice-user-avatar-fallback",
                        "{vp_first}"
                    }
                }
            }
            span { class: "voice-user-name", "{vp_name}" }
            if participant.is_muted {
                span { class: "voice-user-icon", "🔇" }
            }
            if participant.is_deafened {
                span { class: "voice-user-icon", "🔕" }
            }
            if participant.is_streaming {
                span { class: "voice-user-icon", "🖥" }
            }
            if participant.is_video_on {
                span { class: "voice-user-icon", "📹" }
            }
        }
    }
}
