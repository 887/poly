//! `ModerationBackend` + `WritableModerationBackend` for `GitHubClient`.
//!
//! GitHub has no kick/ban/timeout/role concept; the one real writable
//! capability is `delete_message` (issue/PR comment delete).
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`): the read trait
//! keeps `get_my_permissions` (real) and the no-data getters
//! (`get_bans`, `get_moderation_log`, `get_server_roles`). The
//! writable trait carries `delete_message` (real) and stubs the rest.

use async_trait::async_trait;
use poly_client::{ClientResult, MemberPermissions, ClientError, BannedMember, ModerationLogEntry, Role, UpdateChannelParams};

use crate::forum::parse_forum_channel;
use crate::{GitHubClient, NS_NO_BAN_CONCEPT, NS_NO_TIMEOUT_CONCEPT};

// ── H.3.a — ModerationBackend (reads + writable accessor) ────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for GitHubClient {
    /// Get the calling user's effective permissions in a repo.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let (owner, repo) = self
            .resolve_owner_repo_from_server_id(server_id)
            .await
            .ok_or_else(|| ClientError::NotFound(format!("repo for server {server_id}")))?;

        let perms = self
            .cli
            .get_repo_permissions(&owner, &repo)
            .await
            .map_err(Self::convert_err)?;

        let display_role = if perms.admin {
            "Admin".to_string()
        } else if perms.maintain {
            "Maintainer".to_string()
        } else if perms.push {
            "Collaborator".to_string()
        } else if perms.triage {
            "Triager".to_string()
        } else {
            "Read".to_string()
        };

        Ok(MemberPermissions {
            manage_server: perms.admin,
            manage_channels: perms.admin || perms.maintain,
            manage_roles: perms.admin,
            kick_members: perms.admin,
            ban_members: perms.admin,
            manage_messages: perms.admin || perms.maintain || perms.push,
            timeout_members: false,
            display_role,
            power_level: None,
        })
    }

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported(
            "GitHub: no per-repo ban list".to_string(),
        ))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported(
            "GitHub: no moderation log".to_string(),
        ))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(
            "GitHub: no role concept".to_string(),
        ))
    }

    fn as_writable_moderation(
        &self,
    ) -> Option<&dyn poly_client::WritableModerationBackend> {
        Some(self)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableModerationBackend for GitHubClient {
    async fn kick_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "GitHub: no kick concept".to_string(),
        ))
    }

    async fn ban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_BAN_CONCEPT.to_string()))
    }

    async fn unban_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_BAN_CONCEPT.to_string()))
    }

    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_TIMEOUT_CONCEPT.to_string()))
    }

    async fn untimeout_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_NO_TIMEOUT_CONCEPT.to_string()))
    }

    /// Delete a comment by ID.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let (owner, repo) = parse_forum_channel(channel_id)?;

        if let Some(id_str) = message_id.strip_prefix("comment:") {
            let comment_id: u64 = id_str
                .parse()
                .map_err(|_e| ClientError::NotFound(format!("invalid comment id: {id_str}")))?;
            let endpoint = format!("/repos/{owner}/{repo}/issues/comments/{comment_id}");
            self.cli
                .api_delete(&endpoint)
                .await
                .map_err(Self::convert_err)
        } else if let Some(id_str) = message_id.strip_prefix("pr-comment:") {
            let comment_id: u64 = id_str
                .parse()
                .map_err(|_e| ClientError::NotFound(format!("invalid pr-comment id: {id_str}")))?;
            let endpoint = format!("/repos/{owner}/{repo}/pulls/comments/{comment_id}");
            self.cli
                .api_delete(&endpoint)
                .await
                .map_err(Self::convert_err)
        } else {
            Err(ClientError::NotSupported(format!(
                "GitHub: cannot delete message with unknown prefix in id '{message_id}'. \
                 Expected 'comment:<id>' or 'pr-comment:<id>'"
            )))
        }
    }

    async fn update_channel(
        &self,
        _channel_id: &str,
        _update: UpdateChannelParams,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "GitHub: channel update not supported".to_string(),
        ))
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "GitHub: channel reordering not supported".to_string(),
        ))
    }
}
