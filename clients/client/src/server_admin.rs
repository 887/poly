//! `ServerAdminBackend` capability sub-trait (Phase H.4.b).
//!
//! Carved out of [`ClientBackend`] in Phase H.4.b.  Groups server and channel
//! management operations that are only available on backends that support
//! modifiable server state.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(sa) = backend.as_server_admin() {
//!     sa.create_server(&name).await?;
//! }
//! ```
//!
//! WIT note: `create-server`, `create-channel`, `update-server-banner`,
//! `mark-channel-read`, `invite-user-to-server`, and `respond-to-server-invite`
//! are all in `poly:messenger/messenger-client`.  Only backends that have
//! real server-management capability opt in (server-client, discord, lemmy,
//! demo).
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;

use crate::{Channel, ChannelType, ClientResult, Server};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ServerAdminBackend: Send + Sync {
    /// Create a new server/guild in this backend.
    async fn create_server(&self, name: &str) -> ClientResult<Server>;

    /// Create a new channel inside a server.
    async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        channel_type: ChannelType,
    ) -> ClientResult<Channel>;

    /// Update the banner image URL for a server.
    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()>;

    /// Mark a channel (server channel or DM) as read on the backend.
    ///
    /// Backends that do not support server-side read markers may return
    /// `Ok(())` silently — this is a best-effort notification, not a
    /// required acknowledgement.  Backends that actively reject it may
    /// return `Err(NotSupported)`.
    async fn mark_channel_read(&self, channel_id: &str) -> ClientResult<()>;

    /// Accept or decline a pending server invite.
    async fn respond_to_server_invite(
        &self,
        server_id: &str,
        accept: bool,
    ) -> ClientResult<()>;

    /// Send a server invite to a specific user (DM-style invite).
    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()>;
}
