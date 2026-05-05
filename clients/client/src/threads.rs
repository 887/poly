//! `ThreadsBackend` capability sub-trait (Phase H.2.c).
//!
//! Carved out of [`ClientBackend`] in Phase H.2.c.  Implemented by backends
//! that expose Discord-style thread channels (`ChannelType::Thread`): currently
//! `poly-discord` and `poly-plugin-host` (via WIT bridge).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(tb) = backend.as_threads() {
//!     let threads = tb.get_active_threads(&server_id).await?;
//!     // …
//! }
//! ```
//!
//! WIT interface: `poly:messenger/messenger-client` — `get-active-threads`
//! and `get-archived-threads` functions.
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;

use crate::{ClientResult, ThreadInfo};

/// Capability sub-trait for Discord-style thread operations.
///
/// Mirrors the `get-active-threads` / `get-archived-threads` functions from
/// the `poly:messenger/messenger-client` WIT interface.
///
/// No default impls: presence of `impl ThreadsBackend` is the opt-in signal.
/// Backends that do not support threads leave [`IsBackend::as_threads`]
/// returning `None` (the default).
///
/// [`IsBackend::as_threads`]: crate::IsBackend::as_threads
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ThreadsBackend: Send + Sync {
    /// Get all active (non-archived) threads in a server.
    async fn get_active_threads(&self, server_id: &str) -> ClientResult<Vec<ThreadInfo>>;

    /// Get archived threads for a parent channel (text or forum).
    ///
    /// `limit` caps the number returned; `None` uses the backend default.
    async fn get_archived_threads(
        &self,
        parent_channel_id: &str,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ThreadInfo>>;
}
