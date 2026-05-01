use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    db::ModlogInsert,
    error::{AppError, Result},
    models::Server,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/servers", get(list_servers).post(create_server))
        .route(
            "/servers/{id}",
            get(get_server).patch(update_server).delete(delete_server),
        )
        .route("/servers/{id}/invite", post(create_invite))
        .route("/servers/join/{code}", post(join_server))
        .route("/servers/{id}/members/me", delete(leave_server))
        .route("/servers/{id}/members/{user_id}", delete(kick_member))
}

// ── Request / response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateServerRequest {
    name: String,
    icon_url: Option<String>,
}


#[derive(Debug, Serialize)]
struct ServerDetail {
    server: Server,
    members: Vec<serde_json::Value>,
    channels: Vec<serde_json::Value>,
    categories: Vec<serde_json::Value>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /servers` — servers current user is a member of.
async fn list_servers(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let servers = state.db.list_servers_for_user(&auth.user_id).await?;
    Ok(Json(servers))
}

/// `POST /servers` — create a new server.
async fn create_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateServerRequest>,
) -> Result<Json<Server>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("server name required".into()));
    }

    let raw = state
        .db
        .create_server_record(&req.name, req.icon_url.as_deref(), &auth.user_id)
        .await?;

    let server: Server = raw
        .into_iter()
        .next()
        .map(|v| serde_json::from_value::<Server>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
        .ok_or_else(|| AppError::Internal("no server returned".into()))?;

    // Auto-join owner as member.
    if let Some(ref id) = server.id {
        state.db.create_membership(&auth.user_id, id).await?;
    }

    Ok(Json(server))
}

/// `GET /servers/:id` — full server detail.
async fn get_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<ServerDetail>> {
    require_member(&state, &auth.user_id, &id).await?;

    let server: Server = state
        .db
        .get_server(&id)
        .await?
        .ok_or(AppError::NotFound)?;

    let members = state.db.get_server_members(&id).await?;

    let channels: Vec<serde_json::Value> = state
        .db
        .get_server_channels(&id)
        .await?
        .into_iter()
        .map(|ch| serde_json::to_value(ch).unwrap_or_default())
        .collect();

    let categories = state.db.get_server_categories(&id).await?;

    Ok(Json(ServerDetail {
        server,
        members,
        channels,
        categories,
    }))
}

/// `PATCH /servers/:id` — update server (owner only).
async fn update_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
    // Use raw Value so we can distinguish "field absent" vs "field = null".
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Server>> {
    require_owner(&state, &auth.user_id, &id).await?;

    let name: Option<String> = body.get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let icon_url: Option<String> = body.get("icon_url")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // banner_url: field absent → None (no change); field present with null → Some(None) (clear);
    //             field present with string → Some(Some(url)) (set).
    let banner_url: Option<Option<String>> = body.get("banner_url").map(|v| {
        if v.is_null() { None } else { v.as_str().map(str::to_string) }
    });

    let server: Server = state
        .db
        .update_server(&id, name, icon_url, banner_url)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(server))
}

/// `DELETE /servers/:id` — delete server (owner only).
async fn delete_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_owner(&state, &auth.user_id, &id).await?;
    state.db.delete_server_cascade(&id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `POST /servers/:id/invite` — create an invite code.
async fn create_invite(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_member(&state, &auth.user_id, &id).await?;
    let code = uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(10)
        .collect::<String>();
    state.db.create_invite(&code, &id, &auth.user_id).await?;
    Ok(Json(serde_json::json!({ "code": code })))
}

/// `POST /servers/join/:code` — join via invite.
async fn join_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let invite: serde_json::Value = state
        .db
        .get_valid_invite(&code)
        .await?
        .ok_or(AppError::NotFound)?;
    let server_id = invite
        .get("server")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("invalid invite record".into()))?
        .to_owned();

    // Check if already a member.
    let existing = state.db.get_membership(&auth.user_id, &server_id).await?;
    if existing.is_none() {
        state.db.create_membership(&auth.user_id, &server_id).await?;
    }
    Ok(Json(serde_json::json!({ "server_id": server_id })))
}

/// `DELETE /servers/:id/members/me` — leave server.
async fn leave_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state.db.delete_membership(&auth.user_id, &id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `DELETE /servers/:id/members/:user_id` — kick a member (Mod+ required).
///
/// Delegates to the moderation tier system. Kept at this path for backward
/// compatibility with the existing HTTP client.
async fn kick_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    use crate::api::moderation::RoleTier;

    let caller_tier = crate::api::moderation::resolve_caller_tier(&state, &auth.user_id, &server_id).await?;
    if caller_tier < RoleTier::Moderator {
        return Err(AppError::Forbidden);
    }

    // Cannot kick owner; mods cannot kick admins.
    let is_target_owner = state.db.get_server(&server_id).await?.is_some_and(|s| s.owner == target_user_id);
    if is_target_owner {
        return Err(AppError::Forbidden);
    }
    if let Some(role_str) = state.db.get_member_role(&server_id, &target_user_id).await? {
        let target_tier = RoleTier::parse(&role_str);
        if caller_tier <= target_tier {
            return Err(AppError::Forbidden);
        }
    }

    state.db.delete_membership(&target_user_id, &server_id).await?;
    state
        .db
        .append_modlog(ModlogInsert {
            server_id: &server_id,
            actor_id: &auth.user_id,
            target_id: Some(&target_user_id),
            action: "kick",
            reason: None,
            channel_id: None,
        })
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

async fn require_member(state: &AppState, user_id: &str, server_id: &str) -> Result<()> {
    state
        .db
        .get_membership(user_id, server_id)
        .await?
        .map(|_| ())
        .ok_or(AppError::Forbidden)
}

async fn require_owner(state: &AppState, user_id: &str, server_id: &str) -> Result<()> {
    let server: Server = state
        .db
        .get_server(server_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if server.owner != user_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Check `user_id` is a member; return the `Membership` row.
pub async fn get_membership(
    state: &AppState,
    user_id: &str,
    server_id: &str,
) -> Result<crate::models::Membership> {
    state
        .db
        .get_membership(user_id, server_id)
        .await?
        .ok_or(AppError::Forbidden)
}
