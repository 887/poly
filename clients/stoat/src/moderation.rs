//! `impl ModerationBackend for StoatClient` — kick / ban / timeout / role queries.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (H.3.a).

use crate::api::{self, StoatBanCreate, StoatChannelEdit, StoatMemberEdit};
use async_trait::async_trait;
use futures::future;
use poly_client::{
    BannedMember, ClientError, ClientResult, MemberPermissions, ModerationLogEntry, Role,
    UpdateChannelParams,
};

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for StoatClient {
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let (server, member_me) = future::try_join(
            self.http.fetch_server(server_id),
            self.http.fetch_my_member(server_id),
        )
        .await?;

        let current_user_id = self.current_account_metadata()?.0;

        // Server owner has all permissions.
        if server.owner == current_user_id {
            return Ok(MemberPermissions {
                manage_server: true,
                manage_channels: true,
                manage_roles: true,
                kick_members: true,
                ban_members: true,
                manage_messages: true,
                timeout_members: true,
                display_role: "Owner".to_string(),
                power_level: None,
            });
        }

        // Compute merged permission bitfield from all assigned roles.
        let roles_map: std::collections::HashMap<String, u64> = server
            .categories // roles live on the server, not categories; use member roles from member_me
            .iter()
            .flat_map(|_| std::iter::empty::<(String, u64)>())
            .collect();
        let _ = roles_map; // placeholder — Stoat roles are on StoatServer but not yet mapped

        // For now we compute from the known bit values directly using a naive
        // approach: if any role grants a bit we set the flag.  Phase B-ST-1 only
        // needs to handle the common case of "no explicit roles" (all false) plus
        // the owner case (all true).  A full role-walking implementation requires
        // the roles to be carried on StoatServer, which is a separate increment.
        // The member_me.roles list tells us role IDs but we don't have role
        // permission bits without a separate GET /servers/{id}/roles call.
        // For now return the safe empty set and mark as non-owner member.
        let has_roles = !member_me.roles.is_empty();
        let _ = has_roles;

        Ok(MemberPermissions {
            manage_server: false,
            manage_channels: false,
            manage_roles: false,
            kick_members: false,
            ban_members: false,
            manage_messages: false,
            timeout_members: false,
            display_role: if member_me.roles.is_empty() {
                "Member".to_string()
            } else {
                "Role Member".to_string()
            },
            power_level: None,
        })
    }

    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        self.http.kick_member(server_id, member_id).await
    }

    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        self.http
            .ban_member(
                server_id,
                member_id,
                &StoatBanCreate {
                    reason: reason.map(str::to_string),
                    delete_message_seconds: delete_message_history_secs,
                },
            )
            .await
    }

    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http.unban_member(server_id, member_id).await
    }

    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        self.http
            .edit_member(
                server_id,
                member_id,
                &StoatMemberEdit {
                    timeout: Some(until.to_rfc3339()),
                    remove: None,
                },
            )
            .await
    }

    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http
            .edit_member(
                server_id,
                member_id,
                &StoatMemberEdit {
                    timeout: None,
                    remove: Some(vec!["Timeout".to_string()]),
                },
            )
            .await
    }

    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let response = self.http.get_bans(server_id).await?;

        // Index users by ID for O(1) lookup.
        let user_index: std::collections::HashMap<String, api::StoatUser> = response
            .users
            .into_iter()
            .map(|user| (user.id.clone(), user))
            .collect();

        Ok(response
            .bans
            .into_iter()
            .map(|ban| {
                let user = user_index.get(&ban.id.user);
                BannedMember {
                    user_id: ban.id.user.clone(),
                    display_name: user
                        .and_then(|u| u.display_name.clone())
                        .unwrap_or_else(|| ban.id.user.clone()),
                    avatar_url: None, // Autumn URL not resolvable without root config here
                    reason: ban.reason,
                    expires_at: None, // Stoat bans are permanent
                    banned_at: None,
                }
            })
            .collect())
    }

    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        self.http.delete_message(channel_id, message_id).await
    }

    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        if update.position.is_some() {
            tracing::warn!(
                "update_channel: Stoat does not support channel position reordering; ignoring position field"
            );
        }

        let edit = StoatChannelEdit {
            name: update.name,
            description: update.topic,
            slowmode: update.slow_mode_secs,
            nsfw: update.nsfw,
        };

        self.http.edit_channel(channel_id, &edit).await
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Stoat: channel reordering not supported".to_string()))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported("Stoat: no audit log endpoint".to_string()))
    }

    async fn get_server_roles(&self, server_id: &str) -> ClientResult<Vec<Role>> {
        // SOLID-audit-stoat C.3: Revolt stores roles inline in the server payload
        // under a `roles` map keyed by role ID.  Fetch the server and extract them.
        let server = self.http.fetch_server(server_id).await?;
        Ok(server.into_poly_roles())
    }
}
