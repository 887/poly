//! `ServerAdminBackend` capability sub-trait (Phase H.4.b).
//!
//! Tier 2 of `plan-trait-split-readable-vs-writable.md`:
//! `create_server`, `create_channel`, `update_server_banner` are now
//! default-delegating shims that consult
//! [`Self::as_writable_server_admin`] and forward to
//! [`WritableServerAdminBackend`] when `Some`, else return
//! `Err(NotSupported)`.
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`WritableServerAdminBackend`]: crate::WritableServerAdminBackend

use async_trait::async_trait;

use crate::{
    Channel, ChannelType, ClientError, ClientResult, Server, WritableServerAdminBackend,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ServerAdminBackend: Send + Sync {
    /// Mark a channel (server channel or DM) as read on the backend.
    ///
    /// Backends that do not support server-side read markers may return
    /// `Ok(())` silently — this is a best-effort notification, not a
    /// required acknowledgement.
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

    /// Returns `Some(self)` if this backend implements
    /// [`WritableServerAdminBackend`].
    ///
    /// Default: `None`. Override in writable backends.
    fn as_writable_server_admin(&self) -> Option<&dyn WritableServerAdminBackend> {
        None
    }

    // ── Write methods — default-delegating shims (Tier 2) ──────────────────

    /// Create a new server/guild in this backend.
    async fn create_server(&self, name: &str) -> ClientResult<Server> {
        match self.as_writable_server_admin() {
            Some(w) => w.create_server(name).await,
            None => Err(ClientError::NotSupported("create_server".to_string())),
        }
    }

    /// Create a new channel inside a server.
    async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        match self.as_writable_server_admin() {
            Some(w) => w.create_channel(server_id, name, channel_type).await,
            None => Err(ClientError::NotSupported("create_channel".to_string())),
        }
    }

    /// Update the banner image URL for a server.
    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        match self.as_writable_server_admin() {
            Some(w) => w.update_server_banner(server_id, banner_url).await,
            None => Err(ClientError::NotSupported(
                "update_server_banner".to_string(),
            )),
        }
    }
}
