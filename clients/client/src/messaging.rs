//! `MessagingBackend` capability sub-trait (Phase H.4.a).
//!
//! Carved out of [`ClientBackend`] in Phase H.4.a.  Groups the messaging
//! capabilities that are optional on some backends: typing indicators, reply
//! threading, message search, pin management, and composer extras (commands,
//! emojis, stickers).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(mb) = backend.as_messaging() {
//!     mb.send_typing(&channel_id).await?;
//! }
//! ```
//!
//! WIT note: these methods are all declared in the single
//! `poly:messenger/messenger-client` WIT interface.  The Rust sub-trait
//! mirrors the optional-by-default subset: all backends are free to leave
//! [`IsBackend::as_messaging`] returning `None` (the default) and the host
//! will hide or grey-out the corresponding UI affordances.
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`IsBackend::as_messaging`]: crate::IsBackend::as_messaging

use async_trait::async_trait;

use crate::{
    ChatCommand, ClientResult, CustomEmoji, Message, MessageSearchHit,
    MessageSearchQuery, StickerItem,
};

/// Capability sub-trait for messaging extras.
///
/// No default impls: presence of `impl MessagingBackend` is the opt-in signal.
/// Backends that support none of these features leave
/// [`IsBackend::as_messaging`] returning `None` (the default).
///
/// [`IsBackend::as_messaging`]: crate::IsBackend::as_messaging
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait MessagingBackend: Send + Sync {
    /// Broadcast a typing indicator for the given channel.
    ///
    /// Fire-and-forget — callers should not block on the result.
    async fn send_typing(&self, channel_id: &str) -> ClientResult<()>;

    /// Send a reply to an existing message.
    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: crate::MessageContent,
    ) -> ClientResult<Message>;

    /// Search messages using the backend's native search implementation.
    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>>;

    /// Get pinned messages for a channel.
    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>>;

    /// Pin or unpin a message in a channel.
    async fn set_message_pinned(
        &self,
        channel_id: &str,
        message_id: &str,
        pinned: bool,
    ) -> ClientResult<()>;

    /// Get slash commands available in a channel.
    ///
    /// Returns app/bot-provided commands valid for `channel_id`. The UI layer
    /// prepends built-in Poly commands (shrug, me, tableflip, …) before showing
    /// the autocomplete popup, so backends do not need to include those.
    ///
    /// Backends that do not support slash commands return an empty list.
    async fn get_channel_commands(&self, channel_id: &str) -> ClientResult<Vec<ChatCommand>>;

    /// Get the custom emoji usable in a channel.
    async fn get_available_emojis(&self, channel_id: &str) -> ClientResult<Vec<CustomEmoji>>;

    /// Get the stickers usable in a channel.
    async fn get_available_stickers(&self, channel_id: &str) -> ClientResult<Vec<StickerItem>>;
}
