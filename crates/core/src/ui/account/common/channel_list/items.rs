//! Individual channel-list row components shared by server and DM views.
//!
//! Single Responsibility: each component owns exactly one row's data-fetch +
//! render concern.
//!
//! - `ChannelItemRow` — a single server channel row (text/voice/video/forum).
//! - `CategorySection` — collapsible category header + its channels.
//! - `VoiceParticipantEntry` — avatar chip for a connected voice participant.
//! - `DMChannelItem` — a single DM channel row.
//! - `GroupChannelItem` — a single group-DM row.

use super::dm_view::load_dm_messages;
use super::server_view::load_channel_data;
use super::ChannelListAction;
use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::state::{
    AppState, ChannelContextMenuState, ChatLists, ChatViewState, DmContextMenuState,
    GroupDmContextMenuState, VoiceState,
};
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::context_menu::menus::{channel_entry_at, dm_entry_at, group_dm_entry_at};
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{Channel, ChannelType, User, VoiceParticipant};
use poly_ui_macros::{context_menu, ui_action};

/// Category header + channels within the category.
///
/// Clicking the category header toggles collapse/expand of its channel list.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(ChannelListAction)]
#[component]
pub(super) fn CategorySection(
    cat_name: String,
    cat_channel_ids: Vec<String>,
) -> Element {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
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
                        if let Some(channel) = chat_lists.peek().channel_by_id(ch_id).cloned() {
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ChannelItemRow(channel: Channel) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    let selected_channel = nav.read().selected_channel.clone();
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
    let account_id_for_menu = nav.read().active_account_id.cloned().unwrap_or_default();
    let backend_slug_for_menu = nav
        .read()
                .active_backend
        .cloned().map_or_else(|| "demo".to_string(), |b| b.slug().to_string());
    let instance_id_for_menu = nav.read().active_instance_id.cloned().unwrap_or_default();

    let type_icon = match ch_type {
        ChannelType::Text | ChannelType::Thread | ChannelType::Announcement => "#",
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
        voice_state
            .read()
            .voice_channel_participants
            .get(&ch_id)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Long-press handler for mobile — 500 ms sustained touch opens the same
    // channel context menu as right-click.
    let long_press = {
        let ch_id = ch_id_for_menu.clone();
        let ch_name = ch_name_for_menu.clone();
        let account_id = account_id_for_menu.clone();
        let server_id = server_id_for_menu.clone();
        let instance_id = instance_id_for_menu.clone();
        let backend_slug = backend_slug_for_menu.clone();
        crate::ui::context_menu::long_press::LongPress::default_500ms(move |x, y| {
            ui_overlays.batch(|o| {
                o.context_menu_stack.push(channel_entry_at(
                    ChannelContextMenuState {
                        x,
                        y,
                        channel_id: ch_id.clone(),
                        channel_name: ch_name.clone(),
                        account_id: account_id.clone(),
                        server_id: server_id.clone(),
                        instance_id: instance_id.clone(),
                        backend_slug: backend_slug.clone(),
                    },
                    x,
                    y,
                ));
            });
        })
    };

    rsx! {
        div {
            class: "{channel_class}",
            "data-testid": "channel-row-{ch_id}",
            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
                let coords = evt.client_coordinates();
                ui_overlays.batch(|o| {
                    o.context_menu_stack.push(channel_entry_at(
                        ChannelContextMenuState {
                            x: coords.x,
                            y: coords.y,
                            channel_id: ch_id_for_menu.clone(),
                            channel_name: ch_name_for_menu.clone(),
                            account_id: account_id_for_menu.clone(),
                            server_id: server_id_for_menu.clone(),
                            instance_id: instance_id_for_menu.clone(),
                            backend_slug: backend_slug_for_menu.clone(),
                        },
                        coords.x,
                        coords.y,
                    ));
                });
            },
            ontouchstart: long_press.on_touch_start(),
            ontouchend: long_press.on_touch_end(),
            ontouchmove: long_press.on_touch_move(),
            ontouchcancel: long_press.on_touch_cancel(),
            onclick: move |_| {
                // No-op when re-clicking the channel we're already on. Without this
                // guard a re-click re-runs `load_channel_data` → `get_voice_participants`
                // which overwrites the in-voice participant list with whatever the
                // backend returns (which may not include the local user mid-session)
                // and visibly drops your own avatar from the voice grid even though
                // the WS connection is still live.
                if let Some(prev) = nav.peek().selected_channel.cloned()
                    && prev == ch_id
                {
                    return;
                }
                if let Some(previous_channel_id) = nav.read().selected_channel.cloned()
                {
                    remember_message_list_scroll_position(&previous_channel_id);
                }
                {
                    let ch = channel_for_click.clone();
                    chat_view_state.batch(|cv| cv.current_channel = Some(ch));
                }
                // Clear unread on click. Tells the backend too so the next
                // get_channels refetch doesn't restore the unread count.
                let server_id_for_mark = nav.read().selected_server.cloned();
                crate::ui::account::common::chat_view::mark_channel_as_read_with_backend(
                    chat_lists,
                    chat_view_state,
                    client_manager,
                    None,
                    server_id_for_mark,
                    &ch_id,
                );
                // Persist last visited channel for this server (fire-and-forget).
                let server_id_for_persist = channel.server_id.clone();
                let channel_id_for_persist = ch_id.clone();
                spawn(async move {
                    if let Some(storage) = crate::STORAGE.get() {
                        drop(
                            storage
                                .set_last_channel_for_server(&server_id_for_persist, &channel_id_for_persist)
                                .await,
                        );
                    }
                });
                let cid = ch_id.clone();
                spawn(async move {
                    load_channel_data(cid, client_manager, app_state, nav, voice_state, chat_view_state).await;
                });
                let server_id = nav.read().selected_server.cloned().unwrap_or_default();
                let (backend_slug, instance_id, account_id) = {
                    let nav_snap = nav.read();
                    match (
                        nav_snap.active_backend.cloned(),
                        nav_snap.active_instance_id.cloned(),
                        nav_snap.active_account_id.cloned(),
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

/// Single connected voice participant chip shown below its voice channel row.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn VoiceParticipantEntry(participant: VoiceParticipant) -> Element {
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

/// Single DM channel item.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DMChannelItem(
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
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let ui_layout: crate::state::BatchedSignal<crate::state::UiLayout> = use_context();
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

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
        // Offline / Invisible / Unknown all suppress the dot — Unknown
        // means "backend has no presence info", which is visually closer
        // to "no indicator" than to a deliberate offline state.
        PresenceStatus::Offline | PresenceStatus::Invisible | PresenceStatus::Unknown => "",
    };

    let menu_channel_id = channel_id.clone();
    let menu_user_id = user_id.clone();
    let menu_display_name = display_name.clone();
    let menu_account_id = account_id.clone();
    let menu_instance_id = instance_id.clone();
    let menu_backend_slug = backend_slug.clone();
    let lp_channel_id = menu_channel_id.clone();
    let lp_user_id = menu_user_id.clone();
    let lp_display_name = menu_display_name.clone();
    let lp_account_id = menu_account_id.clone();
    let lp_instance_id = menu_instance_id.clone();
    let lp_backend_slug = menu_backend_slug.clone();
    let dm_long_press = crate::ui::context_menu::long_press::LongPress::default_500ms(
        move |x, y| {
            ui_overlays.batch(|o| {
                o.context_menu_stack.push(dm_entry_at(
                    DmContextMenuState {
                        x,
                        y,
                        channel_id: lp_channel_id.clone(),
                        user_id: lp_user_id.clone(),
                        display_name: lp_display_name.clone(),
                        account_id: lp_account_id.clone(),
                        instance_id: lp_instance_id.clone(),
                        backend_slug: lp_backend_slug.clone(),
                        unread_count: unread,
                    },
                    x,
                    y,
                ));
            });
        },
    );

    rsx! {
        div {
            class: if is_active { "channel-item active" } else { "channel-item" },
            "data-testid": "channel-row-{channel_id}",
            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
                let coords = evt.client_coordinates();
                ui_overlays.batch(|o| {
                    o.context_menu_stack.push(dm_entry_at(
                        DmContextMenuState {
                            x: coords.x,
                            y: coords.y,
                            channel_id: menu_channel_id.clone(),
                            user_id: menu_user_id.clone(),
                            display_name: menu_display_name.clone(),
                            account_id: menu_account_id.clone(),
                            instance_id: menu_instance_id.clone(),
                            backend_slug: menu_backend_slug.clone(),
                            unread_count: unread,
                        },
                        coords.x,
                        coords.y,
                    ));
                });
            },
            ontouchstart: dm_long_press.on_touch_start(),
            ontouchend: dm_long_press.on_touch_end(),
            ontouchmove: dm_long_press.on_touch_move(),
            ontouchcancel: dm_long_press.on_touch_cancel(),
            onclick: move |_| {
                if let Some(previous_channel_id) = nav.read().selected_channel.cloned()
                {
                    remember_message_list_scroll_position(&previous_channel_id); // Clear group member list — this is an individual DM.
                }
                let cur_chan = Channel {
                    id: channel_id.clone(),
                    name: display_name.clone(),
                    channel_type: ChannelType::Text,
                    server_id: String::new(),
                    unread_count: unread,
                    mention_count: 0,
                    last_message_id: None,
                    forum_tags: None,
                    parent_channel_id: None,
                    thread_metadata: None,
                };
                chat_view_state.batch(|cv| {
                    cv.active_group_members = Vec::new();
                    cv.current_channel = Some(cur_chan);
                    cv.current_server = None;
                });
                // Clear unread on click + tell the backend so the next
                // get_dm_channels refetch doesn't restore the badge.
                crate::ui::account::common::chat_view::mark_channel_as_read_with_backend(
                    chat_lists,
                    chat_view_state,
                    client_manager,
                    Some(account_id.clone()),
                    None,
                    &channel_id,
                );
                ui_layout.batch(|l| l.dm_right_sidebar_visible = false);
                let cid = channel_id.clone();
                let aid = account_id.clone();
                spawn(async move {
                    load_dm_messages(cid, aid, client_manager, chat_view_state).await;
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn GroupChannelItem(
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
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: crate::state::BatchedSignal<crate::state::UiOverlays> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    let display_name = group_name.unwrap_or_else(|| {
        members
            .iter()
            .map(|m| m.display_name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let member_count = members.len();

    let menu_channel_id = group_id.clone();
    let menu_display_name = display_name.clone();
    let menu_account_id = account_id.clone();
    let menu_instance_id = instance_id.clone();
    let menu_backend_slug = backend_slug.clone();
    let lp_channel_id = menu_channel_id.clone();
    let lp_display_name = menu_display_name.clone();
    let lp_account_id = menu_account_id.clone();
    let lp_instance_id = menu_instance_id.clone();
    let lp_backend_slug = menu_backend_slug.clone();
    let group_long_press = crate::ui::context_menu::long_press::LongPress::default_500ms(
        move |x, y| {
            ui_overlays.batch(|o| {
                o.context_menu_stack.push(group_dm_entry_at(
                    GroupDmContextMenuState {
                        x,
                        y,
                        channel_id: lp_channel_id.clone(),
                        display_name: lp_display_name.clone(),
                        account_id: lp_account_id.clone(),
                        instance_id: lp_instance_id.clone(),
                        backend_slug: lp_backend_slug.clone(),
                        unread_count: 0,
                    },
                    x,
                    y,
                ));
            });
        },
    );

    rsx! {
        div {
            class: if is_active { "channel-item active" } else { "channel-item" },
            oncontextmenu: move |evt| {
                evt.prevent_default();
                evt.stop_propagation();
                let coords = evt.client_coordinates();
                ui_overlays.batch(|o| {
                    o.context_menu_stack.push(group_dm_entry_at(
                        GroupDmContextMenuState {
                            x: coords.x,
                            y: coords.y,
                            channel_id: menu_channel_id.clone(),
                            display_name: menu_display_name.clone(),
                            account_id: menu_account_id.clone(),
                            instance_id: menu_instance_id.clone(),
                            backend_slug: menu_backend_slug.clone(),
                            unread_count: 0,
                        },
                        coords.x,
                        coords.y,
                    ));
                });
            },
            ontouchstart: group_long_press.on_touch_start(),
            ontouchend: group_long_press.on_touch_end(),
            ontouchmove: group_long_press.on_touch_move(),
            ontouchcancel: group_long_press.on_touch_cancel(),
            onclick: move |_| {
                if let Some(previous_channel_id) = nav.read().selected_channel.cloned()
                {
                    remember_message_list_scroll_position(&previous_channel_id); // Populate group members for the DM member sidebar.
                } // Synthesize a Channel so ChatView can display the group header
                let group_members_clone = members.clone();
                let cur_chan = Channel {
                    id: group_id.clone(),
                    name: display_name.clone(),
                    channel_type: ChannelType::Text,
                    server_id: String::new(),
                    unread_count: 0,
                    mention_count: 0,
                    last_message_id: None,
                    forum_tags: None,
                    parent_channel_id: None,
                    thread_metadata: None,
                };
                chat_view_state.batch(|cv| {
                    cv.active_group_members = group_members_clone;
                    cv.current_channel = Some(cur_chan);
                    cv.current_server = None;
                });
                crate::ui::account::common::chat_view::mark_channel_as_read_with_backend(
                    chat_lists,
                    chat_view_state,
                    client_manager,
                    Some(account_id.clone()),
                    None,
                    &group_id,
                );
                let cid = group_id.clone();
                let aid = account_id.clone();
                spawn(async move {
                    load_dm_messages(cid, aid, client_manager, chat_view_state).await;
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
