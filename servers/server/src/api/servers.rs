use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
    models::{Membership, Server},
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

#[derive(Debug, Deserialize)]
struct UpdateServerRequest {
    name: Option<String>,
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
    let servers: Vec<serde_json::Value> = state
        .db
        .query("SELECT server.* FROM membership WHERE user = type::thing($uid) FETCH server")
        .bind(("uid", auth.user_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
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

    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE server CONTENT { \
              name: $name, icon_url: $icon, \
              owner: type::thing($owner), \
              created_at: time::now() \
            } RETURN *",
        )
        .bind(("name", req.name))
        .bind(("icon", req.icon_url))
        .bind(("owner", auth.user_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let server: Server = raw
        .into_iter()
        .next()
        .map(|v| serde_json::from_value::<Server>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
        .ok_or_else(|| AppError::Internal("no server returned".into()))?;

    // Auto-join owner as member.
    if let Some(ref id) = server.id {
        state
            .db
            .query(
                "CREATE membership CONTENT { \
                  user: type::thing($uid), \
                  server: type::thing($sid), \
                  joined_at: time::now() \
                }",
            )
            .bind(("uid", auth.user_id.clone()))
            .bind(("sid", id.clone()))
            .await?
            .check()
            .map_err(AppError::Db)?;
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

    let raw_server: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($id)")
        .bind(("id", id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let server: Server = raw_server
        .map(|v| serde_json::from_value::<Server>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
        .ok_or(AppError::NotFound)?;

    let members: Vec<serde_json::Value> = state
        .db
        .query("SELECT user.* FROM membership WHERE server = type::thing($id) FETCH user")
        .bind(("id", id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let channels: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM channel WHERE server = type::thing($id) ORDER BY position")
        .bind(("id", id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let categories: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM category WHERE server = type::thing($id) ORDER BY position")
        .bind(("id", id))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

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
    Json(req): Json<UpdateServerRequest>,
) -> Result<Json<Server>> {
    require_owner(&state, &auth.user_id, &id).await?;

    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "UPDATE type::thing($id) MERGE { \
              name: $name ?? name, \
              icon_url: $icon ?? icon_url \
            } RETURN *",
        )
        .bind(("id", id))
        .bind(("name", req.name))
        .bind(("icon", req.icon_url))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    raw.map(|v| serde_json::from_value::<Server>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
        .ok_or(AppError::NotFound)
        .map(Json)
}

/// `DELETE /servers/:id` — delete server (owner only).
async fn delete_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_owner(&state, &auth.user_id, &id).await?;
    // TODO(phase-2.2.4.5): cascade-delete channels, memberships, messages.
    state
        .db
        .query("DELETE type::thing($id)")
        .bind(("id", id))
        .await?
        .check()
        .map_err(AppError::Db)?;
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
    state
        .db
        .query(
            "CREATE invite CONTENT { \
              code: $code, server: type::thing($sid), \
              created_by: type::thing($uid), \
              created_at: time::now(), uses: 0 \
            }",
        )
        .bind(("code", code.clone()))
        .bind(("sid", id))
        .bind(("uid", auth.user_id.clone()))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "code": code })))
}

/// `POST /servers/join/:code` — join via invite.
async fn join_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let invite: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM invite \
             WHERE code = $code \
             AND (expires_at IS NONE OR expires_at > time::now()) \
             LIMIT 1",
        )
        .bind(("code", code))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let invite = invite.ok_or(AppError::NotFound)?;
    let server_id = invite
        .get("server")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("invalid invite record".into()))?
        .to_owned();

    state
        .db
        .query(
            "CREATE membership CONTENT { \
              user: type::thing($uid), server: type::thing($sid), \
              joined_at: time::now() \
            } ON DUPLICATE KEY IGNORE",
        )
        .bind(("uid", auth.user_id.clone()))
        .bind(("sid", server_id.clone()))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "server_id": server_id })))
}

/// `DELETE /servers/:id/members/me` — leave server.
async fn leave_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .query(
            "DELETE membership \
             WHERE user = type::thing($uid) AND server = type::thing($sid)",
        )
        .bind(("uid", auth.user_id.clone()))
        .bind(("sid", id))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `DELETE /servers/:id/members/:user_id` — kick a member (owner only).
async fn kick_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((server_id, target_user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    require_owner(&state, &auth.user_id, &server_id).await?;
    state
        .db
        .query(
            "DELETE membership \
             WHERE user = type::thing($uid) AND server = type::thing($sid)",
        )
        .bind(("uid", target_user_id))
        .bind(("sid", server_id))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Auth helpers ──────────────────────────────────────────────────────────────

async fn require_member(state: &AppState, user_id: &str, server_id: &str) -> Result<()> {
    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM membership \
             WHERE user = type::thing($uid) AND server = type::thing($sid) \
             LIMIT 1",
        )
        .bind(("uid", user_id.to_owned()))
        .bind(("sid", server_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|_| ()).ok_or(AppError::Forbidden)
}

async fn require_owner(state: &AppState, user_id: &str, server_id: &str) -> Result<()> {
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($id) LIMIT 1")
        .bind(("id", server_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let server: Server = raw
        .map(|v| serde_json::from_value::<Server>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
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
) -> Result<Membership> {
    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM membership \
             WHERE user = type::thing($uid) AND server = type::thing($sid) \
             LIMIT 1",
        )
        .bind(("uid", user_id.to_owned()))
        .bind(("sid", server_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|v| {
        serde_json::from_value::<Membership>(v).map_err(|e| AppError::Internal(e.to_string()))
    })
    .transpose()?
    .ok_or(AppError::Forbidden)
}
