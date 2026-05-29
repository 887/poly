//! `impl ModerationBackend for TeamsClient` — kick, ban stubs, channel ops.
//! H.3.a: all moderation-surface methods live here.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use poly_client::{ClientResult, MemberPermissions, ClientError, BannedMember, UpdateChannelParams, ModerationLogEntry, Role};

// ── H.3.a — ModerationBackend ────────────────────────────────────────────────
#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for TeamsClient {
    /// Get the caller's permissions in a team.
    ///
    /// Fetches the team member list and checks whether the caller has the
    /// "owner" role. Teams is a binary owner/member model — no per-channel
    /// permissions exist in Graph.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let caller_id = self.account_id();
        let members = self.http.get_team_members(server_id).await?;
        let is_owner = members.iter().any(|m| {
            m.user_id.as_deref() == Some(caller_id.as_str())
                && m.roles.iter().any(|r| r == "owner")
        });
        Ok(MemberPermissions {
            manage_server: is_owner,
            manage_channels: is_owner,
            manage_roles: false, // no role concept in Teams
            kick_members: is_owner,
            ban_members: false,  // Teams has no ban concept
            manage_messages: is_owner,
            timeout_members: false, // no timeout concept in Teams
            display_role: if is_owner { "Owner".into() } else { "Member".into() },
            power_level: None,
        })
    }

    /// Kick a member by resolving their membership ID via the members list.
    ///
    /// `member_id` may be the user's OID; we look up the membership ID
    /// (base64-encoded composite) before issuing the DELETE.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        let members = self.http.get_team_members(server_id).await?;
        let membership_id = members
            .iter()
            .find(|m| m.user_id.as_deref() == Some(member_id) || m.id == member_id)
            .map(|m| m.id.clone())
            .ok_or_else(|| ClientError::NotFound(format!("member {member_id} not in team {server_id}")))?;
        self.http.delete_team_member(server_id, &membership_id).await
    }

    // ban_member / unban_member / timeout_member / untimeout_member / get_bans —
    // Teams has no ban or timeout concept. Return NotSupported so the UI gates
    // these behind has_ban=false / has_timed_ban=false and hides them entirely.

    async fn ban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "ban_member: Teams has no ban concept".into(),
        ))
    }

    async fn unban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "unban_member: Teams has no ban concept".into(),
        ))
    }

    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "timeout_member: Teams has no timeout concept".into(),
        ))
    }

    async fn untimeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "untimeout_member: Teams has no timeout concept".into(),
        ))
    }

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported(
            "get_bans: Teams has no ban concept".into(),
        ))
    }

    /// Soft-delete a channel message.
    ///
    /// Uses the Graph softDelete action which preserves the compliance copy.
    /// `channel_id` must be in `"team_id/channel_id"` format per plugin contract.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams delete_message requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http
            .soft_delete_channel_message(team_id, ch_id, message_id)
            .await
    }

    /// Update a channel.
    ///
    /// Only `name` (→ `displayName`) and `topic` (→ `description`) are
    /// forwarded to Graph. `slow_mode_secs`, `nsfw`, and `position` are silently
    /// ignored — Teams/Graph has no equivalent fields.
    ///
    /// `channel_id` must be in `"team_id/channel_id"` format.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams update_channel requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        // These three fields have no Graph equivalent — log at debug so we
        // don't warn-spam every time the UI sends a full update payload.
        // SOLID-audit-teams (Phase B.2).
        if update.slow_mode_secs.is_some() {
            tracing::debug!("Teams update_channel: slow_mode_secs has no Graph equivalent — ignored");
        }
        if update.nsfw.is_some() {
            tracing::debug!("Teams update_channel: nsfw has no Graph equivalent — ignored");
        }
        if update.position.is_some() {
            tracing::debug!("Teams update_channel: position has no Graph equivalent — ignored");
        }
        self.http
            .patch_channel(
                team_id,
                ch_id,
                update.name.as_deref(),
                update.topic.as_deref(),
            )
            .await
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Teams: Microsoft Graph has no channel position endpoint".into(),
        ))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported(
            "get_moderation_log: Teams has no moderation log".into(),
        ))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(
            "get_server_roles: Teams has no role concept".into(),
        ))
    }
}
