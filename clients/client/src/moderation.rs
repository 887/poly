//! `ModerationBackend` capability sub-trait (Phase H.3.a).
//!
//! Carved out of [`ClientBackend`] in Phase H.3.a.  Implemented by backends
//! that expose moderation operations: currently `poly-discord`, `poly-matrix`,
//! `poly-stoat`, `poly-lemmy`, `poly-server-client`, and partially `poly-teams`
//! and `poly-forgejo` (real-impl on `get_my_permissions` and some methods).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(m) = backend.as_moderation() {
//!     m.ban_member(server_id, user_id, Some("spam"), None).await?;
//! }
//! ```
//!
//! WIT note: there is currently no `poly:client/moderation` WIT interface —
//! these methods exist as a pure Rust-side contract.  If a WIT interface is
//! added in the future, this trait MUST mirror its surface exactly to keep the
//! plugin-host bridge in sync.
//!
//! [`ClientBackend`]: crate::ClientBackend
//! [`IsBackend::as_moderation`]: crate::IsBackend::as_moderation

use async_trait::async_trait;

use crate::{
    BannedMember, ClientResult, MemberPermissions, ModerationLogEntry, Role,
    UpdateChannelParams,
};

/// Capability sub-trait for server moderation operations.
///
/// No default impls: presence of `impl ModerationBackend` is the opt-in signal.
/// Backends that do not support moderation leave [`IsBackend::as_moderation`]
/// returning `None` (the default).
///
/// [`IsBackend::as_moderation`]: crate::IsBackend::as_moderation
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ModerationBackend: Send + Sync {
    /// Get the calling user's effective permissions in a server (and optionally
    /// a specific channel).
    ///
    /// Backends that do not expose a permission model return `NotSupported`.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions>;

    /// Kick a member from a server.
    ///
    /// Backends that do not support kick return `NotSupported`.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()>;

    /// Permanently ban a member from a server.
    ///
    /// Use `timeout_member` for temporary suspensions. Backends that do not
    /// support permanent bans return `NotSupported`.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()>;

    /// Lift a ban for a member.
    ///
    /// Backends that do not support bans return `NotSupported`.
    async fn unban_member(
        &self,
        server_id: &str,
        member_id: &str,
    ) -> ClientResult<()>;

    /// Temporarily suspend a member until `until`.
    ///
    /// This maps to Discord's `communication_disabled_until`, Stoat's native
    /// timeout field, or Lemmy's `expires`-bearing ban — each backend uses its
    /// own native primitive. Backends that do not support timed suspensions
    /// return `NotSupported`.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()>;

    /// Remove a timeout / suspension from a member.
    ///
    /// Backends that do not support timeouts return `NotSupported`.
    async fn untimeout_member(
        &self,
        server_id: &str,
        member_id: &str,
    ) -> ClientResult<()>;

    /// Get the list of banned members for a server.
    ///
    /// Backends that do not support bans return `NotSupported`.
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>>;

    /// Delete a single message by ID.
    ///
    /// The caller should already have verified the user has `manage_messages`
    /// permission or is the message author. Backends that do not support
    /// message deletion return `NotSupported`.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()>;

    /// Update channel settings (name, topic, slow-mode, nsfw, position).
    ///
    /// Only fields set to `Some` are changed. Backends that do not support
    /// channel editing return `NotSupported`.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()>;

    /// Reorder channels within a server.
    ///
    /// `ordering` is the desired channel-ID order (all channels, including
    /// those not being moved). Backends that do not support reordering return
    /// `NotSupported`.
    async fn reorder_channels(
        &self,
        server_id: &str,
        ordering: Vec<String>,
    ) -> ClientResult<()>;

    /// Fetch recent moderation log entries for a server.
    ///
    /// `limit` caps the number of entries returned. Backends that do not
    /// expose a moderation log return `NotSupported`.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>>;

    /// Fetch the role list for a server.
    ///
    /// Returns roles sorted by position (ascending). Backends that do not
    /// expose roles return `NotSupported`.
    async fn get_server_roles(&self, server_id: &str) -> ClientResult<Vec<Role>>;
}
