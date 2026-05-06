//! `DmsAndGroupsBackend` capability sub-trait (Phase H.3.c).
//!
//! Carved out of [`ClientBackend`] in Phase H.3.c.  Implemented by backends
//! that expose direct messaging and group DM operations.
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(dg) = backend.as_dms_and_groups() {
//!     let dms = dg.get_dm_channels().await?;
//! }
//! ```
//!
//! WIT note: WIT exposes `dm-channel` and related types but there is
//! currently no separate `poly:client/dms-and-groups` WIT interface.
//! These methods exist as a pure Rust-side contract.
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{ClientResult, DmChannel, Group};

/// Capability sub-trait for direct messaging and group DM operations.
///
/// No default impls: presence of `impl DmsAndGroupsBackend` is the opt-in signal.
/// Backends that do not support DMs/groups leave
/// [`IsBackend::as_dms_and_groups`] returning `None` (the default).
///
/// [`IsBackend::as_dms_and_groups`]: crate::IsBackend::as_dms_and_groups
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait DmsAndGroupsBackend: Send + Sync {
    /// Get all DM channels.
    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>>;

    /// Get all group chats.
    async fn get_groups(&self) -> ClientResult<Vec<Group>>;

    /// Open or create a DM channel with the target user.
    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel>;

    /// Open the authenticated user's Saved Messages / self-DM channel.
    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel>;

    /// Add a user to a group DM.
    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()>;

    /// Remove a user from a group DM.
    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()>;

    /// Add one or more users to a group DM.
    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()>;

    /// Hide a DM (1-on-1 or group) from the conversation list. The
    /// channel itself is not deleted; receiving a new message reopens it.
    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()>;

    /// Mute notifications for a conversation (channel or DM) until the
    /// given timestamp; pass `None` to mute indefinitely.
    async fn mute_conversation(
        &self,
        channel_id: &str,
        until: Option<DateTime<Utc>>,
    ) -> ClientResult<()>;

    /// Reverse a previous `mute_conversation`.
    async fn unmute_conversation(&self, channel_id: &str) -> ClientResult<()>;

    /// Leave a group DM. The remaining members continue without the caller.
    async fn leave_group_dm(&self, channel_id: &str) -> ClientResult<()>;

    /// Update a group DM's name and/or avatar (`None` leaves field unchanged).
    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()>;
}
