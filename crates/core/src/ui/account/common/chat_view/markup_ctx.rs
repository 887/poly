//! `ChatViewMarkupCtx` — the render-time context bundle for the chat view.
//!
//! Built once per render by `build_chat_view_markup_ctx` from the live signal
//! values in `ChatViewSignals`.  Passed by value (Clone) through the render
//! helper call chain so each function receives only what it needs via field
//! destructuring.

use dioxus::prelude::*;
use crate::state::BatchedSignal;
use crate::state::{ChatLists, ChatViewState, NavState, UiLayout, UiOverlays, VoiceState};
use crate::client_manager::ClientManager;
use super::super::chat_history::ChatHistoryUiState;
use super::composer_helpers::PendingAttachmentPreview;
use super::search_filter::{
    SearchFilterOption,
    build_search_filter_options, filter_search_filter_options,
    contextual_search_placeholder, message_search_terms,
};
use super::composer_helpers::contextual_compose_placeholder;
use super::virtualization::MessageVirtualWindowState;
use super::MsgContextMenu;
use super::ChatUtilityPanel;
use super::signals::ChatViewSignals;
use poly_client::{
    Channel, ChatCommand, Message, MessageReplyPreview, MessageSearchHit,
    PresenceStatus, User,
};

#[derive(Clone)]
pub(super) struct ChatViewMarkupCtx {
    pub(super) nav: BatchedSignal<NavState>,
    pub(super) ui_layout: BatchedSignal<UiLayout>,
    pub(super) ui_overlays: BatchedSignal<UiOverlays>,
    pub(super) client_manager: BatchedSignal<ClientManager>,
    pub(super) chat_lists: BatchedSignal<ChatLists>,
    pub(super) chat_view_state: BatchedSignal<ChatViewState>,
    pub(super) voice_state: BatchedSignal<VoiceState>,
    pub(super) channel_id: Option<String>,
    pub(super) messages: Vec<Message>,
    pub(super) current_channel: Option<Channel>,
    pub(super) current_server: Option<poly_client::Server>,
    pub(super) loading: bool,
    pub(super) reaction_picker_id: Option<String>,
    pub(super) group_members: Vec<poly_client::User>,
    pub(super) search_query_input_value: String,
    pub(super) search_query_value: String,
    pub(super) is_dm_channel: bool,
    pub(super) is_group_channel: bool,
    pub(super) member_list_visible: bool,
    pub(super) search_terms: Vec<String>,
    pub(super) search_placeholder: String,
    pub(super) compose_placeholder: String,
    pub(super) search_filter_channel_name_onfocus: String,
    pub(super) search_filter_channel_name_oninput: String,
    pub(super) filtered_search_filter_options: Vec<SearchFilterOption>,
    pub(super) unread_marker_id: Option<String>,
    pub(super) unread_banner_visible: bool,
    pub(super) unread_banner_count: String,
    pub(super) unread_banner_time: String,
    pub(super) unread_banner_date: String,
    pub(super) unread_banner_channel_id: Option<String>,
    pub(super) self_user_id: String,
    pub(super) dm_user: Option<User>,
    pub(super) dm_user_avatar: Option<String>,
    pub(super) dm_user_presence: PresenceStatus,
    pub(super) search_hit_channel_id: Option<String>,
    pub(super) pinned_hit_channel_id: Option<String>,
    pub(super) search_hit_server: Option<poly_client::Server>,
    pub(super) pinned_hit_server: Option<poly_client::Server>,
    pub(super) pinned_hit_channel: Option<Channel>,
    pub(super) nav_for_search: crate::ui::dioxus_router::Navigator,
    pub(super) nav_for_pinned: crate::ui::dioxus_router::Navigator,
    pub(super) message_input: Signal<String>,
    pub(super) show_input_emoji: Signal<bool>,
    pub(super) markdown_enabled: Signal<bool>,
    pub(super) reaction_picker_msg: Signal<Option<String>>,
    pub(super) drag_over: Signal<bool>,
    pub(super) editing_msg_id: Signal<Option<String>>,
    pub(super) edit_draft: Signal<String>,
    pub(super) msg_context_menu: Signal<Option<MsgContextMenu>>,
    pub(super) utility_panel: Signal<Option<ChatUtilityPanel>>,
    pub(super) search_query: Signal<String>,
    pub(super) search_hits: Signal<Vec<MessageSearchHit>>,
    pub(super) pinned_messages: Signal<Vec<Message>>,
    pub(super) notifications_muted: Signal<bool>,
    pub(super) show_search_filters: Signal<bool>,
    pub(super) active_search_filter_idx: Signal<usize>,
    pub(super) pending_attachments: Signal<Vec<PendingAttachmentPreview>>,
    pub(super) command_suggestions: Signal<Vec<ChatCommand>>,
    pub(super) active_command_idx: Signal<usize>,
    pub(super) show_command_popup: Signal<bool>,
    pub(super) reply_target: Signal<Option<MessageReplyPreview>>,
    pub(super) history_state: BatchedSignal<ChatHistoryUiState>,
    pub(super) unread_marker_on_screen: Signal<bool>,
    pub(super) virtual_window: Signal<MessageVirtualWindowState>,
    pub(super) header_actions_overflow: Signal<bool>,
    pub(super) header_actions_menu_open: Signal<bool>,
    /// Whether the filter/search box is open inside the Pinned tab
    pub(super) pinned_filter_open: Signal<bool>,
    /// Current filter query text for the Pinned tab
    pub(super) pinned_filter_query: Signal<String>,
    /// Whether the filter/search box is open inside the Threads tab
    pub(super) threads_filter_open: Signal<bool>,
    /// Current filter query text for the Threads tab
    pub(super) threads_filter_query: Signal<String>,
    /// Whether the user has scrolled far enough from the live tail for "Jump to Present".
    pub(super) scrolled_from_bottom: Signal<bool>,
    /// Count of live messages that arrived while the user was scrolled up.
    pub(super) new_messages_while_scrolled_up: Signal<u32>,
    /// Resize-driven rerender tick — forwarded to ChatHeaderActions for overflow detection.
    pub(super) mobile_layout_resize_tick: Signal<u64>,
}

// lint-allow-unused: long cohesive view/handler; splitting risks reactive bugs
#[allow(clippy::too_many_lines)]
pub(super) fn build_chat_view_markup_ctx(signals: &ChatViewSignals) -> ChatViewMarkupCtx {
    let nav_signal = signals.nav;
    let ui_layout = signals.ui_layout;
    let ui_overlays = signals.ui_overlays;
    let client_manager = signals.client_manager;
    let chat_lists = signals.chat_lists;
    let chat_view_state = signals.chat_view_state;
    let voice_state = signals.voice_state;
    let nav = navigator();
    let channel_id = nav_signal.read().selected_channel.cloned();
    let messages = chat_view_state.read().messages.clone();
    let current_channel = chat_view_state.read().current_channel.clone();
    let current_server = chat_view_state.read().current_server.clone();
    let loading = chat_view_state.read().loading;
    let reaction_picker_id = signals.reaction_picker_msg.read().clone();
    let group_members = chat_view_state.read().active_group_members.clone();
    let search_query_input_value = signals.search_query.read().clone();
    let search_query_value = search_query_input_value.trim().to_string();
    let current_channel_name = current_channel
        .as_ref()
        .map(|channel| channel.name.clone())
        .unwrap_or_default();
    let search_filter_options = build_search_filter_options(&current_channel_name);
    let filtered_search_filter_options =
        filter_search_filter_options(&search_filter_options, &search_query_input_value);
    let is_dm_channel = channel_id.as_deref().unwrap_or_default().starts_with("dm-");
    let is_group_channel = channel_id
        .as_deref()
        .unwrap_or_default()
        .starts_with("group-");
    let member_list_visible = if is_dm_channel || is_group_channel {
        ui_layout.read().dm_right_sidebar_visible
    } else {
        ui_layout.read().right_sidebar_visible
    };
    let (
        unread_marker_id,
        unread_banner_visible,
        unread_banner_count,
        unread_banner_time,
        unread_banner_date,
    ) = build_unread_banner_fields(signals.history_state, &messages);

    ChatViewMarkupCtx {
        nav: nav_signal,
        ui_layout,
        ui_overlays,
        client_manager,
        chat_lists,
        chat_view_state,
        voice_state,
        channel_id: channel_id.clone(),
        messages,
        current_channel: current_channel.clone(),
        current_server: current_server.clone(),
        loading,
        reaction_picker_id,
        group_members,
        search_query_input_value,
        search_query_value: search_query_value.clone(),
        is_dm_channel,
        is_group_channel,
        member_list_visible,
        search_terms: message_search_terms(&search_query_value),
        search_placeholder: contextual_search_placeholder(
            current_channel.as_ref(),
            is_dm_channel,
            is_group_channel,
        ),
        compose_placeholder: contextual_compose_placeholder(
            current_channel.as_ref(),
            is_dm_channel,
            is_group_channel,
        ),
        search_filter_channel_name_onfocus: current_channel_name.clone(),
        search_filter_channel_name_oninput: current_channel_name,
        filtered_search_filter_options,
        unread_marker_id,
        unread_banner_visible,
        unread_banner_count,
        unread_banner_time,
        unread_banner_date,
        unread_banner_channel_id: channel_id.clone(),
        self_user_id: current_self_user_id(nav_signal, client_manager),
        dm_user: current_dm_user(chat_lists, &channel_id, is_dm_channel),
        dm_user_avatar: current_dm_user_avatar(chat_lists, &channel_id, is_dm_channel),
        dm_user_presence: current_dm_user_presence(chat_lists, &channel_id, is_dm_channel),
        search_hit_channel_id: channel_id.clone(),
        pinned_hit_channel_id: channel_id,
        search_hit_server: current_server.clone(),
        pinned_hit_server: current_server.clone(),
        pinned_hit_channel: current_channel,
        nav_for_search: nav,
        nav_for_pinned: nav,
        message_input: signals.message_input,
        show_input_emoji: signals.show_input_emoji,
        markdown_enabled: signals.markdown_enabled,
        reaction_picker_msg: signals.reaction_picker_msg,
        drag_over: signals.drag_over,
        editing_msg_id: signals.editing_msg_id,
        edit_draft: signals.edit_draft,
        msg_context_menu: signals.msg_context_menu,
        utility_panel: signals.utility_panel,
        search_query: signals.search_query,
        search_hits: signals.search_hits,
        pinned_messages: signals.pinned_messages,
        notifications_muted: signals.notifications_muted,
        show_search_filters: signals.show_search_filters,
        active_search_filter_idx: signals.active_search_filter_idx,
        pending_attachments: signals.pending_attachments,
        command_suggestions: signals.command_suggestions,
        active_command_idx: signals.active_command_idx,
        show_command_popup: signals.show_command_popup,
        reply_target: signals.reply_target,
        history_state: signals.history_state,
        unread_marker_on_screen: signals.unread_marker_on_screen,
        virtual_window: signals.virtual_window,
        header_actions_overflow: signals.header_actions_overflow,
        header_actions_menu_open: signals.header_actions_menu_open,
        pinned_filter_open: signals.pinned_filter_open,
        pinned_filter_query: signals.pinned_filter_query,
        threads_filter_open: signals.threads_filter_open,
        threads_filter_query: signals.threads_filter_query,
        scrolled_from_bottom: signals.scrolled_from_bottom,
        new_messages_while_scrolled_up: signals.new_messages_while_scrolled_up,
        mobile_layout_resize_tick: signals.mobile_layout_resize_tick,
    }
}

fn build_unread_banner_fields(
    history_state: BatchedSignal<ChatHistoryUiState>,
    messages: &[Message],
) -> (Option<String>, bool, String, String, String) {
    let unread_marker_id = history_state.read().unread_marker_message_id.clone();
    let unread_count = history_state.read().unread_count;
    let unread_banner_time = unread_banner_timestamp(messages, unread_marker_id.as_deref())
        .map(|timestamp| timestamp.format("%H:%M").to_string())
        .unwrap_or_default();
    let unread_banner_date = unread_banner_timestamp(messages, unread_marker_id.as_deref())
        .map(|timestamp| timestamp.format("%-d %B %Y").to_string())
        .unwrap_or_default();

    (
        unread_marker_id,
        unread_count > 0,
        display_unread_count(unread_count),
        unread_banner_time,
        unread_banner_date,
    )
}

fn unread_banner_timestamp<'a>(
    messages: &'a [Message],
    marker_message_id: Option<&str>,
) -> Option<&'a chrono::DateTime<chrono::Utc>> {
    let marker_message_id = marker_message_id?;
    messages
        .iter()
        .find(|message| message.id == marker_message_id)
        .map(|message| &message.timestamp)
}

fn display_unread_count(unread_count: u32) -> String {
    if unread_count > 9 {
        return format!("{unread_count}+");
    }
    unread_count.to_string()
}

fn current_self_user_id(
    nav: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
) -> String {
    let state = nav.read();
    let cm = client_manager.read();
    state
        .active_account_id
        .as_deref()
        .and_then(|aid| cm.sessions.get(aid))
        .map(|session| session.user.id.clone())
        .unwrap_or_default()
}

fn current_dm_user_avatar(
    chat_lists: BatchedSignal<ChatLists>,
    channel_id: &Option<String>,
    is_dm_channel: bool,
) -> Option<String> {
    if !is_dm_channel {
        return None;
    }

    let cid = channel_id.clone().unwrap_or_default();
    chat_lists
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.id == cid)
        .and_then(|dm| dm.user.avatar_url.clone())
}

fn current_dm_user(
    chat_lists: BatchedSignal<ChatLists>,
    channel_id: &Option<String>,
    is_dm_channel: bool,
) -> Option<User> {
    if !is_dm_channel {
        return None;
    }

    let cid = channel_id.clone().unwrap_or_default();
    chat_lists
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.id == cid)
        .map(|dm| dm.user.clone())
}

fn current_dm_user_presence(
    chat_lists: BatchedSignal<ChatLists>,
    channel_id: &Option<String>,
    is_dm_channel: bool,
) -> PresenceStatus {
    if !is_dm_channel {
        return PresenceStatus::Offline;
    }

    let cid = channel_id.clone().unwrap_or_default();
    chat_lists
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.id == cid)
        .map_or(PresenceStatus::Offline, |dm| dm.user.presence)
}
