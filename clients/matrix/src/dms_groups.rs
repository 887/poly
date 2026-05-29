//! `impl DmsAndGroupsBackend for MatrixClient` — DM channels, group rooms, mute/leave.
//!
//! Matrix supports DM channels (via m.direct account data) and group rooms.
//! No native saved-messages or add_group_member concept; those return NotSupported.

use poly_client::{ClientResult, Group, DmChannel, User, PresenceStatus, BackendType, ClientError};

use crate::MatrixClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for MatrixClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let user_id = self.current_user_id()?;
        let m_direct = self
            .http
            .fetch_account_data(&user_id, "m.direct")
            .await
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

        // F-MX-1: collect (room_id, user_id) pairs first, then fetch profiles concurrently.
        let mut pairs: Vec<(String, String)> = Vec::new();
        if let Some(obj) = m_direct.as_object() {
            for (other_user_id, room_ids) in obj {
                if let Some(room_id) = room_ids
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(serde_json::Value::as_str)
                {
                    pairs.push((room_id.to_string(), other_user_id.clone()));
                }
            }
        }

        // Fetch profiles in parallel; fall back to MXID on any error.
        let profile_futures: Vec<_> = pairs
            .iter()
            .map(|(_, uid)| self.http.fetch_profile(uid))
            .collect();
        let profiles = futures::future::join_all(profile_futures).await;

        let mut dms = Vec::new();
        for ((room_id, other_user_id), profile_result) in pairs.into_iter().zip(profiles) {
            let (display_name, avatar_url) = match profile_result {
                Ok(p) => (
                    p.displayname.unwrap_or_else(|| other_user_id.clone()),
                    p.avatar_url,
                ),
                Err(_) => (other_user_id.clone(), None),
            };
            dms.push(DmChannel {
                id: room_id,
                user: User {
                    id: other_user_id,
                    display_name,
                    avatar_url,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                last_message: None,
                unread_count: 0,
                backend: BackendType::from(crate::SLUG),
                account_id: user_id.clone(),
            });
        }

        Ok(dms)
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: Matrix DM creation not yet implemented".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Matrix has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_group_member: use add_users_to_group_dm for Matrix".to_string(),
        ))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "remove_group_member: Matrix room membership management not yet implemented"
                .to_string(),
        ))
    }

    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()> {
        for uid in user_ids {
            self.http.invite_to_room(channel_id, uid).await?;
        }
        Ok(())
    }

    /// Close a DM channel: leave the room, forget it, and remove from `m.direct`.
    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()> {
        self.http.leave_room(channel_id).await?;
        self.http.forget_room(channel_id).await?;
        let me = self.current_user_id()?;
        let mut m_direct = self.http.fetch_m_direct(&me).await?;
        if let Some(obj) = m_direct.as_object_mut() {
            for rooms in obj.values_mut() {
                if let Some(arr) = rooms.as_array_mut() {
                    arr.retain(|v| v.as_str() != Some(channel_id));
                }
            }
            obj.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
        }
        self.http.put_m_direct(&me, &m_direct).await?;
        Ok(())
    }

    async fn mute_conversation(
        &self,
        channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        self.http.put_room_push_rule_mute(channel_id).await
    }

    async fn unmute_conversation(&self, channel_id: &str) -> ClientResult<()> {
        self.http.delete_room_push_rule(channel_id).await
    }

    async fn leave_group_dm(&self, channel_id: &str) -> ClientResult<()> {
        self.http.leave_room(channel_id).await
    }

    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        if let Some(n) = name {
            self.http.set_room_name(channel_id, n).await?;
        }
        if let Some(url) = avatar_url {
            if url.starts_with("mxc://") {
                self.http.set_room_avatar(channel_id, url).await?;
            } else {
                tracing::warn!(
                    "edit_group_dm(matrix): avatar_url {url:?} is not an mxc:// URI — skipped"
                );
            }
        }
        Ok(())
    }
}
