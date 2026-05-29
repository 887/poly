//! `impl DmsAndGroupsBackend for LemmyClient` — private messages + group stubs (H.3.c).
//!
//! Lemmy supports private messages (1:1 DMs). No group DMs.
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::{ClientResult, Group, DmChannel, ClientError};
use std::collections::HashMap;

use crate::{CONVO_MUTE_UNSUPPORTED, GROUP_DM_UNSUPPORTED, LemmyClient};
use crate::api::{self, map_pm_to_dm_channel};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for LemmyClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let my_user_id = self.current_user_id().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy client is not authenticated".to_string())
        })?;
        let (account_id, _) = self.current_account_metadata()?;

        let resp = self.http.fetch_private_messages().await?;

        let mut by_partner: HashMap<i64, _> = HashMap::new();
        for view in &resp.private_messages {
            let partner_id = if view.creator.id == my_user_id {
                view.recipient.id
            } else {
                view.creator.id
            };
            by_partner
                .entry(partner_id)
                .and_modify(|existing: &mut &api::PrivateMessageView| {
                    if view.private_message.published > existing.private_message.published {
                        *existing = view;
                    }
                })
                .or_insert(view);
        }

        Ok(by_partner
            .values()
            .map(|view| map_pm_to_dm_channel(view, my_user_id, &account_id))
            .collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: not yet implemented for Lemmy".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Lemmy has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(GROUP_DM_UNSUPPORTED.to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(GROUP_DM_UNSUPPORTED.to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported(GROUP_DM_UNSUPPORTED.to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "close_dm_channel: not yet implemented for Lemmy".to_string(),
        ))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(CONVO_MUTE_UNSUPPORTED.to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(CONVO_MUTE_UNSUPPORTED.to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(GROUP_DM_UNSUPPORTED.to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(GROUP_DM_UNSUPPORTED.to_string()))
    }
}
