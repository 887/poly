//! `WritableMessagingBackend` capability sub-trait
//! (`plan-trait-split-readable-vs-writable.md`, Phase B.1).
//!
//! Carved out of [`IsBackend`] to give read-only backends
//! (`poly-forgejo`, future read-only feeds) a way to NOT declare a
//! `send_message` method at all, instead of inheriting the parent
//! trait's `Err(NotSupported)` default or stubbing one out manually.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(wm) = backend.as_writable_messaging() {
//!     wm.send_message(&channel_id, content).await?;
//! } else {
//!     // backend is read-only — surface a "this chat doesn't accept
//!     // new messages" affordance in the UI instead of trying to send
//! }
//! ```
//!
//! The legacy [`IsBackend::send_message`] method remains as a
//! default-delegating shim, so existing call sites in `crates/core/`
//! and `mcp/chat-mcp/` continue to compile unchanged — the default
//! impl consults `as_writable_messaging()` and delegates if `Some`,
//! else returns `Err(NotSupported)`.
//!
//! [`IsBackend`]: crate::IsBackend
//! [`IsBackend::send_message`]: crate::IsBackend::send_message

use async_trait::async_trait;

use crate::{ClientResult, Message, MessageContent};

/// Capability sub-trait for backends that accept new outbound messages.
///
/// No default impls: presence of `impl WritableMessagingBackend` is the
/// opt-in signal.  Read-only backends (news feeds, forge indexes)
/// leave [`IsBackend::as_writable_messaging`] returning `None` (the
/// default) and the host treats `send_message` as unsupported.
///
/// # Liskov contract
///
/// `send_message` MUST obey the same contract the method had when it
/// lived directly on [`IsBackend`]:
///
/// * Returns `Ok(Message)` echoing what the backend persisted, with
///   any backend-assigned ID/timestamp filled in.
/// * May fail with [`ClientError::Network`], [`ClientError::Auth`], or
///   a backend-specific [`ClientError::NotSupported`] explaining why
///   *this particular channel kind* refuses writes (e.g. GitHub
///   forum-index channels).
/// * Must not panic.
///
/// [`IsBackend`]: crate::IsBackend
/// [`IsBackend::as_writable_messaging`]: crate::IsBackend::as_writable_messaging
/// [`ClientError::Network`]: crate::ClientError::Network
/// [`ClientError::Auth`]: crate::ClientError::Auth
/// [`ClientError::NotSupported`]: crate::ClientError::NotSupported
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WritableMessagingBackend: Send + Sync {
    /// Send a message to a channel.
    ///
    /// Backends with channel-kind-specific write restrictions (e.g.
    /// GitHub forum indexes that require the web UI) may still return
    /// `Err(NotSupported)` for those specific channels — the opt-in
    /// is "this backend supports writing to *at least some* channels",
    /// not "every channel accepts writes".
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message>;
}
