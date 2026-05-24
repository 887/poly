//! `ModerationBackend` capability sub-trait (Phase H.3.a).
//!
//! Tier 2 of `plan-trait-split-readable-vs-writable.md`: the mutating
//! methods (kick/ban/timeout/update/reorder/delete_message) are now
//! default-delegating shims that consult
//! [`Self::as_writable_moderation`] and forward to
//! [`WritableModerationBackend`] when `Some`, else return
//! `Err(NotSupported)`. Read-only moderation backends (forge indexes,
//! news feeds that surface a ban list but don't allow mutating it)
//! leave the accessor `None` and skip implementing the writable trait.
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`IsBackend::as_moderation`]: crate::IsBackend::as_moderation
//! [`WritableModerationBackend`]: crate::WritableModerationBackend

use async_trait::async_trait;

use crate::{
    BannedMember, ClientError, ClientResult, MemberPermissions, ModerationLogEntry, Role,
    UpdateChannelParams, WritableModerationBackend,
};

/// Capability sub-trait for server moderation operations (reads +
/// shims).
///
/// The read surface (`get_my_permissions`, `get_bans`,
/// `get_moderation_log`, `get_server_roles`) is abstract. Write
/// methods are default-delegating shims via
/// [`Self::as_writable_moderation`].
///
/// [`IsBackend::as_moderation`]: crate::IsBackend::as_moderation
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ModerationBackend: Send + Sync {
    /// Get the calling user's effective permissions in a server (and optionally
    /// a specific channel).
    async fn get_my_permissions(
        &self,
        server_id: &str,
        channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions>;

    /// Get the list of banned members for a server.
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>>;

    /// Fetch recent moderation log entries for a server.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>>;

    /// Fetch the role list for a server.
    async fn get_server_roles(&self, server_id: &str) -> ClientResult<Vec<Role>>;

    /// Returns `Some(self)` if this backend implements
    /// [`WritableModerationBackend`].
    ///
    /// Default: `None`. Override in writable backends.
    fn as_writable_moderation(&self) -> Option<&dyn WritableModerationBackend> {
        None
    }

    // ── Write methods — default-delegating shims (Tier 2) ──────────────────

    /// Kick a member from a server.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.kick_member(server_id, member_id, reason).await,
            None => Err(ClientError::NotSupported("kick_member".to_string())),
        }
    }

    /// Permanently ban a member from a server.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => {
                w.ban_member(server_id, member_id, reason, delete_message_history_secs)
                    .await
            }
            None => Err(ClientError::NotSupported("ban_member".to_string())),
        }
    }

    /// Lift a ban for a member.
    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.unban_member(server_id, member_id).await,
            None => Err(ClientError::NotSupported("unban_member".to_string())),
        }
    }

    /// Temporarily suspend a member until `until`.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.timeout_member(server_id, member_id, until, reason).await,
            None => Err(ClientError::NotSupported("timeout_member".to_string())),
        }
    }

    /// Remove a timeout / suspension from a member.
    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.untimeout_member(server_id, member_id).await,
            None => Err(ClientError::NotSupported("untimeout_member".to_string())),
        }
    }

    /// Delete a single message by ID.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.delete_message(channel_id, message_id).await,
            None => Err(ClientError::NotSupported("delete_message".to_string())),
        }
    }

    /// Update channel settings.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.update_channel(channel_id, update).await,
            None => Err(ClientError::NotSupported("update_channel".to_string())),
        }
    }

    /// Reorder channels within a server.
    async fn reorder_channels(
        &self,
        server_id: &str,
        ordering: Vec<String>,
    ) -> ClientResult<()> {
        match self.as_writable_moderation() {
            Some(w) => w.reorder_channels(server_id, ordering).await,
            None => Err(ClientError::NotSupported("reorder_channels".to_string())),
        }
    }
}
