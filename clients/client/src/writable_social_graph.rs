//! `WritableSocialGraphBackend` capability sub-trait
//! (Tier 2 of `plan-trait-split-readable-vs-writable.md`).
//!
//! Carved out of [`SocialGraphBackend`] to give read-only backends
//! (`poly-forgejo`, `poly-github`, `poly-hackernews`, `poly-lemmy`) a
//! way to NOT declare friend/block/ignore/presence-set methods at all,
//! instead of stubbing each one with `Err(NotSupported)`.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(wsg) = backend.as_writable_social_graph() {
//!     wsg.add_friend(user_id).await?;
//! }
//! ```
//!
//! Legacy [`SocialGraphBackend`] write methods (`add_friend`, …) remain
//! as default-delegating shims that consult
//! [`SocialGraphBackend::as_writable_social_graph`] and forward when
//! `Some`, else return `Err(NotSupported)`. Existing call sites in
//! `crates/core/` (e.g. `dm_context_menu.rs`) continue to compile
//! unchanged.
//!
//! [`SocialGraphBackend`]: crate::SocialGraphBackend
//! [`SocialGraphBackend::as_writable_social_graph`]: crate::SocialGraphBackend::as_writable_social_graph

use async_trait::async_trait;

use crate::{ClientResult, PresenceStatus};

/// Capability sub-trait for backends that mutate the social graph
/// (friend list, blocks, ignores, presence).
///
/// No default impls: presence of `impl WritableSocialGraphBackend` is
/// the opt-in signal. Read-only backends (forge indexes, news feeds)
/// leave [`SocialGraphBackend::as_writable_social_graph`] returning
/// `None` and the host treats every write as unsupported.
///
/// # Liskov contract
///
/// Each method MUST obey the same contract the matching method had when
/// it lived directly on [`SocialGraphBackend`]:
///
/// * Returns `Ok(())` on success.
/// * May fail with [`ClientError::Network`], [`ClientError::Auth`], or
///   a backend-specific [`ClientError::NotSupported`] when a *specific*
///   target user / presence kind can't be mutated.
/// * Must not panic.
///
/// [`SocialGraphBackend`]: crate::SocialGraphBackend
/// [`SocialGraphBackend::as_writable_social_graph`]: crate::SocialGraphBackend::as_writable_social_graph
/// [`ClientError::Network`]: crate::ClientError::Network
/// [`ClientError::Auth`]: crate::ClientError::AuthFailed
/// [`ClientError::NotSupported`]: crate::ClientError::NotSupported
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WritableSocialGraphBackend: Send + Sync {
    /// Send a friend request to another user.
    async fn add_friend(&self, user_id: &str) -> ClientResult<()>;

    /// Remove a friend (or cancel an outgoing/incoming request).
    async fn remove_friend(&self, user_id: &str) -> ClientResult<()>;

    /// Accept or reject a pending friend request.
    async fn respond_to_friend_request(&self, user_id: &str, accept: bool) -> ClientResult<()>;

    /// Set or clear a per-friend nickname (`None` clears).
    async fn set_friend_nickname(
        &self,
        user_id: &str,
        nickname: Option<&str>,
    ) -> ClientResult<()>;

    /// Set or clear a private note about a user (`None` clears).
    async fn set_user_note(&self, user_id: &str, note: Option<&str>) -> ClientResult<()>;

    /// Block a user.
    async fn block_user(&self, user_id: &str) -> ClientResult<()>;

    /// Unblock a previously blocked user.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()>;

    /// Ignore a user.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()>;

    /// Reverse a previous `ignore_user`.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()>;

    /// Set the calling user's own presence status.
    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()>;
}
