//! `ChatViewState` — reactive store for the active channel view.
//!
//! Holds the currently-loaded data for whichever server+channel the
//! user is looking at: messages, members, the channel / server info
//! rendered in headers, and transient UX state (loading spinner,
//! typing indicator, anchor-restore flag).
//!
//! Provided as `BatchedSignal<ChatViewState>` at the `App` level
//! (Phase G.6 of plan-solid-refactor-survey.md).
//!
//! ## By-id shadow
//!
//! `messages_by_id` is an index map into `messages`. Always use
//! `set_messages` / `push_message` instead of writing into `messages`
//! directly.

use poly_client::{Channel, Message, Server, User};
use std::collections::HashMap;

use super::chat_actions::ChatAction;

/// Reactive store for the active channel/server view state.
///
/// Components that only need message/member data subscribe to this
/// signal and are not re-rendered when the server list or account
/// settings change.
#[derive(Debug, Clone, Default)]
pub struct ChatViewState {
    /// Messages for the currently selected channel.
    pub messages: Vec<Message>,
    /// Members of the currently selected channel.
    pub members: Vec<User>,
    /// Currently selected server info (for channel list header).
    pub current_server: Option<Server>,
    /// Currently selected channel info (for chat header).
    pub current_channel: Option<Channel>,
    /// F-DC-1 — Permission-denied error for the currently selected channel.
    ///
    /// Set when a channel load fails with `ClientError::PermissionDenied`.
    /// Cleared when the channel changes. Drives a styled permission-denied
    /// empty state in `render_message_list_content` instead of the generic
    /// "no messages" wave.
    pub channel_load_error: Option<String>,
    /// Users currently typing in the selected channel.
    ///
    /// Each entry is a display name string. Updated by the event stream
    /// consumer when `TypingStarted` events arrive, cleared after a
    /// few-second timeout.
    pub typing_users: Vec<String>,
    /// Members of the currently open group DM.
    ///
    /// Populated from the `Group::members` list when a group conversation
    /// is opened. Empty for individual DMs and server channels.
    /// Used by `DmUserSidebar` to render the group member list.
    pub active_group_members: Vec<User>,
    /// Set when the most recent channel message load used `MessageQuery::around`
    /// (anchor restore). Tells `use_history_state_effect` to set `has_more_after = true`
    /// so the bottom sentinel and "Jump to Present" will chain-load newer messages.
    /// Reset to `false` after `use_history_state_effect` consumes it.
    pub messages_loaded_via_anchor: bool,
    /// Whether data is currently loading.
    pub loading: bool,

    // --- by-id shadow (filled atomically with `messages`) ---
    /// Index shadow for `messages` — maps `message.id` → index in `self.messages`.
    pub messages_by_id: HashMap<String, usize>,
}

impl ChatViewState {
    // -------------------------------------------------------------------------
    // Invariant-preserving setters for messages
    // -------------------------------------------------------------------------

    /// Replace the entire messages list and rebuild the by-id index atomically.
    pub fn set_messages(&mut self, messages: Vec<Message>) {
        self.messages_by_id = messages
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.clone(), i))
            .collect();
        self.messages = messages;
    }

    /// Append one message and update the by-id index.
    pub fn push_message(&mut self, message: Message) {
        self.messages_by_id
            .insert(message.id.clone(), self.messages.len());
        self.messages.push(message);
    }

    /// Look up a message by its ID in O(1).
    #[must_use]
    pub fn message_by_id(&self, id: &str) -> Option<&Message> {
        self.messages_by_id
            .get(id)
            .and_then(|i| self.messages.get(*i))
    }

    // -------------------------------------------------------------------------
    // Named action dispatch (mirrors ChatAction — now on ChatViewState)
    // -------------------------------------------------------------------------

    /// Apply a typed [`ChatAction`] to this `ChatViewState`.
    ///
    /// Call inside a `.batch()` closure:
    /// ```ignore
    /// chat_view.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
    /// ```
    // lint-allow-unused: ChatAction is consumed by the match; by-value is correct
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply(&mut self, action: ChatAction) {
        match action {
            ChatAction::ClearChannelContext => {
                self.current_server = None;
                self.current_channel = None;
                self.messages.clear();
                self.messages_by_id.clear();
                self.members.clear();
                // channels lives in ChatLists — clear that separately
            }
            ChatAction::ClearActiveChannel => {
                self.current_channel = None;
                self.messages.clear();
                self.messages_by_id.clear();
                self.members.clear();
            }
        }
    }
}
