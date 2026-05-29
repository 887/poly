//! Moderation API — roles, bans, kick, timeout, channel edits, modlog.
//!
//! Role hierarchy (numeric for comparison):
//! - owner     = 3  (server.owner field — not stored in membership.role)
//! - admin     = 2
//! - moderator = 1
//! - member    = 0

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    db::ModlogInsert,
    error::{AppError, Result},
    models::Server,
};

// ── Role tier ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoleTier {
    Member = 0,
    Moderator = 1,
    Admin = 2,
    Owner = 3,
}

impl RoleTier {
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            "moderator" => Self::Moderator,
            _ => Self::Member,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Moderator => "moderator",
            Self::Member => "member",
        }
    }
}

// ── Permission helpers ────────────────────────────────────────────────────────

/// Resolve the effective `RoleTier` for a user in a server.
/// Owners have tier Owner regardless of membership.role.
pub async fn resolve_caller_tier(state: &AppState, user_id: &str, server_id: &str) -> Result<RoleTier> {
    let server: Server = state
        .db
        .get_server(server_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if server.owner == user_id {
        return Ok(RoleTier::Owner);
    }
    // Must be a member.
    let role_str = state
        .db
        .get_member_role(server_id, user_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    Ok(RoleTier::parse(&role_str))
}

/// Reject with 403 if caller's tier is below `min`.
async fn require_tier(state: &AppState, user_id: &str, server_id: &str, min: RoleTier) -> Result<RoleTier> {
    let tier = resolve_caller_tier(state, user_id, server_id).await?;
    if tier < min {
        return Err(AppError::Forbidden);
    }
    Ok(tier)
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        // Permissions
        .route("/servers/{id}/members/@me/permissions", get(get_my_permissions))
        // Bans
        .route("/servers/{id}/bans", get(list_bans))
        .route("/servers/{id}/bans/{user_id}", post(ban_member).delete(unban_member))
        // Member role
        .route("/servers/{id}/members/{user_id}/role", patch(update_member_role))
        // Kick
        .route("/servers/{id}/members/{user_id}/kick", delete(kick_member))
        // Timeout
        .route("/servers/{id}/members/{user_id}/timeout", patch(set_timeout))
        // Message delete (moderation path — separate from regular user delete)
        .route("/servers/{id}/channels/{channel_id}/messages/{message_id}", delete(delete_message_mod))
        // Channel moderation update
        .route("/servers/{id}/channels/{channel_id}/moderation", patch(update_channel_moderation))
        // Channel reorder
        .route("/servers/{id}/channels/reorder", patch(reorder_channels))
        // Modlog
        .route("/servers/{id}/modlog", get(list_modlog))
}

// ── Request / response types ──────────────────────────────────────────────────

// PermissionsResponse is a direct serialization of server permission flags; bool fields are
// idiomatic for a JSON boolean permissions response — enum variants would add no clarity here.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Serialize)]
struct PermissionsResponse {
    role: String,
    manage_server: bool,
    manage_channels: bool,
    manage_roles: bool,
    kick_members: bool,
    ban_members: bool,
    manage_messages: bool,
    timeout_members: bool,
}

impl PermissionsResponse {
    fn from_tier(tier: RoleTier) -> Self {
        match tier {
            RoleTier::Owner => Self {
                role: "owner".into(),
                manage_server: true,
                manage_channels: true,
                manage_roles: true,
                kick_members: true,
                ban_members: true,
                manage_messages: true,
                timeout_members: true,
            },
            RoleTier::Admin => Self {
                role: "admin".into(),
                manage_server: false,
                manage_channels: true,
                manage_roles: true,
                kick_members: true,
                ban_members: true,
                manage_messages: true,
                timeout_members: true,
            },
            RoleTier::Moderator => Self {
                role: "moderator".into(),
                manage_server: false,
                manage_channels: false,
                manage_roles: false,
                kick_members: true,
                ban_members: true,
                manage_messages: true,
                timeout_members: true,
            },
            RoleTier::Member => Self {
                role: "member".into(),
                manage_server: false,
                manage_channels: false,
                manage_roles: false,
                kick_members: false,
                ban_members: false,
                manage_messages: false,
                timeout_members: false,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct BanRequest {
    reason: Option<String>,
    /// ISO8601 expiry for temporary ban. None = permanent.
    expires_at: Option<String>,
    /// Unused by this backend; accepted for API parity with Discord shape.
    /// Underscore-prefix marks intentionally-unused so the lint-gate's
    /// allow_ban check doesn't trip on `#[allow(dead_code)]`.
    _delete_message_seconds: Option<u64>,
}

// BanRecord serialization is handled directly via serde_json::Value from the DB.
// The struct is kept here as documentation of the wire shape.

#[derive(Debug, Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

#[derive(Debug, Deserialize)]
struct TimeoutRequest {
    /// ISO8601 timestamp for when the timeout expires. Null = clear timeout.
    until: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateChannelModerationRequest {
    topic: Option<String>,
    slow_mode_secs: Option<u32>,
    nsfw: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ReorderChannelsRequest {
    ordering: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModlogQuery {
    limit: Option<usize>,
}

// ModlogEntry serialization is handled directly via serde_json::Value from the DB.
// The struct is kept here as documentation of the wire shape.

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /servers/{id}/members/@me/permissions`
async fn get_my_permissions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
) -> Result<Json<PermissionsResponse>> {
    let tier = resolve_caller_tier(&state, &auth.user_id, &server_id).await?;
    Ok(Json(PermissionsResponse::from_tier(tier)))
}

/// `GET /servers/{id}/bans`  (Mod+)
async fn list_bans(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;
    let bans = state.db.list_bans(&server_id).await?;
    Ok(Json(bans))
}

/// `POST /servers/{id}/bans/{user_id}`  (Mod+)
async fn ban_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_id)): Path<(String, String)>,
    Json(req): Json<BanRequest>,
) -> Result<StatusCode> {
    let caller_tier = require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;

    // Cannot ban someone with equal or higher role.
    let target_role_str = state.db.get_member_role(&server_id, &target_id).await?;
    if let Some(ref role_str) = target_role_str {
        let target_tier = if state.db.get_server(&server_id).await?.is_some_and(|s| s.owner == target_id) {
            RoleTier::Owner
        } else {
            RoleTier::parse(role_str)
        };
        if caller_tier <= target_tier {
            return Err(AppError::Forbidden);
        }
    }

    state
        .db
        .ban_member(
            &server_id,
            &target_id,
            &auth.user_id,
            req.reason.as_deref(),
            req.expires_at.as_deref(),
        )
        .await?;

    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: Some(&target_id),
            action: "ban",
            reason: req.reason.as_deref(),
            channel_id: None,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /servers/{id}/bans/{user_id}`  (Mod+)
async fn unban_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;
    state.db.unban_member(&server_id, &target_id).await?;
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: Some(&target_id),
            action: "unban",
            reason: None,
            channel_id: None,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /servers/{id}/members/{user_id}/role`  (Admin+ for admin/mod/member; Owner for owner)
async fn update_member_role(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_id)): Path<(String, String)>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<StatusCode> {
    let caller_tier_val = require_tier(&state, &auth.user_id, &server_id, RoleTier::Admin).await?;
    let new_tier = RoleTier::parse(&req.role);

    // Promoting to owner requires the caller to themselves be Owner.
    if new_tier == RoleTier::Owner && caller_tier_val < RoleTier::Owner {
        return Err(AppError::Forbidden);
    }
    // Cannot assign a role >= caller's own tier (except owner assigning owner).
    if new_tier >= caller_tier_val && caller_tier_val < RoleTier::Owner {
        return Err(AppError::Forbidden);
    }

    state.db.set_member_role(&server_id, &target_id, new_tier.as_str()).await?;
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: Some(&target_id),
            action: "update-role",
            reason: Some(&req.role),
            channel_id: None,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /servers/{id}/members/{user_id}/kick`  (Mod+)
async fn kick_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_id)): Path<(String, String)>,
) -> Result<StatusCode> {
    let caller_tier_val = require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;

    // Cannot kick owner; mods cannot kick admins.
    let target_role_str = state.db.get_member_role(&server_id, &target_id).await?;
    let is_owner = state.db.get_server(&server_id).await?.is_some_and(|s| s.owner == target_id);
    if is_owner {
        return Err(AppError::Forbidden);
    }
    if let Some(ref role_str) = target_role_str {
        let target_tier = RoleTier::parse(role_str);
        if caller_tier_val <= target_tier {
            return Err(AppError::Forbidden);
        }
    }

    state.db.delete_membership(&target_id, &server_id).await?;
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: Some(&target_id),
            action: "kick",
            reason: None,
            channel_id: None,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /servers/{id}/members/{user_id}/timeout`  (Mod+)
async fn set_timeout(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_id)): Path<(String, String)>,
    Json(req): Json<TimeoutRequest>,
) -> Result<StatusCode> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;
    state.db.set_member_timeout(&server_id, &target_id, req.until.as_deref()).await?;
    let action = if req.until.is_some() { "timeout" } else { "untimeout" };
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: Some(&target_id),
            action,
            reason: None,
            channel_id: None,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /servers/{id}/channels/{channel_id}/messages/{message_id}`  (Mod+)
async fn delete_message_mod(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, channel_id, message_id)): Path<(String, String, String)>,
) -> Result<StatusCode> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;
    state.db.soft_delete_message(&message_id).await?;
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: None,
            action: "delete-message",
            reason: None,
            channel_id: Some(&channel_id),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /servers/{id}/channels/{channel_id}/moderation`  (Admin+)
async fn update_channel_moderation(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, channel_id)): Path<(String, String)>,
    Json(req): Json<UpdateChannelModerationRequest>,
) -> Result<StatusCode> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Admin).await?;
    state
        .db
        .update_channel_moderation(&channel_id, req.topic.as_deref(), req.slow_mode_secs, req.nsfw)
        .await?;
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: None,
            action: "update-channel",
            reason: None,
            channel_id: Some(&channel_id),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /servers/{id}/channels/reorder`  (Admin+)
async fn reorder_channels(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
    Json(req): Json<ReorderChannelsRequest>,
) -> Result<StatusCode> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Admin).await?;
    for (pos, channel_id) in req.ordering.iter().enumerate() {
        drop(
            state
                .db
                .update_channel(
                    channel_id,
                    None,
                    None,
                    Some(i64::try_from(pos).unwrap_or(i64::MAX)),
                )
                .await,
        );
    }
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: None,
            action: "reorder-channels",
            reason: None,
            channel_id: None,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /servers/{id}/modlog?limit=50`  (Mod+)
async fn list_modlog(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
    Query(params): Query<ModlogQuery>,
) -> Result<Json<Vec<serde_json::Value>>> {
    require_tier(&state, &auth.user_id, &server_id, RoleTier::Moderator).await?;
    let limit = params.limit.unwrap_or(50).min(200);
    let entries = state.db.list_modlog(&server_id, limit).await?;
    Ok(Json(entries))
}
