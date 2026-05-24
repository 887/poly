use async_trait::async_trait;
use poly_client::*;

use crate::{
    GitHubClient, NS_NO_CONVERSATION_MUTE, NS_NO_DM_CONCEPT, NS_NO_GROUP_DMS,
};

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// GitHub has no DM or group DM concept.

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for GitHubClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(NS_NO_DM_CONCEPT.to_string()))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported("GitHub has no saved-messages concept".to_string()))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_GROUP_DMS.to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_GROUP_DMS.to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_GROUP_DMS.to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_DM_CONCEPT.to_string()))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_CONVERSATION_MUTE.to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_CONVERSATION_MUTE.to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_GROUP_DMS.to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_GROUP_DMS.to_string()))
    }
}
