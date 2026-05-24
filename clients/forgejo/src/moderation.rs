//! `impl ModerationBackend for ForgejoClient` — permissions, delete_message,
//! and not-supported stubs for kick/ban/role management.

use crate::*;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for ForgejoClient {
    /// Return the caller's effective permissions on a Forgejo repo.
    ///
    /// Calls `GET /repos/{owner}/{repo}` and reads the `permissions` object.
    /// `manage_messages` is true when the caller has admin or push access
    /// (which lets them delete issue comments via the REST API).
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let (owner, repo) = channel_ids::repo_owner_name_from_server_id(self, server_id).await?;
        let resp = self.api.get_repo_permissions(&owner, &repo).await?;
        let p = resp.permissions;
        let can_manage = p.admin || p.push;
        let display_role = if p.admin {
            "Admin".to_string()
        } else if p.push {
            "Write".to_string()
        } else {
            "Read".to_string()
        };
        Ok(MemberPermissions {
            manage_server: p.admin,
            manage_channels: false,
            manage_roles: false,
            kick_members: false,
            ban_members: false,
            manage_messages: can_manage,
            timeout_members: false,
            display_role,
            power_level: None,
        })
    }

    /// Kick is not a concept on Forgejo (collaborator management is separate).
    async fn kick_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: collaborators have no kick concept; use the org settings to remove access"
                .to_string(),
        ))
    }

    /// Forgejo has no per-repo ban concept.
    async fn ban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no per-repo ban; site admins can suspend users via the admin panel only"
                .to_string(),
        ))
    }

    async fn unban_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no per-repo ban/unban".to_string(),
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
            "Forgejo: no timeout concept".to_string(),
        ))
    }

    async fn untimeout_member(&self, _server_id: &str, _member_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: no timeout concept".to_string(),
        ))
    }

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported(
            "Forgejo: no per-repo ban list".to_string(),
        ))
    }

    /// Delete an issue comment.
    ///
    /// `channel_id` must be an issue thread channel (`fj-issue-{owner}-{repo}-{n}`).
    /// `message_id` must be a comment message ID (`fj-comment-{numeric_id}`).
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        // Parse the numeric comment id from the message_id prefix.
        let comment_id_str = message_id
            .strip_prefix("fj-comment-")
            .ok_or_else(|| {
                ClientError::NotFound(format!(
                    "delete_message: not a Forgejo comment id: {message_id}"
                ))
            })?;
        let comment_id: u64 = comment_id_str.parse().map_err(|_err| {
            ClientError::NotFound(format!(
                "delete_message: malformed comment id: {message_id}"
            ))
        })?;

        // Parse owner/repo from the issue thread channel id.
        let (owner, repo) = channel_ids::parse_issue_thread_owner_repo(channel_id)?;
        self.api.delete_issue_comment(&owner, &repo, comment_id).await
    }

    /// Channel update is not supported for Forgejo repos.
    async fn update_channel(
        &self,
        _channel_id: &str,
        _update: UpdateChannelParams,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: channel concept maps to issue/PR sections; renaming/reordering not exposed"
                .to_string(),
        ))
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Forgejo: channel reordering not supported".to_string(),
        ))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported(
            "Forgejo: admin audit log is not available via the REST API".to_string(),
        ))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(
            "Forgejo: no role concept".to_string(),
        ))
    }
}
