//! `DmsAndGroupsBackend` capability sub-trait (Phase H.3.c).
//!
//! Tier 2 of `plan-trait-split-readable-vs-writable.md`: the mutating
//! group/DM methods (`add_group_member`, `remove_group_member`,
//! `add_users_to_group_dm`, `edit_group_dm`, `close_dm_channel`) are
//! default-delegating shims that consult
//! [`Self::as_writable_dms_and_groups`].
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{ClientError, ClientResult, DmChannel, Group, WritableDmsAndGroupsBackend};

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

    /// Returns `Some(self)` if this backend implements
    /// [`WritableDmsAndGroupsBackend`].
    ///
    /// Default: `None`. Override in writable backends.
    fn as_writable_dms_and_groups(&self) -> Option<&dyn WritableDmsAndGroupsBackend> {
        None
    }

    // ── Write methods — default-delegating shims (Tier 2) ──────────────────

    /// Add a user to a group DM.
    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        match self.as_writable_dms_and_groups() {
            Some(w) => w.add_group_member(group_id, user_id).await,
            None => Err(ClientError::NotSupported("add_group_member".to_string())),
        }
    }

    /// Remove a user from a group DM.
    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        match self.as_writable_dms_and_groups() {
            Some(w) => w.remove_group_member(group_id, user_id).await,
            None => Err(ClientError::NotSupported(
                "remove_group_member".to_string(),
            )),
        }
    }

    /// Add one or more users to a group DM.
    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()> {
        match self.as_writable_dms_and_groups() {
            Some(w) => w.add_users_to_group_dm(channel_id, user_ids).await,
            None => Err(ClientError::NotSupported(
                "add_users_to_group_dm".to_string(),
            )),
        }
    }

    /// Update a group DM's name and/or avatar.
    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        match self.as_writable_dms_and_groups() {
            Some(w) => w.edit_group_dm(channel_id, name, avatar_url).await,
            None => Err(ClientError::NotSupported("edit_group_dm".to_string())),
        }
    }

    /// Hide a DM (1-on-1 or group) from the conversation list.
    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()> {
        match self.as_writable_dms_and_groups() {
            Some(w) => w.close_dm_channel(channel_id).await,
            None => Err(ClientError::NotSupported("close_dm_channel".to_string())),
        }
    }
}
