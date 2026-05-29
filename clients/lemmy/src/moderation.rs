//! `impl ModerationBackend for LemmyClient` — bans, removals, modlog (H.3.a).
//!
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::{ClientResult, MemberPermissions, ClientError, BannedMember, UpdateChannelParams, ModerationLogEntry, ModerationAction, User, PresenceStatus, BackendType, Role};
use std::collections::HashMap;

use crate::LemmyClient;
use crate::api::{self, BanFromCommunityRequest, map_person};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for LemmyClient {
    async fn get_my_permissions(
        &self,
        _server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        Err(ClientError::NotSupported("Lemmy: permission model not exposed".to_string()))
    }

    /// Lemmy has no kick concept — community membership is implicit.
    async fn kick_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy has no kick concept; community membership is implicit".to_string(),
        ))
    }

    /// Ban a member from a community (permanent — no `expires`).
    ///
    /// `server_id` is `lemmy-community-{id}`.
    /// `member_id` is a Lemmy person id as a string or `lemmy-user-{id}`.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        let person_id = Self::parse_person_id(member_id)?;
        self.http
            .ban_from_community(BanFromCommunityRequest {
                community_id,
                person_id,
                ban: true,
                reason: reason.map(str::to_string),
                expires: None,
                remove_data: false,
            })
            .await
            .map(|_| ())
    }

    /// Unban a member from a community.
    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        let person_id = Self::parse_person_id(member_id)?;
        self.http
            .ban_from_community(BanFromCommunityRequest {
                community_id,
                person_id,
                ban: false,
                reason: None,
                expires: None,
                remove_data: false,
            })
            .await
            .map(|_| ())
    }

    /// Timeout a member by banning with a native `expires` timestamp.
    ///
    /// Lemmy's `ban_user` endpoint accepts a Unix timestamp `expires` field,
    /// making a short ban functionally equivalent to a timeout. This method is
    /// a thin wrapper that calls `ban_from_community` with `ban: true` and the
    /// computed expiry.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        let person_id = Self::parse_person_id(member_id)?;
        self.http
            .ban_from_community(BanFromCommunityRequest {
                community_id,
                person_id,
                ban: true,
                reason: reason.map(str::to_string),
                expires: Some(until.timestamp()),
                remove_data: false,
            })
            .await
            .map(|_| ())
    }

    /// Remove a timeout from a member by unbanning them.
    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.unban_member(server_id, member_id).await
    }

    /// List banned members by querying the modlog for `ModBanFromCommunity` events.
    ///
    /// Lemmy has no `/community/bans` endpoint; `GET /api/v3/modlog` with
    /// `type_=ModBanFromCommunity` is the only way to retrieve the ban list.
    /// The response includes all ban/unban history; we deduplicate per person
    /// keeping only the most recent ban entry (ignoring unban records).
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let community_id = Self::parse_community_id(server_id)?;
        let modlog = self.http.get_modlog_bans(community_id).await?;

        // Deduplicate: for each person keep only their most recent entry.
        // If the most recent entry has `banned==true`, they are still banned.
        // If it has `banned==false` (unban), they are not currently banned.
        let mut most_recent: HashMap<i64, api::ModBanFromCommunityView> = HashMap::new();
        for entry in modlog {
            most_recent
                .entry(entry.banned_person.id)
                .and_modify(|existing| {
                    if entry.mod_ban_from_community.when_
                        > existing.mod_ban_from_community.when_
                    {
                        *existing = entry.clone();
                    }
                })
                .or_insert(entry);
        }

        // Only include entries where the most recent action was a ban.
        let by_person: HashMap<i64, api::ModBanFromCommunityView> = most_recent
            .into_iter()
            .filter(|(_, e)| e.mod_ban_from_community.banned)
            .collect();

        Ok(by_person
            .values()
            .map(|e| BannedMember {
                user_id: format!("lemmy-user-{}", e.banned_person.id),
                display_name: e
                    .banned_person
                    .display_name
                    .clone()
                    .unwrap_or_else(|| e.banned_person.name.clone()),
                avatar_url: e.banned_person.avatar.clone(),
                reason: e.mod_ban_from_community.reason.clone(),
                expires_at: e
                    .mod_ban_from_community
                    .expires
                    .map(|dt| dt.to_rfc3339()),
                banned_at: Some(e.mod_ban_from_community.when_.to_rfc3339()),
            })
            .collect())
    }

    /// Delete (remove) a message by ID.
    ///
    /// Message IDs are prefixed:
    /// - `lemmy-post-{id}` → `POST /api/v3/post/remove`
    /// - `lemmy-comment-{id}` → `POST /api/v3/comment/remove`
    ///
    /// The `channel_id` parameter is ignored — Lemmy's remove endpoints use
    /// only the post/comment id.
    async fn delete_message(
        &self,
        _channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        if let Some(post_id) = message_id
            .strip_prefix("lemmy-post-")
            .and_then(|s| s.parse::<i64>().ok())
        {
            return self.http.remove_post(post_id, None).await;
        }

        if let Some(comment_id) = message_id
            .strip_prefix("lemmy-comment-")
            .and_then(|s| s.parse::<i64>().ok())
        {
            return self.http.remove_comment(comment_id, None).await;
        }

        Err(ClientError::NotFound(format!(
            "delete_message: unrecognised message id '{message_id}'; \
             expected 'lemmy-post-{{n}}' or 'lemmy-comment-{{n}}'"
        )))
    }

    /// Lemmy community update is admin-only and out of scope for v1.
    async fn update_channel(
        &self,
        _channel_id: &str,
        _update: UpdateChannelParams,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy: 'channel' = community; community update is admin-only and out-of-scope for v1"
                .to_string(),
        ))
    }

    /// Lemmy has no channel reordering concept.
    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy: channel reordering is not supported".to_string(),
        ))
    }

    /// Fetch the moderation log for a community.
    ///
    /// Aggregates `removed_posts`, `removed_comments`, and
    /// `banned_from_community` from `GET /api/v3/modlog` and returns them
    /// sorted by timestamp (most recent first), capped at `limit`.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        let community_id = Self::parse_community_id(server_id)?;
        let modlog = self.http.get_modlog(community_id).await?;

        let mut entries: Vec<ModerationLogEntry> = Vec::new();

        for e in &modlog.banned_from_community {
            let action = if e.mod_ban_from_community.banned {
                if e.mod_ban_from_community.expires.is_some() {
                    ModerationAction::MemberTimedOut
                } else {
                    ModerationAction::MemberBanned
                }
            } else {
                ModerationAction::MemberUnbanned
            };
            let moderator = e.moderator.as_ref().map_or_else(
                || User {
                    id: "lemmy-user-unknown".to_string(),
                    display_name: "Unknown".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                map_person,
            );
            entries.push(ModerationLogEntry {
                id: format!("lemmy-modlog-ban-{}", e.mod_ban_from_community.id),
                action,
                moderator,
                target_user_id: Some(format!("lemmy-user-{}", e.banned_person.id)),
                target_display_name: Some(
                    e.banned_person
                        .display_name
                        .clone()
                        .unwrap_or_else(|| e.banned_person.name.clone()),
                ),
                channel_id: None,
                message_id: None,
                reason: e.mod_ban_from_community.reason.clone(),
                timestamp: e.mod_ban_from_community.when_.to_rfc3339(),
            });
        }

        for e in &modlog.removed_posts {
            let moderator = e.moderator.as_ref().map_or_else(
                || User {
                    id: "lemmy-user-unknown".to_string(),
                    display_name: "Unknown".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                map_person,
            );
            entries.push(ModerationLogEntry {
                id: format!("lemmy-modlog-rmpost-{}", e.mod_remove_post.id),
                action: ModerationAction::MessageDeleted,
                moderator,
                target_user_id: None,
                target_display_name: None,
                channel_id: Some(format!(
                    "lemmy-feed-{}",
                    e.community.id
                )),
                message_id: Some(format!("lemmy-post-{}", e.post.id)),
                reason: e.mod_remove_post.reason.clone(),
                timestamp: e.mod_remove_post.when_.to_rfc3339(),
            });
        }

        for e in &modlog.removed_comments {
            let moderator = e.moderator.as_ref().map_or_else(
                || User {
                    id: "lemmy-user-unknown".to_string(),
                    display_name: "Unknown".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                map_person,
            );
            entries.push(ModerationLogEntry {
                id: format!("lemmy-modlog-rmcomment-{}", e.mod_remove_comment.id),
                action: ModerationAction::MessageDeleted,
                moderator,
                target_user_id: Some(format!("lemmy-user-{}", e.commenter.id)),
                target_display_name: Some(
                    e.commenter
                        .display_name
                        .clone()
                        .unwrap_or_else(|| e.commenter.name.clone()),
                ),
                channel_id: Some(format!("lemmy-feed-{}", e.community.id)),
                message_id: Some(format!("lemmy-comment-{}", e.comment.id)),
                reason: e.mod_remove_comment.reason.clone(),
                timestamp: e.mod_remove_comment.when_.to_rfc3339(),
            });
        }

        // Sort most-recent first.
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit);
        Ok(entries)
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported("Lemmy: no role concept".to_string()))
    }
}
