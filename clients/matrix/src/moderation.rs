//! `ModerationBackend` + `WritableModerationBackend` for `MatrixClient`.
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`): writes
//! (kick/ban/unban/timeout/redact/update_channel/reorder) live in the
//! writable impl; reads (`get_my_permissions`, `get_bans`,
//! `get_moderation_log`, `get_server_roles`) stay on the read trait.

use async_trait::async_trait;
use poly_client::*;

use crate::moderation_log;
use crate::MatrixClient;

// ── ModerationBackend (reads + writable accessor) ────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for MatrixClient {
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let user_id = self.current_user_id()?;
        let pl = self.http.fetch_power_levels(server_id).await?;
        let level = pl.user_level(&user_id);

        let display_role = if level >= 100 {
            "Admin".to_string()
        } else if level >= 50 {
            "Moderator".to_string()
        } else {
            "Member".to_string()
        };

        Ok(MemberPermissions {
            manage_server: level >= pl.state_default,
            manage_channels: level >= pl.state_default,
            manage_roles: level >= pl.state_default,
            kick_members: level >= pl.kick,
            ban_members: level >= pl.ban,
            manage_messages: level >= pl.redact,
            timeout_members: false,
            display_role,
            power_level: Some(level),
        })
    }

    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let members_resp = self.http.fetch_banned_members(server_id).await?;

        let banned: Vec<BannedMember> = members_resp
            .chunk
            .iter()
            .filter(|ev| {
                ev.event_type == "m.room.member"
                    && ev
                        .content
                        .get("membership")
                        .and_then(serde_json::Value::as_str)
                        == Some("ban")
            })
            .filter_map(|ev| {
                let user_id = ev.state_key.as_deref()?;
                let display_name = ev
                    .content
                    .get("displayname")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(user_id)
                    .to_string();
                let avatar_url = ev
                    .content
                    .get("avatar_url")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                let reason = ev
                    .content
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                Some(BannedMember {
                    user_id: user_id.to_string(),
                    display_name,
                    avatar_url,
                    reason,
                    expires_at: None,
                    banned_at: None,
                })
            })
            .collect();

        Ok(banned)
    }

    /// Synthesise a moderation log by walking recent timeline events on every
    /// child room of the space.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        moderation_log::synthesize(&self.http, server_id, limit).await
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(
            "Matrix: no server roles concept; power levels serve this function".to_string(),
        ))
    }

    fn as_writable_moderation(
        &self,
    ) -> Option<&dyn poly_client::WritableModerationBackend> {
        Some(self)
    }
}

// ── WritableModerationBackend (writes) ──────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableModerationBackend for MatrixClient {
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        self.http.kick_member(server_id, member_id, reason).await
    }

    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        self.http.ban_member(server_id, member_id, reason).await
    }

    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http.unban_member(server_id, member_id).await
    }

    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "timeout: Matrix has no timeout primitive; use power level changes for moderation"
                .to_string(),
        ))
    }

    async fn untimeout_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "untimeout: Matrix has no timeout primitive".to_string(),
        ))
    }

    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        let txn_id = uuid::Uuid::new_v4().to_string();
        self.http
            .redact_event(channel_id, message_id, &txn_id, None)
            .await
    }

    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        if update.position.is_some() {
            tracing::warn!(
                "update_channel(matrix): `position` has no Matrix equivalent — ignored"
            );
        }
        if update.slow_mode_secs.is_some() {
            tracing::warn!(
                "update_channel(matrix): `slow_mode_secs` has no Matrix equivalent — ignored"
            );
        }
        if update.nsfw.is_some() {
            tracing::warn!("update_channel(matrix): `nsfw` has no Matrix equivalent — ignored");
        }

        if let Some(name) = &update.name {
            self.http.set_room_name(channel_id, name).await?;
        }
        if let Some(topic) = &update.topic {
            self.http.set_room_topic(channel_id, topic).await?;
        }

        Ok(())
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Matrix spaces order via space hierarchy state events; reorder not exposed in trait shape"
                .to_string(),
        ))
    }
}
