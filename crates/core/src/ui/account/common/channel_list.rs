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
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::backend_badge;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_client::{Channel, ChannelType, Server, User, VoiceParticipant};

/// Main channel list component — delegates to sub-views based on current view.
#[component]
pub fn ChannelList() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let current_view = app_state.read().nav.view;
    let current_server = chat_data.read().current_server.clone();

    rsx! {
        aside { class: "channel-list",
            ServerBanner { current_view, current_server: current_server.clone() }

            div { class: "channel-entries",
                if current_view == View::DmsFriends {
                    DMFriendsView {}
                } else if current_server.is_some() {
                    ServerChannelView {}
                } else {
                    div { class: "channel-empty",
                        p { "{t(\"chat-no-messages\")}" }
                    }
                }
            }
        }
    }
}

/// Server banner — displays the current server name or the "Direct Messages" title.
#[component]
fn ServerBanner(current_view: View, current_server: Option<Server>) -> Element {
    rsx! {
        div { class: "server-banner-sidebar",
            if current_view == View::DmsFriends {
                h3 { "{t(\"nav-dms\")}" }
            } else if let Some(ref server) = current_server {
                h3 { class: "server-name", "{server.name}" }
                div { class: "server-source",
                    span { class: "source-badge-inline", "{backend_badge(&server.backend)}" }
                    span { class: "source-text",
                        "{server.backend.display_name()} — {server.account_display_name}"
                    }
                }
            } else {
                h3 { "{t(\"nav-dms\")}" }
            }
        }
    }
}

/// DMs and Friends view — search, unified list of DMs + groups + friend contacts.
#[component]
fn DMFriendsView() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    // Only show DMs and groups belonging to the currently active account.
    let active_account_id = app_state.read().nav.active_account_id.clone();
    let dm_channels: Vec<_> = chat_data
        .read()
        .dm_channels
        .iter()
        .filter(|dm| active_account_id.as_deref() == Some(&dm.account_id))
        .cloned()
        .collect();
    let groups: Vec<_> = chat_data
        .read()
        .groups
        .iter()
        .filter(|g| active_account_id.as_deref() == Some(&g.account_id))
        .cloned()
        .collect();
    let friends = chat_data.read().friends.clone();
    let selected_channel = app_state.read().nav.selected_channel.clone();
    let mut dm_filter = use_signal(String::new);

    let filter_val = dm_filter.read().clone();
    let filter_lower = filter_val.to_lowercase();

    // Sort DMs by unread + recency
    let mut sorted_dms = dm_channels.clone();
    sorted_dms.sort_by(|a, b| {
        b.unread_count.cmp(&a.unread_count).then_with(|| {
            b.last_message
                .as_ref()
                .map(|m| m.timestamp)
                .cmp(&a.last_message.as_ref().map(|m| m.timestamp))
        })
    });

    // Sort groups by recency
    let mut sorted_groups = groups.clone();
    sorted_groups.sort_by(|a, b| {
        b.last_message
            .as_ref()
            .map(|m| m.timestamp)
            .cmp(&a.last_message.as_ref().map(|m| m.timestamp))
    });

    // Apply filter
    let filtered_dms: Vec<_> = sorted_dms
        .into_iter()
        .filter(|dm| {
            filter_lower.is_empty() || dm.user.display_name.to_lowercase().contains(&filter_lower)
        })
        .collect();

    let filtered_groups: Vec<_> = sorted_groups
        .into_iter()
        .filter(|g| {
            if filter_lower.is_empty() {
                return true;
            }
            let name = g.name.clone().unwrap_or_else(|| {
                g.members
                    .iter()
                    .map(|m| m.display_name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            name.to_lowercase().contains(&filter_lower)
        })
        .collect();

    let show_friends: Vec<_> = if filter_lower.is_empty() {
        vec![]
    } else {
        friends
            .iter()
            .filter(|f| f.display_name.to_lowercase().contains(&filter_lower))
            .collect()
    };

    let no_filter_results = !filter_lower.is_empty()
        && filtered_dms.is_empty()
        && filtered_groups.is_empty()
        && show_friends.is_empty();

    // Pre-compute instance_ids for DMs and groups (cannot use let inside RSX for-loops)
    let dm_instance_ids: Vec<String> = filtered_dms
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
    let group_instance_ids: Vec<String> = filtered_groups
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
        // Friends button
        button {
            class: "dm-friends-row-btn",
            onclick: move |_| {
                // Navigate to Friends for the currently active account.
                let (backend_slug, instance_id, account_id) = {
                    let nav = &app_state.read().nav;
                    match (
                        nav.active_backend,
                        nav.active_instance_id.clone(),
                        nav.active_account_id.clone(),
                    ) {
                        (Some(b), Some(iid), Some(id)) => (b.slug().to_string(), iid, id),
                        _ => ("demo".to_string(), "demo".to_string(), "demo-cat".to_string()),
                    }
                };
                navigator()
                    .push(Route::FriendsRoute {
                        backend: backend_slug,
                        instance_id,
                        account_id,
                    });
            },
            span { class: "dm-friends-row-icon", "👥" }
            span { class: "dm-friends-row-label", "{t(\"friends-title\")}" }
        }

        // Search bar
        div { class: "dm-search-bar",
            input {
                r#type: "text",
                class: "dm-search-input",
                placeholder: "{t(\"dm-search-placeholder\")}",
                value: "{filter_val}",
                oninput: move |e| dm_filter.set(e.value()),
            }
            if !filter_val.is_empty() {
                button {
                    class: "dm-search-clear",
                    onclick: move |_| dm_filter.set(String::new()),
                    "×"
                }
            }
        }

        // Unified DM + Group list
        div { class: "dm-unified-list",
            for (dm , dm_iid) in filtered_dms.iter().zip(dm_instance_ids.iter()) {
                DMChannelItem {
                    channel_id: dm.id.clone(),
                    display_name: dm.user.display_name.clone(),
                    user_id: dm.user.id.clone(),
                    unread: dm.unread_count,
                    is_active: selected_channel.as_deref() == Some(&dm.id),
                    account_id: dm.account_id.clone(),
                    backend_slug: dm.backend.slug().to_string(),
                    instance_id: dm_iid.clone(),
                }
            }

            for (group , group_iid) in filtered_groups.iter().zip(group_instance_ids.iter()) {
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

            if !show_friends.is_empty() {
                div { class: "dm-section-header", "{t(\"nav-friends\")}" }
                for friend in &show_friends {
                    FriendItem {
                        display_name: friend.display_name.clone(),
                        user_id: friend.id.clone(),
                    }
                }
            }

            if no_filter_results {
                div { class: "dm-no-results", "{t(\"dm-no-results\")}" }
            }
        }
    }
}

/// Server channel view — categories and channels.
#[component]
fn ServerChannelView() -> Element {
    let app_state: Signal<AppState> = use_context();
    let _client_manager: Signal<ClientManager> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let channels = chat_data.read().channels.clone();
    let current_server = chat_data.read().current_server.clone();
    let _selected_channel = app_state.read().nav.selected_channel.clone();

    if let Some(ref server) = current_server {
        rsx! {
            for category in &server.categories {
                CategorySection {
                    cat_name: category.name.clone(),
                    cat_channel_ids: category.channel_ids.clone(),
                    channels: channels.clone(),
                }
            }
        }
    } else {
        rsx! {}
    }
}

/// Single DM channel item.
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
) -> Element {
    use crate::state::chat_data::user_color;
    let mut app_state: Signal<AppState> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let color = user_color(&user_id);
    let first_char: String = display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();

    rsx! {
        div {
            class: if is_active { "channel-item active" } else { "channel-item" },
            onclick: move |_| {
                app_state.write().nav.selected_channel = Some(channel_id.clone());
                // Clear group member list — this is an individual DM.
                chat_data.write().active_group_members = Vec::new();
                app_state.write().nav.dm_right_sidebar_visible = false;
                // Synthesize a Channel so ChatView can display the DM header
                chat_data.write().current_channel = Some(Channel {
                    id: channel_id.clone(),
                    name: display_name.clone(),
                    channel_type: ChannelType::Text,
                    server_id: String::new(),
                    unread_count: 0,
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
            },
            div { class: "dm-avatar-small", style: "background-color: {color};", "{first_char}" }
            span { class: "channel-name", "{display_name}" }
            if unread > 0 {
                span { class: "unread-badge", "{unread}" }
            }
        }
    }
}

/// Single group channel item.
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
                app_state.write().nav.selected_channel = Some(group_id.clone());
                // Populate group members for the DM member sidebar.
                chat_data.write().active_group_members = members.clone();
                // Synthesize a Channel so ChatView can display the group header
                chat_data.write().current_channel = Some(Channel {
                    id: group_id.clone(),
                    name: display_name.clone(),
                    channel_type: ChannelType::Text,
                    server_id: String::new(),
                    unread_count: 0,
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
            },
            span { class: "channel-icon", "👥" }
            span { class: "channel-name", "{display_name}" }
            span { class: "dm-member-count", "({member_count})" }
        }
    }
}

/// Friend contact in search results.
#[component]
fn FriendItem(display_name: String, user_id: String) -> Element {
    use crate::state::chat_data::user_color;

    let color = user_color(&user_id);
    let first_char: String = display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();

    rsx! {
        div { class: "channel-item",
            div { class: "dm-avatar-small", style: "background-color: {color};", "{first_char}" }
            span { class: "channel-name", "{display_name}" }
        }
    }
}

/// Category header + channels within the category.
///
/// Clicking the category header toggles collapse/expand of its channel list.
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
    let is_active = selected_channel.as_deref() == Some(&ch_id);

    let type_icon = match ch_type {
        ChannelType::Text => "#",
        ChannelType::Voice => "🔊",
        ChannelType::Video => "📹",
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

    rsx! {
        div {
            class: if is_active { "channel-item active" } else { "channel-item" },
            onclick: move |_| {
                app_state.write().nav.selected_channel = Some(ch_id.clone());
                chat_data.write().current_channel = Some(channel.clone());
                let cid = ch_id.clone();
                spawn(async move {
                    load_channel_data(cid, client_manager, chat_data, app_state).await;
                });
                let server_id = app_state.read().nav.selected_server.clone().unwrap_or_default();
                let (backend_slug, instance_id, account_id) = {
                    let nav = &app_state.read().nav;
                    match (
                        nav.active_backend,
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
            },
            span { class: "channel-icon", "{type_icon}" }
            span { class: "channel-name", "{ch_name}" }
            if unread > 0 {
                span { class: "unread-badge", "{unread}" }
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

/// Single connected voice participant entry.
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

/// Load messages, members, and voice participants for a channel.
async fn load_channel_data(
    channel_id: String,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
) {
    chat_data.write().loading = true;

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
            // Text channel — load messages and members
            if let Ok(messages) = guard
                .get_messages(&channel_id, poly_client::MessageQuery::default())
                .await
            {
                chat_data.write().messages = messages;
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

    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        chat_data.write().loading = false;
        return;
    };

    let guard = backend.read().await;
    if let Ok(messages) = guard
        .get_messages(&channel_id, poly_client::MessageQuery::default())
        .await
    {
        chat_data.write().messages = messages;
    }
    if let Ok(members) = guard.get_channel_members(&channel_id).await {
        chat_data.write().members = members;
    }

    chat_data.write().loading = false;
}
