//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::{permission_bits, DiscordClient, guardrails, api};
use async_trait::async_trait;
use poly_client::{ClientResult, MemberPermissions, BannedMember, UpdateChannelParams, ModerationLogEntry, ModerationAction, User, PresenceStatus, BackendType, Role};

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for DiscordClient {
    /// B-DS-1: Compute effective permissions for the authenticated user.
    ///
    /// Fetches `GET /guilds/{id}/members/@me` to get role IDs, then
    /// `GET /guilds/{id}/roles` for the permission bitfields. Combines via OR.
    /// Guild owner gets all flags true regardless of roles.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        use twilight_model::id::marker::RoleMarker;
        use twilight_model::id::Id as TwilightId;
        use permission_bits::{ADMINISTRATOR, MANAGE_GUILD, MANAGE_CHANNELS, MANAGE_ROLES, KICK_MEMBERS, BAN_MEMBERS, MANAGE_MESSAGES, MODERATE_MEMBERS};

        let member = self.http.get_guild_member_me(server_id).await?;
        let all_roles = self.http.get_guild_roles(server_id).await?;
        let guild = self.http.get_guild(server_id).await?;

        // Determine if caller is the guild owner.
        let caller_id = self.account_id();
        let is_owner = guild
            .owner_id
            .as_deref()
            .is_some_and(|oid| oid == caller_id);

        if is_owner {
            // D.4 — cache owner status so pre-flight guards bypass all checks.
            self.permission_guard.set_owner(server_id, true);
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

        // Build a set of the caller's role IDs for fast lookup.
        let member_role_ids: std::collections::HashSet<TwilightId<RoleMarker>> =
            member.roles.into_iter().collect();

        // Find @everyone role (same ID as the guild).
        let everyone_id: u64 = server_id.parse().unwrap_or(0);

        // Combine permission bits: start with @everyone, then OR in member roles.
        let mut effective: i64 = 0;
        let mut highest_role_name = "Member".to_string();
        let mut highest_position = 0u32;

        for role in &all_roles {
            let role_id_u64 = role.id.get();
            let is_everyone = role_id_u64 == everyone_id;
            let is_member_role = member_role_ids.contains(&role.id);

            if is_everyone || is_member_role {
                let bits: i64 = role.permissions.parse().unwrap_or(0);
                effective |= bits;
                if is_member_role && role.position > highest_position {
                    highest_position = role.position;
                    highest_role_name.clone_from(&role.name);
                }
            }
        }

        let has = |flag: i64| (effective & ADMINISTRATOR != 0) || (effective & flag != 0);

        // D.4 — cache the computed effective permissions so pre-flight guards work.
        self.permission_guard
            .update_permissions(server_id, &effective.to_string());
        self.permission_guard.set_owner(server_id, is_owner);

        Ok(MemberPermissions {
            manage_server: has(MANAGE_GUILD),
            manage_channels: has(MANAGE_CHANNELS),
            manage_roles: has(MANAGE_ROLES),
            kick_members: has(KICK_MEMBERS),
            ban_members: has(BAN_MEMBERS),
            manage_messages: has(MANAGE_MESSAGES),
            timeout_members: has(MODERATE_MEMBERS),
            display_role: highest_role_name,
            power_level: None,
        })
    }

    /// B-DS-2: Kick a member from the guild.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        // D.4 — pre-flight permission guard (fail-safe: denies when not cached).
        if let Err(e) = self.permission_guard.check(
            server_id,
            guardrails::PERM_KICK_MEMBERS,
            "kick_member",
        ) {
            self.http.counters.inc_permission_trip("kick_member", server_id);
            return Err(e);
        }
        self.http.kick_member(server_id, member_id, reason).await
    }

    /// B-DS-3: Permanently ban a member.
    ///
    /// Discord bans are always permanent — `timeout_member` handles timed
    /// suspensions via `communication_disabled_until`.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        // D.4 — pre-flight permission guard.
        if let Err(e) = self.permission_guard.check(
            server_id,
            guardrails::PERM_BAN_MEMBERS,
            "ban_member",
        ) {
            self.http.counters.inc_permission_trip("ban_member", server_id);
            return Err(e);
        }
        self.http
            .ban_member(server_id, member_id, reason, delete_message_history_secs)
            .await
    }

    /// B-DS-4: Unban a member.
    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        // D.4 — unban requires BAN_MEMBERS permission.
        if let Err(e) = self.permission_guard.check(
            server_id,
            guardrails::PERM_BAN_MEMBERS,
            "unban_member",
        ) {
            self.http.counters.inc_permission_trip("unban_member", server_id);
            return Err(e);
        }
        self.http.unban_member(server_id, member_id).await
    }

    /// B-DS-5: List current bans.
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let bans = self.http.get_bans(server_id).await?;
        Ok(bans
            .into_iter()
            .map(|b| BannedMember {
                user_id: b.user.id.to_string(),
                display_name: b.user.global_name.unwrap_or(b.user.username),
                avatar_url: None,
                reason: b.reason,
                expires_at: None, // Discord bans are permanent
                banned_at: None,
            })
            .collect())
    }

    /// B-DS (timeout): Temporarily suspend a member via `communication_disabled_until`.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        // D.4 — MODERATE_MEMBERS permission guard.
        if let Err(e) = self.permission_guard.check(
            server_id,
            guardrails::PERM_MODERATE_MEMBERS,
            "timeout_member",
        ) {
            self.http.counters.inc_permission_trip("timeout_member", server_id);
            return Err(e);
        }
        let iso = until.to_rfc3339();
        self.http
            .set_member_timeout(server_id, member_id, Some(&iso))
            .await
    }

    /// B-DS (untimeout): Clear an active timeout.
    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        // D.4 — MODERATE_MEMBERS permission guard.
        if let Err(e) = self.permission_guard.check(
            server_id,
            guardrails::PERM_MODERATE_MEMBERS,
            "untimeout_member",
        ) {
            self.http.counters.inc_permission_trip("untimeout_member", server_id);
            return Err(e);
        }
        self.http.set_member_timeout(server_id, member_id, None).await
    }

    /// B-DS-6: Delete a single message.
    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        // D.4 — MANAGE_MESSAGES permission guard (guild_id not available here;
        // only enforce if we can look up the guild from the channel — skip if unknown).
        // Since we don't cache channel→guild mapping in ModerationBackend, we pass the
        // channel_id as the key. When no permissions are cached for channel_id, the
        // guard allows (not deny) for delete_message since the channel may not be a guild.
        // The HTTP layer will still 403 if Discord refuses.
        let _ = channel_id; // guard is best-effort here; http.delete_message handles 403
        self.http.delete_message(channel_id, message_id).await
    }

    /// B-DS-7: Update channel metadata.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        let mut body = serde_json::json!({});
        if let Some(obj) = body.as_object_mut() {
            if let Some(name) = &update.name {
                obj.insert("name".to_string(), serde_json::json!(name));
            }
            if let Some(topic) = &update.topic {
                obj.insert("topic".to_string(), serde_json::json!(topic));
            }
            if let Some(slow) = update.slow_mode_secs {
                obj.insert("rate_limit_per_user".to_string(), serde_json::json!(slow));
            }
            if let Some(nsfw) = update.nsfw {
                obj.insert("nsfw".to_string(), serde_json::json!(nsfw));
            }
            if let Some(pos) = update.position {
                obj.insert("position".to_string(), serde_json::json!(pos));
            }
        }
        self.http.patch_channel(channel_id, body).await.map(|_| ())
    }

    /// B-DS-8: Reorder channels within a guild.
    async fn reorder_channels(
        &self,
        server_id: &str,
        ordering: Vec<String>,
    ) -> ClientResult<()> {
        let payload: Vec<serde_json::Value> = ordering
            .into_iter()
            .enumerate()
            .map(|(pos, id)| serde_json::json!({ "id": id, "position": pos }))
            .collect();
        self.http.reorder_channels(server_id, &payload).await
    }

    /// B-DS-9: Fetch moderation log from Discord audit log.
    ///
    /// Maps action types: 20=kick, 22=ban_add, 23=ban_remove, 12=channel_update, 72=msg_delete.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        const MODERATION_ACTION_TYPES: &[u32] = &[20, 22, 23, 12, 72];

        let resp = self.http.get_audit_log(server_id, limit).await?;

        // Build a user lookup map from the embedded users array.
        let user_map: std::collections::HashMap<String, api::DiscordUser> = resp
            .users
            .into_iter()
            .map(|u| (u.id.to_string(), u))
            .collect();

        let entries = resp
            .audit_log_entries
            .into_iter()
            .filter(|e| MODERATION_ACTION_TYPES.contains(&e.action_type))
            .map(|entry| {
                let action = match entry.action_type {
                    20 => ModerationAction::MemberKicked,
                    22 => ModerationAction::MemberBanned,
                    23 => ModerationAction::MemberUnbanned,
                    12 => ModerationAction::ChannelUpdated,
                    72 => ModerationAction::MessageDeleted,
                    _ => ModerationAction::Other(entry.action_type.to_string()),
                };

                // Resolve moderator user from the map.
                let moderator_id = entry
                    .user_id
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                let moderator = user_map.get(&moderator_id).map_or_else(
                    || User {
                        id: moderator_id.clone(),
                        display_name: moderator_id.clone(),
                        avatar_url: None,
                        presence: PresenceStatus::Offline,
                        backend: BackendType::from(crate::SLUG),
                    },
                    |u| self.discord_user_to_poly(u.clone()),
                );

                // The audit log entry's snowflake ID encodes the timestamp.
                // Discord snowflake epoch: 2015-01-01T00:00:00.000Z = 1420070400000ms
                let entry_id_u64 = entry.id.get();
                let discord_epoch_ms: u64 = 1_420_070_400_000;
                let ts_ms = (entry_id_u64 >> 22).wrapping_add(discord_epoch_ms);
                let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                    i64::try_from(ts_ms).unwrap_or(i64::MAX),
                )
                .map_or_else(
                    || chrono::Utc::now().to_rfc3339(),
                    |dt| dt.to_rfc3339(),
                );

                ModerationLogEntry {
                    id: entry.id.to_string(),
                    action,
                    moderator,
                    target_user_id: entry.target_id.clone(),
                    target_display_name: None,
                    channel_id: None,
                    message_id: None,
                    reason: entry.reason,
                    timestamp,
                }
            })
            .collect();

        Ok(entries)
    }

    async fn get_server_roles(&self, server_id: &str) -> ClientResult<Vec<Role>> {
        let discord_roles = self.http.get_guild_roles(server_id).await?;

        let mut roles: Vec<Role> = discord_roles
            .into_iter()
            .map(|dr| {
                use permission_bits::{ADMINISTRATOR, MANAGE_GUILD, MANAGE_CHANNELS, MANAGE_ROLES, KICK_MEMBERS, BAN_MEMBERS, MANAGE_MESSAGES, MODERATE_MEMBERS};
                let perms_bits: i64 = dr.permissions.parse().unwrap_or(0);
                let is_admin = perms_bits & ADMINISTRATOR != 0;
                let has = |flag_bit: i64| is_admin || (perms_bits & flag_bit != 0);
                let permissions = MemberPermissions {
                    manage_server: has(MANAGE_GUILD),
                    manage_channels: has(MANAGE_CHANNELS),
                    manage_roles: has(MANAGE_ROLES),
                    kick_members: has(KICK_MEMBERS),
                    ban_members: has(BAN_MEMBERS),
                    manage_messages: has(MANAGE_MESSAGES),
                    timeout_members: has(MODERATE_MEMBERS),
                    display_role: dr.name.clone(),
                    power_level: None,
                };
                let color = if dr.color == 0 {
                    None
                } else {
                    Some(format!("#{:06X}", dr.color))
                };
                Role {
                    id: dr.id.to_string(),
                    name: dr.name,
                    color,
                    permissions,
                    position: dr.position,
                }
            })
            .collect();

        // Sort by position descending (highest rank first).
        roles.sort_by(|a, b| b.position.cmp(&a.position));
        Ok(roles)
    }
}
