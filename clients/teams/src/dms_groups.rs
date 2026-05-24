//! `impl DmsAndGroupsBackend for TeamsClient` — DMs, group chat ops.
//! H.3.c: Teams supports chat channels as DMs. No group-DM management API exposed.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use poly_client::*;

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// Teams supports chat channels as DMs. No group-DM management API exposed.

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for TeamsClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let account_id = self.account_id();
        Ok(self.http.get_chats().await?.into_iter().map(|chat| {
            let contact = chat.members.iter()
                .find(|m| m.user_id.as_deref() != Some(account_id.as_str()))
                .and_then(|m| {
                    m.display_name.as_ref().map(|name| User {
                        id: m.user_id.clone().unwrap_or_default(),
                        display_name: name.clone(),
                        avatar_url: None,
                        presence: PresenceStatus::Offline,
                        backend: BackendType::from(crate::SLUG),
                    })
                })
                .unwrap_or_else(|| self.unknown_user());
            DmChannel {
                id: chat.id,
                user: contact,
                last_message: None,
                unread_count: 0,
                backend: BackendType::from(crate::SLUG),
                account_id: account_id.clone(),
            }
        }).collect())
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        // C.2: POST /v1.0/chats with chatType=oneOnOne and a two-entry member list
        // (caller + target user). Graph returns the existing chat if one already
        // exists between these two users, so this is idempotent.
        let account_id = self.account_id();
        let members = vec![
            serde_json::json!({
                "@odata.type": "#microsoft.graph.aadUserConversationMember",
                "user@odata.bind": format!("https://graph.microsoft.com/v1.0/users('{account_id}')"),
                "roles": ["owner"],
            }),
            serde_json::json!({
                "@odata.type": "#microsoft.graph.aadUserConversationMember",
                "user@odata.bind": format!("https://graph.microsoft.com/v1.0/users('{user_id}')"),
                "roles": [],
            }),
        ];
        let chat = self.http.create_chat("oneOnOne", &members).await?;
        // Build a contact User from chat members; pick the non-self member.
        let contact = chat.members.iter()
            .find(|m| m.user_id.as_deref() != Some(account_id.as_str()))
            .map(|m| User {
                id: m.user_id.clone().unwrap_or_else(|| user_id.to_string()),
                display_name: m.display_name.clone().unwrap_or_else(|| user_id.to_string()),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from(crate::SLUG),
            })
            .unwrap_or_else(|| User {
                id: user_id.to_string(),
                display_name: user_id.to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from(crate::SLUG),
            });
        Ok(DmChannel {
            id: chat.id,
            user: contact,
            last_message: None,
            unread_count: 0,
            backend: BackendType::from(crate::SLUG),
            account_id,
        })
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Teams has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        // C.3: POST /v1.0/chats/{group_id}/members
        self.http.add_chat_member(group_id, user_id).await
    }

    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        // C.3: Resolve the membership ID for `user_id` then
        // DELETE /v1.0/chats/{group_id}/members/{membership_id}.
        // Graph requires the membership ID (base64-encoded composite), not the OID.
        let members = self.http.get_chat_members(group_id).await?;
        let membership_id = members
            .iter()
            .find(|m| m.user_id.as_deref() == Some(user_id) || m.id == user_id)
            .map(|m| m.id.clone())
            .ok_or_else(|| {
                ClientError::NotFound(format!(
                    "user {user_id} is not a member of chat {group_id}"
                ))
            })?;
        self.http.remove_chat_member(group_id, &membership_id).await
    }

    async fn add_users_to_group_dm(&self, channel_id: &str, user_ids: &[String]) -> ClientResult<()> {
        // C.3: Add each user sequentially. Graph does not expose a batch-add endpoint
        // for chat members, so this is O(n) round-trips. On the first error we
        // surface it immediately; partial success is left as-is (members already
        // added are not rolled back — Graph add is idempotent if the user is already in).
        for uid in user_ids {
            self.http.add_chat_member(channel_id, uid).await?;
        }
        Ok(())
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "close_dm_channel: not yet implemented for Teams".to_string(),
        ))
    }

    async fn mute_conversation(
        &self,
        channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        // SOLID-audit-teams C.4: Graph `PATCH /chats/{id}/members/{membershipId}` with
        // `notificationSettings` requires knowing the caller's membership ID for each chat,
        // which requires an extra round-trip per call.  The in-memory `muted_dms` store that
        // already backs the context-menu "mute-dm" action is the same source of truth the
        // sidebar uses — wire `mute_conversation` to it directly so both call sites agree.
        // The `_until` timed-mute field is noted but Graph notifications don't support
        // expiry; we store the mute unconditionally (best-effort parity).
        tracing::debug!(channel_id, "teams: mute_conversation (in-memory store)");
        self.muted_dms
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(channel_id.to_string());
        Ok(())
    }

    async fn unmute_conversation(&self, channel_id: &str) -> ClientResult<()> {
        // SOLID-audit-teams C.4: symmetric with mute_conversation above.
        tracing::debug!(channel_id, "teams: unmute_conversation (in-memory store)");
        self.muted_dms
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(channel_id);
        Ok(())
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "leave_group_dm: not yet implemented for Teams".to_string(),
        ))
    }

    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        // C.5: PATCH /v1.0/chats/{channel_id} with `topic` (display name).
        // Graph does not support a photo/avatar URL for chats — the `avatar_url`
        // field is accepted from the caller but silently ignored, matching the
        // "no endpoint exists" note in the plan.
        let _ = avatar_url; // Graph has no chat-photo endpoint; ignore gracefully.
        if let Some(topic) = name {
            self.http.patch_chat_topic(channel_id, topic).await?;
        }
        Ok(())
    }
}
