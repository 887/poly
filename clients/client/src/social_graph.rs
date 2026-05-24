//! `SocialGraphBackend` capability sub-trait (Phase H.3.b).
//!
//! Carved out of [`ClientBackend`] in Phase H.3.b. Implemented by backends
//! that expose social operations: `poly-demo`, `poly-discord`, `poly-matrix`,
//! `poly-server-client`, `poly-stoat` (writable), and `poly-forgejo`,
//! `poly-github`, `poly-hackernews`, `poly-lemmy`, `poly-reddit`,
//! `poly-teams` (read-only or partial).
//!
//! # Read vs write split (Tier 2 of `plan-trait-split-readable-vs-writable.md`)
//!
//! The read surface (`get_user`, `get_friends`, `get_presence`) is
//! abstract and every implementer provides it.  The write surface
//! (`add_friend`, `block_user`, …) is carved out into
//! [`WritableSocialGraphBackend`]; the methods remain as
//! default-delegating shims on [`SocialGraphBackend`] that consult
//! [`SocialGraphBackend::as_writable_social_graph`] and forward when
//! `Some`, else return `Err(NotSupported)`. Read-only backends override
//! nothing in the write surface — the shim's `NotSupported` default is
//! the right answer.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(sg) = backend.as_social_graph() {
//!     let friends = sg.get_friends().await?;
//!     if let Some(wsg) = sg.as_writable_social_graph() {
//!         wsg.add_friend("u123").await?;
//!     }
//! }
//! ```
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`IsBackend::as_social_graph`]: crate::IsBackend::as_social_graph
//! [`WritableSocialGraphBackend`]: crate::WritableSocialGraphBackend

use async_trait::async_trait;

use crate::{ClientError, ClientResult, PresenceStatus, User, WritableSocialGraphBackend};

/// Capability sub-trait for social-graph operations.
///
/// No default impls for the reads: presence of `impl SocialGraphBackend`
/// is the opt-in signal. Backends that do not expose social-graph data
/// leave [`IsBackend::as_social_graph`] returning `None` (the default).
///
/// The write surface (`add_friend`, `block_user`, …) is provided as
/// default-delegating shims that consult
/// [`Self::as_writable_social_graph`]. Writable backends override that
/// accessor and provide a real [`WritableSocialGraphBackend`] impl.
///
/// [`IsBackend::as_social_graph`]: crate::IsBackend::as_social_graph
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SocialGraphBackend: Send + Sync {
    /// Get a user by ID.
    async fn get_user(&self, id: &str) -> ClientResult<User>;

    /// Get the authenticated user's friend list.
    async fn get_friends(&self) -> ClientResult<Vec<User>>;

    /// Get the current presence status for a user.
    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus>;

    /// Returns `Some(self)` if this backend implements
    /// [`WritableSocialGraphBackend`].
    ///
    /// Default: `None`. Override in writable backends to return
    /// `Some(self)`. Read-only backends leave the default and every
    /// write method below returns `Err(NotSupported)` via its shim.
    fn as_writable_social_graph(&self) -> Option<&dyn WritableSocialGraphBackend> {
        None
    }

    // ── Write methods — default-delegating shims (Tier 2) ──────────────────

    /// Send a friend request to another user.
    ///
    /// Default: delegates to [`WritableSocialGraphBackend::add_friend`]
    /// via [`Self::as_writable_social_graph`], else `Err(NotSupported)`.
    async fn add_friend(&self, user_id: &str) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.add_friend(user_id).await,
            None => Err(ClientError::NotSupported("add_friend".to_string())),
        }
    }

    /// Remove a friend (or cancel an outgoing/incoming request).
    async fn remove_friend(&self, user_id: &str) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.remove_friend(user_id).await,
            None => Err(ClientError::NotSupported("remove_friend".to_string())),
        }
    }

    /// Accept or reject a pending friend request.
    async fn respond_to_friend_request(&self, user_id: &str, accept: bool) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.respond_to_friend_request(user_id, accept).await,
            None => Err(ClientError::NotSupported(
                "respond_to_friend_request".to_string(),
            )),
        }
    }

    /// Set or clear a per-friend nickname (`None` clears).
    async fn set_friend_nickname(
        &self,
        user_id: &str,
        nickname: Option<&str>,
    ) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.set_friend_nickname(user_id, nickname).await,
            None => Err(ClientError::NotSupported("set_friend_nickname".to_string())),
        }
    }

    /// Set or clear a private note about a user (`None` clears).
    async fn set_user_note(&self, user_id: &str, note: Option<&str>) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.set_user_note(user_id, note).await,
            None => Err(ClientError::NotSupported("set_user_note".to_string())),
        }
    }

    /// Block a user.
    async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.block_user(user_id).await,
            None => Err(ClientError::NotSupported("block_user".to_string())),
        }
    }

    /// Unblock a previously blocked user.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.unblock_user(user_id).await,
            None => Err(ClientError::NotSupported("unblock_user".to_string())),
        }
    }

    /// Ignore a user.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.ignore_user(user_id).await,
            None => Err(ClientError::NotSupported("ignore_user".to_string())),
        }
    }

    /// Reverse a previous `ignore_user`.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.unignore_user(user_id).await,
            None => Err(ClientError::NotSupported("unignore_user".to_string())),
        }
    }

    /// Set the calling user's own presence status.
    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()> {
        match self.as_writable_social_graph() {
            Some(w) => w.set_presence(status).await,
            None => Err(ClientError::NotSupported("set_presence".to_string())),
        }
    }
}
