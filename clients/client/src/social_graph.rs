//! `SocialGraphBackend` capability sub-trait (Phase H.3.b).
//!
//! Carved out of [`ClientBackend`] in Phase H.3.b.  Implemented by backends
//! that expose social operations: `poly-demo`, `poly-discord`, `poly-matrix`,
//! `poly-server-client`, `poly-stoat` (all five have real implementations of
//! `get_friends` / `get_user`).  Other backends (`poly-lemmy`, `poly-teams`,
//! `poly-forgejo`, `poly-github`, `poly-hackernews`) leave
//! [`IsBackend::as_social_graph`] returning `None` (the default).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(sg) = backend.as_social_graph() {
//!     let friends = sg.get_friends().await?;
//! }
//! ```
//!
//! WIT note: WIT exposes a `dm-channel` type but there is currently no
//! separate `poly:client/social-graph` WIT interface.  These methods exist
//! as a pure Rust-side contract.
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`IsBackend::as_social_graph`]: crate::IsBackend::as_social_graph

use async_trait::async_trait;

use crate::{ClientResult, PresenceStatus, User};

/// Capability sub-trait for social-graph operations.
///
/// No default impls: presence of `impl SocialGraphBackend` is the opt-in signal.
/// Backends that do not support social graphs leave
/// [`IsBackend::as_social_graph`] returning `None` (the default).
///
/// [`IsBackend::as_social_graph`]: crate::IsBackend::as_social_graph
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SocialGraphBackend: Send + Sync {
    /// Get a user by ID.
    async fn get_user(&self, id: &str) -> ClientResult<User>;

    /// Get the authenticated user's friend list.
    async fn get_friends(&self) -> ClientResult<Vec<User>>;

    /// Send a friend request to another user.
    async fn add_friend(&self, user_id: &str) -> ClientResult<()>;

    /// Remove a friend (or cancel an outgoing/incoming request).
    async fn remove_friend(&self, user_id: &str) -> ClientResult<()>;

    /// Accept or reject a pending friend request.
    ///
    /// `user_id` is the ID of the user who sent the request.
    /// `accept` is `true` to accept, `false` to reject.
    async fn respond_to_friend_request(&self, user_id: &str, accept: bool) -> ClientResult<()>;

    /// Set or clear a per-friend nickname (`None` clears).
    async fn set_friend_nickname(
        &self,
        user_id: &str,
        nickname: Option<&str>,
    ) -> ClientResult<()>;

    /// Set or clear a private note about a user (`None` clears).
    ///
    /// Notes are visible only to the calling user.
    async fn set_user_note(&self, user_id: &str, note: Option<&str>) -> ClientResult<()>;

    /// Block a user. Future messages from them are hidden and they
    /// cannot DM the calling user.
    async fn block_user(&self, user_id: &str) -> ClientResult<()>;

    /// Unblock a previously blocked user.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()>;

    /// Ignore a user — quieter than block. Their messages are kept
    /// but notifications are suppressed and DMs hidden from the list.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()>;

    /// Reverse a previous `ignore_user`.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()>;

    /// Get the current presence status for a user.
    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus>;

    /// Set the calling user's own presence status.
    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()>;
}
