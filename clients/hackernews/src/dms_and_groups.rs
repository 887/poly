//! `DmsAndGroupsBackend` implementation for `HackerNewsClient`.
//!
//! Hacker News has no DM, group DM, or conversation-mute concept.
//! All methods return either an empty list or `NotSupported`.

use async_trait::async_trait;
use poly_client::{ClientResult, Group, DmChannel, ClientError};

use crate::HackerNewsClient;

// ── NotSupported constants ───────────────────────────────────────────────────

const ERR_NO_DM: &str = "Hacker News has no DM concept";
const ERR_NO_SAVED_MESSAGES: &str = "Hacker News has no saved-messages concept";
const ERR_NO_GROUP_DM: &str = "Hacker News has no group DMs";
const ERR_NO_CONV_MUTE: &str = "Hacker News has no conversation mute";

// ── H.3.c — DmsAndGroupsBackend ─────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for HackerNewsClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(ERR_NO_DM.to_string()))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(ERR_NO_SAVED_MESSAGES.to_string()))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_GROUP_DM.to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_GROUP_DM.to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_GROUP_DM.to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_DM.to_string()))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_CONV_MUTE.to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_CONV_MUTE.to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_GROUP_DM.to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(ERR_NO_GROUP_DM.to_string()))
    }
}
