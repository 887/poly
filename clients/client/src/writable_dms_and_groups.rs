//! `WritableDmsAndGroupsBackend` capability sub-trait
//! (Tier 2 of `plan-trait-split-readable-vs-writable.md`).
//!
//! Carves the group-mutation / DM-mutation methods off
//! [`DmsAndGroupsBackend`] so backends that only expose the
//! conversation list / open-DM operations can drop the group-edit and
//! close-DM stubs.
//!
//! [`DmsAndGroupsBackend`]: crate::DmsAndGroupsBackend

use async_trait::async_trait;

use crate::ClientResult;

/// Capability sub-trait for backends that mutate DMs / group DM membership.
///
/// Opt-in via [`DmsAndGroupsBackend::as_writable_dms_and_groups`] +
/// `impl WritableDmsAndGroupsBackend for X`.
///
/// [`DmsAndGroupsBackend`]: crate::DmsAndGroupsBackend
/// [`DmsAndGroupsBackend::as_writable_dms_and_groups`]: crate::DmsAndGroupsBackend::as_writable_dms_and_groups
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait WritableDmsAndGroupsBackend: Send + Sync {
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

    /// Update a group DM's name and/or avatar.
    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()>;

    /// Hide a DM (1-on-1 or group) from the conversation list.
    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()>;
}
