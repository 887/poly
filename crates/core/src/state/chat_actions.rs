//! Typed action dispatch for [`super::chat_data::ChatData`] mutations.
//!
//! Each variant encodes a *named* mutation pattern that appears in multiple
//! call sites. Only patterns that recur across ≥2 files are given a variant —
//! one-off mutations stay as direct field writes inside `.batch()` closures.
//!
//! ## Usage
//!
//! ```ignore
//! // Instead of:
//! chat_data.batch(|cd| {
//!     cd.current_server = None;
//!     cd.current_channel = None;
//!     cd.channels.clear();
//!     cd.messages.clear();
//!     cd.members.clear();
//! });
//!
//! // Write:
//! chat_data.batch(|cd| cd.apply(ChatAction::ClearChannelContext));
//!
//! // Or, when mixing with extra mutations in the same batch:
//! chat_data.batch(|cd| {
//!     cd.apply(ChatAction::ClearChannelContext);
//!     cd.current_server = Some(server.clone());
//! });
//! ```
//!
//! ## Adding variants
//!
//! Only add a variant when the pattern recurs across **multiple files**.
//! Wrapping a single-site mutation in an action adds indirection without
//! reducing duplication.

use super::chat_data::ChatData;

/// A named mutation on [`ChatData`].
///
/// Apply via [`ChatData::apply`] inside a `.batch()` closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
    /// Clear the full server + channel context.
    ///
    /// Sets `current_server` and `current_channel` to `None` and clears
    /// `channels`, `messages`, and `members`. Use when navigating away
    /// from a server to a top-level route (Overview, DMs, Notifications,
    /// Friends, Discover), or when switching servers.
    ///
    /// Canonical 5-field pattern repeated across `account_server_bar.rs`
    /// (5 buttons), `favorites_sidebar.rs` (account-switch click),
    /// and `demo.rs` (demo account teardown).
    ClearChannelContext,

    /// Clear only the active channel's loaded data.
    ///
    /// Sets `current_channel` to `None` and clears `messages` and `members`.
    /// Use when staying on the same server but clearing the right-pane state
    /// (e.g. when loading server shell data without auto-selecting a channel).
    ///
    /// Pattern found in `favorites_sidebar.rs::load_server_data_internal`
    /// (the `!auto_select_first_text_channel` branch).
    ClearActiveChannel,
}

impl ChatData {
    /// Apply a typed [`ChatAction`] to this `ChatData`.
    ///
    /// Call inside a `.batch()` closure:
    /// ```ignore
    /// chat_data.batch(|cd| cd.apply(ChatAction::ClearChannelContext));
    /// ```
    pub fn apply(&mut self, action: ChatAction) {
        match action {
            ChatAction::ClearChannelContext => {
                self.current_server = None;
                self.current_channel = None;
                self.channels.clear();
                self.messages.clear();
                self.members.clear();
            }
            ChatAction::ClearActiveChannel => {
                self.current_channel = None;
                self.messages.clear();
                self.members.clear();
            }
        }
    }
}
