//! `ChatViewSignals` — the signal bundle for the chat view component.
//!
//! One struct that owns every `Signal<T>` / `BatchedSignal<T>` wired up
//! inside `render_chat_view`.  The constructor (`use_chat_view_signals`)
//! must be called from inside a `#[component]`-scoped frame because it
//! calls `use_signal` / `use_context` / `BatchedSignal::use_batched`.

use dioxus::prelude::*;
use crate::state::BatchedSignal;
use crate::state::{AccountSessions, ChatLists, ChatViewState, NavState, UiLayout, UiOverlays, VoiceState};
use crate::client_manager::ClientManager;
use super::super::chat_history::ChatHistoryUiState;
use super::composer_helpers::PendingAttachmentPreview;
use super::virtualization::MessageVirtualWindowState;
use super::MsgContextMenu;
use super::ChatUtilityPanel;
use poly_client::{ChatCommand, Message, MessageReplyPreview, MessageSearchHit};

pub(super) struct ChatViewSignals {
    pub(super) nav: BatchedSignal<NavState>,
    pub(super) ui_layout: BatchedSignal<UiLayout>,
    pub(super) ui_overlays: BatchedSignal<UiOverlays>,
    pub(super) client_manager: BatchedSignal<ClientManager>,
    pub(super) chat_lists: BatchedSignal<ChatLists>,
    pub(super) chat_view_state: BatchedSignal<ChatViewState>,
    pub(super) account_sessions: BatchedSignal<AccountSessions>,
    pub(super) voice_state: BatchedSignal<VoiceState>,
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
    /// Resize-driven rerender tick so desktop/mobile header branches flip immediately.
    pub(super) mobile_layout_resize_tick: Signal<u64>,
    /// Whether the user has scrolled far enough from the live tail that the
    /// "Jump to Present" button should be shown.
    pub(super) scrolled_from_bottom: Signal<bool>,
    /// Count of live messages that arrived while the user was scrolled up.
    /// Shown as a badge on the "Jump to Present" button.
    pub(super) new_messages_while_scrolled_up: Signal<u32>,
}

pub(super) fn use_chat_view_signals() -> ChatViewSignals {
    ChatViewSignals {
        nav: use_context(),
        ui_layout: use_context(),
        ui_overlays: use_context(),
        client_manager: use_context(),
        chat_lists: use_context(),
        chat_view_state: use_context(),
        account_sessions: use_context(),
        voice_state: use_context(),
        message_input: use_signal(String::new),
        show_input_emoji: use_signal(|| false),
        markdown_enabled: use_signal(|| false),
        reaction_picker_msg: use_signal(|| None::<String>),
        drag_over: use_signal(|| false),
        editing_msg_id: use_signal(|| None::<String>),
        edit_draft: use_signal(String::new),
        msg_context_menu: use_signal(|| None::<MsgContextMenu>),
        utility_panel: use_signal(|| None::<ChatUtilityPanel>),
        search_query: use_signal(String::new),
        search_hits: use_signal(Vec::<MessageSearchHit>::new),
        pinned_messages: use_signal(Vec::<Message>::new),
        notifications_muted: use_signal(|| false),
        show_search_filters: use_signal(|| false),
        active_search_filter_idx: use_signal(|| 0_usize),
        pending_attachments: use_signal(Vec::<PendingAttachmentPreview>::new),
        command_suggestions: use_signal(Vec::<ChatCommand>::new),
        active_command_idx: use_signal(|| 0_usize),
        show_command_popup: use_signal(|| false),
        reply_target: use_signal(|| None::<MessageReplyPreview>),
        history_state: BatchedSignal::use_batched(ChatHistoryUiState::default),
        unread_marker_on_screen: use_signal(|| false),
        virtual_window: use_signal(MessageVirtualWindowState::default),
        pinned_filter_open: use_signal(|| false),
        pinned_filter_query: use_signal(String::new),
        threads_filter_open: use_signal(|| false),
        threads_filter_query: use_signal(String::new),
        mobile_layout_resize_tick: use_signal(|| 0_u64),
        header_actions_overflow: use_signal(|| false),
        header_actions_menu_open: use_signal(|| false),
        scrolled_from_bottom: use_signal(|| false),
        new_messages_while_scrolled_up: use_signal(|| 0_u32),
    }
}
