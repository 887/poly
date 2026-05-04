//! Typed action dispatch for [`super::chat_view_state::ChatViewState`] mutations.
//!
//! Each variant encodes a *named* mutation pattern that appears in multiple
//! call sites. Only patterns that recur across ≥2 files are given a variant —
//! one-off mutations stay as direct field writes inside `.batch()` closures.
//!
//! ## Usage
//!
//! ```ignore
//! // Instead of:
//! chat_view.batch(|cv| {
//!     cv.current_server = None;
//!     cv.current_channel = None;
//!     cv.messages.clear();
//!     cv.members.clear();
//! });
//! // … and separately:
//! chat_lists.batch(|cl| cl.set_channels(vec![]));
//!
//! // Write:
//! chat_view.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
//! chat_lists.batch(|cl| cl.set_channels(vec![]));
//!
//! // Or, when mixing with extra mutations in the same batch:
//! chat_view.batch(|cv| {
//!     cv.apply(ChatAction::ClearChannelContext);
//!     cv.current_server = Some(server.clone());
//! });
//! ```
//!
//! ## Note on ClearChannelContext
//!
//! `ClearChannelContext` clears *view-state* fields only (current_server,
//! current_channel, messages, members). Callers must ALSO clear
//! `chat_lists.set_channels(vec![])` in a separate batch on `ChatLists`.
//! This two-call pattern replaces the old single `chat_data.batch(|cd| cd.apply(...))`.
//!
//! ## Adding variants
//!
//! Only add a variant when the pattern recurs across **multiple files**.
//! Wrapping a single-site mutation in an action adds indirection without
//! reducing duplication.

/// A named mutation on [`super::chat_view_state::ChatViewState`].
///
/// Apply via [`super::chat_view_state::ChatViewState::apply`] inside a `.batch()` closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
    /// Clear the full server + channel context (view-state fields only).
    ///
    /// Sets `current_server` and `current_channel` to `None` and clears
    /// `messages` and `members`. Use when navigating away from a server
    /// to a top-level route (Overview, DMs, Notifications, Friends, Discover),
    /// or when switching servers.
    ///
    /// **Important:** this only clears view-state fields. Callers must also
    /// call `chat_lists.batch(|cl| cl.set_channels(vec![]))` separately.
    ///
    /// Canonical pattern repeated across `account_server_bar.rs`,
    /// `favorites_sidebar.rs`, and `demo.rs`.
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
