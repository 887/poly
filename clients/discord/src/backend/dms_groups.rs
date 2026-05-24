//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::*;
use async_trait::async_trait;
use poly_client::*;

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for DiscordClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        use twilight_model::channel::ChannelType as DcChType;
        let account_id = self.account_id();
        Ok(self.http.get_dm_channels().await?.into_iter()
            .filter(|c| c.channel_type == DcChType::Private)
            .map(|c| DmChannel {
                id: c.id.to_string(),
                user: User {
                    id: String::new(),
                    display_name: c.name,
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                last_message: None,
                unread_count: 0,
                backend: BackendType::from(crate::SLUG),
                account_id: account_id.clone(),
            })
            .collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: not yet implemented for Discord".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Discord has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_group_member: use add_users_to_group_dm for Discord".to_string(),
        ))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "remove_group_member: not yet implemented for Discord".to_string(),
        ))
    }

    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()> {
        for uid in user_ids {
            self.http.add_group_dm_recipient(channel_id, uid).await?;
        }
        Ok(())
    }

    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()> {
        self.http.delete_channel(channel_id).await
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "mute_conversation: Discord notification settings require guild context; not yet implemented".to_string(),
        ))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "unmute_conversation: Discord notification settings require guild context; not yet implemented".to_string(),
        ))
    }

    async fn leave_group_dm(&self, channel_id: &str) -> ClientResult<()> {
        self.http.delete_channel(channel_id).await
    }

    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        let mut body = serde_json::json!({});
        if let Some(obj) = body.as_object_mut() {
            if let Some(n) = name {
                obj.insert("name".to_string(), serde_json::json!(n));
            }
            if let Some(icon) = avatar_url {
                obj.insert("icon".to_string(), serde_json::json!(icon));
            }
        }
        self.http.patch_channel(channel_id, body).await.map(|_| ())
    }
}
