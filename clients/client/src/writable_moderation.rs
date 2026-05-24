//! `WritableModerationBackend` capability sub-trait
//! (Tier 2 of `plan-trait-split-readable-vs-writable.md`).
//!
//! Carves the mutating methods off [`ModerationBackend`] so backends
//! that only expose read-side moderation (permissions, ban list, audit
//! log) can drop the kick/ban/timeout/update/reorder/delete stubs
//! entirely.
//!
//! [`ModerationBackend`]: crate::ModerationBackend

use async_trait::async_trait;

use crate::{ClientResult, UpdateChannelParams};

/// Capability sub-trait for backends that mutate server moderation
/// state (kick/ban/timeout members, edit/reorder channels, delete
/// messages).
///
/// Opt-in via [`ModerationBackend::as_writable_moderation`] +
/// `impl WritableModerationBackend for X`.
///
/// # Liskov contract
///
/// Each method MUST obey the same contract its sibling on
/// [`ModerationBackend`] had: returns `Ok(())` on success, may fail
/// with `Network`/`Auth`/`NotSupported` for the specific target, must
/// not panic.
///
/// [`ModerationBackend`]: crate::ModerationBackend
/// [`ModerationBackend::as_writable_moderation`]: crate::ModerationBackend::as_writable_moderation
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WritableModerationBackend: Send + Sync {
    /// Kick a member from a server.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()>;

    /// Permanently ban a member from a server.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()>;

    /// Lift a ban for a member.
    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()>;

    /// Temporarily suspend a member until `until`.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()>;

    /// Remove a timeout / suspension from a member.
    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()>;

    /// Delete a single message by ID.
    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()>;

    /// Update channel settings (name, topic, slow-mode, nsfw, position).
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()>;

    /// Reorder channels within a server.
    async fn reorder_channels(
        &self,
        server_id: &str,
        ordering: Vec<String>,
    ) -> ClientResult<()>;
}
