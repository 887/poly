//! `DmsAndGroupsBackend` impl for [`super::RedditBackend`].
//!
//! Reddit supports inbox messages as DMs. No group DMs.

use async_trait::async_trait;
use poly_client::{ClientError, ClientResult, DmChannel, Group};

use super::error::{NS_CLOSE_DM, NS_CONV_MUTE, NS_GROUP_DM, NS_OPEN_DM, NS_SAVED_MSG};
use super::mapping::raw_dm_to_dm_channel;
use super::RedditBackend;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for RedditBackend {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let dms = self.client.inbox().await.map_err(ClientError::from)?;
        let account_id = self.account_id();
        let bt = Self::backend_type();
        Ok(dms
            .iter()
            .map(|dm| raw_dm_to_dm_channel(dm, account_id, &bt))
            .collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(NS_OPEN_DM.to_string()))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(NS_SAVED_MSG.to_string()))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_GROUP_DM.to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_GROUP_DM.to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_GROUP_DM.to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_CLOSE_DM.to_string()))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_CONV_MUTE.to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_CONV_MUTE.to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_GROUP_DM.to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_GROUP_DM.to_string()))
    }
}
