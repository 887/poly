//! Channel list — categories and channels for the selected server.
//!
//! In Server view: shows the server name + source banner, collapsible categories,
//! and channels with type icons (#, 🔊, 📹) and unread indicators.
//!
//! In DMs/Friends view: shows DM channels, groups, and Friends list.
//!
//! All data comes from `Signal<ChatData>`.
// TODO(phase-2.5.5): Wire channel list to backend data

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::{backend_badge, user_color};
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_client::ChannelType;

/// Channel list component.
///
/// In Server view: shows server name header with source info, categories,
/// and channels with type icons and unread indicators.
///
/// In DMs/Friends view: shows DM channels, groups, and friends.
#[component]
pub fn ChannelList() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let selected_channel = app_state.read().nav.selected_channel.clone();
    let current_view = app_state.read().nav.view;

    let channels = chat_data.read().channels.clone();
    let current_server = chat_data.read().current_server.clone();
    let dm_channels = chat_data.read().dm_channels.clone();
    let groups = chat_data.read().groups.clone();
    let friends = chat_data.read().friends.clone();
    let mut dm_filter = use_signal(String::new);

    // Pre-compute DM filter/sort so values can be referenced inside rsx! arms.
    let filter_val = dm_filter.read().clone();
    let filter_lower = filter_val.to_lowercase();

    let mut sorted_dms = dm_channels.clone();
    sorted_dms.sort_by(|a, b| {
        b.unread_count.cmp(&a.unread_count).then_with(|| {
            b.last_message
                .as_ref()
                .map(|m| m.timestamp)
                .cmp(&a.last_message.as_ref().map(|m| m.timestamp))
        })
    });
    let mut sorted_groups = groups.clone();
    sorted_groups.sort_by(|a, b| {
        b.last_message
            .as_ref()
            .map(|m| m.timestamp)
            .cmp(&a.last_message.as_ref().map(|m| m.timestamp))
    });
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
    let has_no_data = dm_channels.is_empty() && groups.is_empty() && friends.is_empty();
    let no_filter_results = !filter_lower.is_empty()
        && filtered_dms.is_empty()
        && filtered_groups.is_empty()
        && show_friends.is_empty();

    rsx! {
        aside { class: "channel-list",
            // Header
            div { class: "channel-list-header",
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

            div { class: "channel-entries",
                if current_view == View::DmsFriends {
                    // ── DMs / Friends view ───────────────────────────────
                    // Search bar: find conversations or contacts across all accounts
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
                    // Unified DM + Group list sorted by recency (newest up top)
                    div { class: "dm-unified-list",
                        // Direct message channels
                        for dm in &filtered_dms {
                            {
                                let dm_id = dm.id.clone();
                                let name = dm.user.display_name.clone();
                                let badge = backend_badge(&dm.backend);
                                let unread = dm.unread_count;
                                let color = user_color(&dm.user.id);
                                let first_char: String = name
                                    .chars()
                                    .next()
                                    .map(|c| c.to_string())
                                    .unwrap_or_default();
                                let is_active = selected_channel.as_deref() == Some(&dm_id);
                                rsx! {
                                    div {
                                        class: if is_active { "channel-item active" } else { "channel-item" },
                                        onclick: {
                                            let dm_id_click = dm_id.clone();
                                            move |_| {
                                                app_state.write().push_nav_history();
                                                app_state.write().nav.selected_channel =
                                                    Some(dm_id_click.clone());
                                            }
                                        },
                                        div { class: "dm-avatar-small", style: "background-color: {color};", "{first_char}" }
                                        span { class: "channel-name", "{name}" }
                                        span { class: "source-badge-inline", "{badge}" }
                                        if unread > 0 {
                                            span { class: "unread-badge", "{unread}" }
                                        }
                                    }
                                }
                            }
                        }
                        // Group chats (sorted by last message timestamp)
                        for group in &filtered_groups {
                            {
                                let group_name = group
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| {
                                        group
                                            .members
                                            .iter()
                                            .map(|m| m.display_name.clone())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    });
                                let group_id = group.id.clone();
                                let badge = backend_badge(&group.backend);
                                let member_count = group.members.len();
                                let is_active = selected_channel.as_deref() == Some(&group_id);
                                rsx! {
                                    div {
                                        class: if is_active { "channel-item active" } else { "channel-item" },
                                        onclick: {
                                            let gid = group_id.clone();
                                            move |_| {
                                                app_state.write().push_nav_history();
                                                app_state.write().nav.selected_channel =
                                                    Some(gid.clone());
                                            }
                                        },
                                        span { class: "channel-icon", "👥" }
                                        span { class: "channel-name", "{group_name}" }
                                        span { class: "source-badge-inline", "{badge}" }
                                        span { class: "dm-member-count", "({member_count})" }
                                    }
                                }
                            }
                        }
                        // Contacts from all accounts — shown only when search is active
                        if !show_friends.is_empty() {
                            div { class: "dm-section-header", "{t(\"nav-friends\")}" }
                            for friend in &show_friends {
                                {
                                    let name = friend.display_name.clone();
                                    let badge = backend_badge(&friend.backend);
                                    let color = user_color(&friend.id);
                                    let first_char: String = name
                                        .chars()
                                        .next()
                                        .map(|c| c.to_string())
                                        .unwrap_or_default();
                                    rsx! {
                                        div { class: "channel-item",
                                            div { class: "dm-avatar-small", style: "background-color: {color};", "{first_char}" }
                                            span { class: "channel-name", "{name}" }
                                            span { class: "source-badge-inline", "{badge}" }
                                        }
                                    }
                                }
                            }
                        }
                        // Empty states
                        if has_no_data {
                            div { class: "channel-empty",
                                p { "Toggle the 🧪 demo to see sample data" }
                            }
                        } else if no_filter_results {
                            div { class: "dm-no-results", "{t(\"dm-no-results\")}" }
                        }
                    }
                } else if let Some(ref server) = current_server {
                    // Render channels grouped by categories
                    for category in &server.categories {
                        {
                            let cat_name = category.name.clone();
                            let cat_channel_ids = category.channel_ids.clone();
                            rsx! {
                                div { class: "channel-category",
                                    div { class: "category-header",
                                        span { class: "category-chevron", "▾" }
                                        span { class: "category-name", "{cat_name}" }
                                    }
                                    for ch_id in &cat_channel_ids {
                                        {
                                            // Find the channel in loaded data
                                            let channel = channels.iter().find(|c| &c.id == ch_id).cloned();
                                            if let Some(channel) = channel {
                                                let ch_id_click = channel.id.clone();
                                                let ch_name = channel.name.clone();
                                                let ch_type = channel.channel_type;
                                                let unread = channel.unread_count;
                                                let is_active = selected_channel.as_deref() == Some(&ch_id_click);
                                                let type_icon = match ch_type {
                                                    ChannelType::Text => "#",
                                                    ChannelType::Voice => "🔊",
                                                    ChannelType::Video => "📹",
                                                };

                                                // Get voice participants for voice/video channels
                                                let voice_participants = if matches!(
                                                    ch_type,
                                                    ChannelType::Voice | ChannelType::Video
                                                ) {
                                                    chat_data
                                                        .read()
                                                        .voice_channel_participants
                                                        .get(&ch_id_click)
                                                        .cloned()
                                                        .unwrap_or_default()
                                                } else {
                                                    Vec::new()
                                                };
                                                rsx! {
                                                    div {
                                                        class: if is_active { "channel-item active" } else { "channel-item" },
                                                        onclick: {
                                                            let ch_id_inner = ch_id_click.clone();
                                                            let channel_clone = channel.clone();
                                                            move |_| {
                                                                app_state.write().push_nav_history();
                                                                app_state.write().nav.selected_channel = Some(ch_id_inner.clone());
                                                                chat_data.write().current_channel = Some(channel_clone.clone());
                                                                // Load messages for this channel
                                                                let cid = ch_id_inner.clone();
                                                                spawn(async move {
                                                                    load_channel_data(cid, client_manager, chat_data, app_state).await;
                                                                });
                                                            }
                                                        },
                                                        span { class: "channel-icon", "{type_icon}" }
                                                        span { class: "channel-name", "{ch_name}" }
                                                        if unread > 0 {
                                                            span { class: "unread-badge", "{unread}" }
                                                        }
                                                    }
                                                    // Voice channel: show connected participants underneath
                                                    if !voice_participants.is_empty() {
                                                        div { class: "voice-channel-users",
                                                            for vp in &voice_participants {
                                                                {
                                                                    let vp_name = vp.user.display_name.clone();
                                                                    let vp_color = user_color(&vp.user.id);
                                                                    let vp_first: String = vp_name
                                                                        .chars()
                                                                        .next()
                                                                        .map(|c| c.to_string())
                                                                        .unwrap_or_default();
                                                                    rsx! {
                                                                        div { class: "voice-user-entry",
                                                                            div { class: "voice-user-avatar", style: "background-color: {vp_color};", "{vp_first}" }
                                                                            span { class: "voice-user-name", "{vp_name}" }
                                                                            if vp.is_muted {
                                                                                span { class: "voice-user-icon", "🔇" }
                                                                            }
                                                                            if vp.is_deafened {
                                                                                span { class: "voice-user-icon", "🔕" }
                                                                            }
                                                                            if vp.is_streaming {
                                                                                span { class: "voice-user-icon", "🖥" }
                                                                            }
                                                                            if vp.is_video_on {
                                                                                span { class: "voice-user-icon", "📹" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
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
                } else if channels.is_empty() {
                    div { class: "channel-empty",
                        p { "{t(\"chat-no-messages\")}" }
                    }
                }
            }
        }
    }
}

/// Load messages and members for a channel.
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

    // Load messages
    let guard = backend.read().await;
    if let Ok(messages) = guard
        .get_messages(&channel_id, poly_client::MessageQuery::default())
        .await
    {
        chat_data.write().messages = messages;
    }

    // Load members
    if let Ok(members) = guard.get_channel_members(&channel_id).await {
        chat_data.write().members = members;
    }

    chat_data.write().loading = false;
}
