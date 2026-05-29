//! `ModerationBackend` + `WritableModerationBackend` for `ForgejoClient`.
//!
//! Forgejo is a forge, not a chat — kick/ban/timeout/role have no
//! analog and stay as `NotSupported` stubs. The one real writable
//! capability is `delete_message` (deletes an issue comment).
//!
//! Tier 2 (`plan-trait-split-readable-vs-writable.md`): all stubbed
//! get_* reads stay on `ModerationBackend`; the write methods that
//! ARE supported (delete_message) move into
//! `WritableModerationBackend`. The other write methods (kick/ban/…)
//! become defaulted by the read-trait shim and return `NotSupported`
//! through that path.

use async_trait::async_trait;
use poly_client::{ClientResult, MemberPermissions, BannedMember, ClientError, ModerationLogEntry, Role, UpdateChannelParams};
use crate::{ForgejoClient, channel_ids};

mod mod_ns {
    pub const BAN_LIST: &str = "Forgejo: no per-repo ban list";
    pub const MOD_LOG: &str = "Forgejo: admin audit log is not available via the REST API";
    pub const ROLES: &str = "Forgejo: no role concept";
    // Tier 2: KICK / BAN / UNBAN / TIMEOUT / CHANNEL_UPDATE /
    // CHANNEL_REORDER constants removed — those mutators dropped from
    // the impl block; the read-trait shim returns generic NotSupported.
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for ForgejoClient {
    /// Return the caller's effective permissions on a Forgejo repo.
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

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported(mod_ns::BAN_LIST.to_string()))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported(mod_ns::MOD_LOG.to_string()))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(mod_ns::ROLES.to_string()))
    }

    fn as_writable_moderation(
        &self,
    ) -> Option<&dyn poly_client::WritableModerationBackend> {
        Some(self)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableModerationBackend for ForgejoClient {
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

    /// Delete an issue comment.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let comment_id_str = message_id.strip_prefix("fj-comment-").ok_or_else(|| {
            ClientError::NotFound(format!(
                "delete_message: not a Forgejo comment id: {message_id}"
            ))
        })?;
        let comment_id: u64 = comment_id_str.parse().map_err(|_err| {
            ClientError::NotFound(format!(
                "delete_message: malformed comment id: {message_id}"
            ))
        })?;

        let (owner, repo) = channel_ids::parse_issue_thread_owner_repo(channel_id)?;
        self.api
            .delete_issue_comment(&owner, &repo, comment_id)
            .await
    }

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
}
