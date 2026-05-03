//! Slash command types.

use serde::{Deserialize, Serialize};

/// The scope in which a slash command is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandScope {
    /// Available everywhere — any channel, DM, and group DM.
    Global,
    /// Available in server text channels only (not DMs).
    Channel,
    /// Available in DMs and group DMs only.
    DirectMessage,
}

/// A slash command available in a channel.
///
/// Returned by [`crate::ClientBackend::get_channel_commands`] to populate the `/`
/// autocomplete popup in the composer. Built-in Poly commands are added by the
/// UI layer; backend- or bot-provided commands are injected by each client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCommand {
    /// Command name without the leading `/` (e.g. `"shrug"`).
    pub name: String,
    /// Short description shown in the autocomplete popup.
    pub description: String,
    /// Display name of the app or bot providing this command
    /// (e.g. `"Built-in"`, `"MusicCat"`, `"ModBot"`).
    pub provider: String,
    /// Whether this is a Poly built-in command (shown in a separate section).
    pub is_builtin: bool,
    /// Optional usage hint shown after the command name (e.g. `"<song URL>"`).
    pub usage: Option<String>,
    /// Scope in which this command is available.
    pub scope: CommandScope,
}
