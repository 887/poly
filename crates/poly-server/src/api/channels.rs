use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
    models::{Category, Channel, ChannelKind, Participant},
};

pub fn router() -> Router<AppState> {
    Router::new()
        // Server channels
        .route(
            "/servers/{server_id}/channels",
            get(list_channels).post(create_channel),
        )
        .route(
            "/channels/{id}",
            patch(update_channel).delete(delete_channel),
        )
        // Categories
        .route("/servers/{server_id}/categories", post(create_category))
        .route(
            "/categories/{id}",
            patch(update_category).delete(delete_category),
        )
        // DMs
        .route("/channels/@dms", get(list_dms).post(open_dm))
        // Group DMs
        .route("/channels/@groups", post(create_group))
        .route(
            "/channels/@groups/{id}/members",
            post(add_group_member).delete(remove_group_member),
        )
        // Participants
        .route("/channels/{id}/participants", get(list_participants))
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateChannelRequest {
    name: String,
    kind: ChannelKind,
    category_id: Option<String>,
    position: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UpdateChannelRequest {
    name: Option<String>,
    category_id: Option<String>,
    position: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateCategoryRequest {
    name: String,
    position: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UpdateCategoryRequest {
    name: Option<String>,
    position: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OpenDmRequest {
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateGroupRequest {
    name: String,
    member_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AddGroupMemberRequest {
    user_id: String,
}

/// Slim response type for channel listing.
#[derive(Debug, Serialize)]
struct ChannelResponse {
    id: String,
    server_id: Option<String>,
    category_id: Option<String>,
    name: String,
    kind: ChannelKind,
    position: i64,
}

// ── Server channels ────────────────────────────────────────────────────────────

async fn list_channels(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
) -> Result<Json<Vec<ChannelResponse>>> {
    require_member(&state, &server_id, &auth.user_id).await?;
    let raw: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM channel WHERE server = type::thing($sid) ORDER BY position")
        .bind(("sid", server_id))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let channels = from_values::<Channel>(raw)?;
    Ok(Json(
        channels.into_iter().map(channel_to_response).collect(),
    ))
}

async fn create_channel(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>)> {
    require_owner(&state, &server_id, &auth.user_id).await?;
    let now = Utc::now();
    let kind_val = serde_json::to_value(req.kind).map_err(|e| AppError::Internal(e.to_string()))?;
    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE channel CONTENT { \
              server: type::thing($sid), \
              category: $cat, \
              name: $name, \
              kind: $kind, \
              position: $pos, \
              created_at: $now \
            } RETURN *",
        )
        .bind(("sid", server_id))
        .bind(("cat", req.category_id.map(|c| format!("category:{c}"))))
        .bind(("name", req.name))
        .bind(("kind", kind_val))
        .bind(("pos", req.position.unwrap_or(0)))
        .bind(("now", now))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let ch: Channel = raw
        .into_iter()
        .next()
        .map(|v| {
            serde_json::from_value::<Channel>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    Ok((StatusCode::CREATED, Json(channel_to_response(ch))))
}

async fn update_channel(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<ChannelResponse>> {
    let ch = get_channel_or_404(&state, &channel_id).await?;
    let server_id = ch.server.clone().ok_or(AppError::Forbidden)?;
    require_owner(&state, &server_id, &auth.user_id).await?;
    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "UPDATE type::thing($id) MERGE { \
              name: $nm ?? name, \
              category: $cat ?? category, \
              position: $pos ?? position \
            } RETURN *",
        )
        .bind(("id", channel_id))
        .bind(("nm", req.name))
        .bind(("cat", req.category_id.map(|c| format!("category:{c}"))))
        .bind(("pos", req.position))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|v| {
        serde_json::from_value::<Channel>(v)
            .map(channel_to_response)
            .map_err(|e| AppError::Internal(e.to_string()))
    })
    .transpose()?
    .ok_or(AppError::NotFound)
    .map(Json)
}

async fn delete_channel(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let ch = get_channel_or_404(&state, &channel_id).await?;
    let server_id = ch.server.clone().ok_or(AppError::Forbidden)?;
    require_owner(&state, &server_id, &auth.user_id).await?;
    state
        .db
        .query("DELETE type::thing($id)")
        .bind(("id", channel_id))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Categories ─────────────────────────────────────────────────────────────────

async fn create_category(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<Category>)> {
    require_owner(&state, &server_id, &auth.user_id).await?;
    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE category CONTENT { \
              server: type::thing($sid), name: $name, position: $pos \
            } RETURN *",
        )
        .bind(("sid", server_id))
        .bind(("name", req.name))
        .bind(("pos", req.position.unwrap_or(0)))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let cat: Category = raw
        .into_iter()
        .next()
        .map(|v| {
            serde_json::from_value::<Category>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    Ok((StatusCode::CREATED, Json(cat)))
}

async fn update_category(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(cat_id): Path<String>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Result<Json<Category>> {
    let cat = get_category_or_404(&state, &cat_id).await?;
    require_owner(&state, &cat.server, &auth.user_id).await?;
    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "UPDATE type::thing($id) MERGE { \
              name: $nm ?? name, position: $pos ?? position \
            } RETURN *",
        )
        .bind(("id", cat_id))
        .bind(("nm", req.name))
        .bind(("pos", req.position))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|v| {
        serde_json::from_value::<Category>(v).map_err(|e| AppError::Internal(e.to_string()))
    })
    .transpose()?
    .ok_or(AppError::NotFound)
    .map(Json)
}

async fn delete_category(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(cat_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let cat = get_category_or_404(&state, &cat_id).await?;
    require_owner(&state, &cat.server, &auth.user_id).await?;
    state
        .db
        .query(
            "UPDATE channel SET category = NONE WHERE category = type::thing($id); \
             DELETE type::thing($id)",
        )
        .bind(("id", cat_id))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── DMs ────────────────────────────────────────────────────────────────────────

async fn list_dms(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<ChannelResponse>>> {
    // Get all channel IDs the user participates in, then filter to DMs (server IS NONE).
    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM channel WHERE \
             id IN (SELECT channel FROM participant WHERE user = type::thing($uid)) \
             AND server IS NONE",
        )
        .bind(("uid", auth.user_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let channels = from_values::<Channel>(raw)?;
    Ok(Json(
        channels.into_iter().map(channel_to_response).collect(),
    ))
}

async fn open_dm(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<OpenDmRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>)> {
    if req.user_id == auth.user_id {
        return Err(AppError::BadRequest("cannot DM yourself".into()));
    }

    // Check if DM already exists between these two users.
    let raw_existing: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM channel WHERE server IS NONE \
             AND id IN (SELECT channel FROM participant WHERE user = type::thing($me)) \
             AND id IN (SELECT channel FROM participant WHERE user = type::thing($them)) \
             LIMIT 1",
        )
        .bind(("me", auth.user_id.clone()))
        .bind(("them", req.user_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    if let Some(v) = raw_existing {
        let ch =
            serde_json::from_value::<Channel>(v).map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok((StatusCode::OK, Json(channel_to_response(ch))));
    }

    // Create new DM channel.
    let now = Utc::now();
    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE channel CONTENT { \
              server: NONE, category: NONE, name: $name, \
              kind: 'text', position: 0, created_at: $now \
            } RETURN *",
        )
        .bind(("name", format!("dm-{}", Uuid::new_v4())))
        .bind(("now", now))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let ch: Channel = raw
        .into_iter()
        .next()
        .map(|v| {
            serde_json::from_value::<Channel>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    let ch_id = ch
        .id
        .clone()
        .ok_or_else(|| AppError::Internal("no id".into()))?;

    // Add both participants.
    state
        .db
        .query(
            "CREATE participant CONTENT { \
              user: type::thing($me), channel: type::thing($ch), added_at: $now \
            }; \
            CREATE participant CONTENT { \
              user: type::thing($them), channel: type::thing($ch), added_at: $now \
            }",
        )
        .bind(("me", auth.user_id.clone()))
        .bind(("them", req.user_id.clone()))
        .bind(("ch", ch_id))
        .bind(("now", now))
        .await?
        .check()
        .map_err(AppError::Db)?;

    Ok((StatusCode::CREATED, Json(channel_to_response(ch))))
}

// ── Group DMs ─────────────────────────────────────────────────────────────────

async fn create_group(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>)> {
    let now = Utc::now();
    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE channel CONTENT { \
              server: NONE, category: NONE, name: $name, \
              kind: 'text', position: 0, created_at: $now \
            } RETURN *",
        )
        .bind(("name", req.name))
        .bind(("now", now))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let ch: Channel = raw
        .into_iter()
        .next()
        .map(|v| {
            serde_json::from_value::<Channel>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    let ch_id = ch
        .id
        .clone()
        .ok_or_else(|| AppError::Internal("no id".into()))?;

    // Add creator + specified members.
    let mut all_members = req.member_ids;
    if !all_members.contains(&auth.user_id) {
        all_members.push(auth.user_id.clone());
    }
    for uid in &all_members {
        state
            .db
            .query(
                "CREATE participant CONTENT { \
                  user: type::thing($uid), channel: type::thing($ch), added_at: $now \
                }",
            )
            .bind(("uid", uid.clone()))
            .bind(("ch", ch_id.clone()))
            .bind(("now", now))
            .await?
            .check()
            .map_err(AppError::Db)?;
    }

    Ok((StatusCode::CREATED, Json(channel_to_response(ch))))
}

async fn add_group_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(req): Json<AddGroupMemberRequest>,
) -> Result<Json<serde_json::Value>> {
    require_participant(&state, &channel_id, &auth.user_id).await?;
    let now = Utc::now();
    state
        .db
        .query(
            "CREATE participant CONTENT { \
              user: type::thing($uid), channel: type::thing($ch), added_at: $now \
            }",
        )
        .bind(("uid", req.user_id))
        .bind(("ch", channel_id))
        .bind(("now", now))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn remove_group_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((channel_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    // Simple policy: only self-removal.
    if user_id != auth.user_id {
        return Err(AppError::Forbidden);
    }
    state
        .db
        .query(
            "DELETE participant \
             WHERE user = type::thing($uid) AND channel = type::thing($ch)",
        )
        .bind(("uid", user_id))
        .bind(("ch", channel_id))
        .await?
        .check()
        .map_err(AppError::Db)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_participants(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>> {
    require_participant(&state, &channel_id, &auth.user_id).await?;
    let parts: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM participant WHERE channel = type::thing($ch)")
        .bind(("ch", channel_id))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    Ok(Json(parts))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn require_member(state: &AppState, server_id: &str, user_id: &str) -> Result<()> {
    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM membership \
             WHERE server = type::thing($sid) AND user = type::thing($uid) LIMIT 1",
        )
        .bind(("sid", server_id.to_owned()))
        .bind(("uid", user_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|_| ()).ok_or(AppError::Forbidden)
}

async fn require_owner(state: &AppState, server_id: &str, user_id: &str) -> Result<()> {
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($sid) WHERE owner = type::thing($uid) LIMIT 1")
        .bind(("sid", server_id.to_owned()))
        .bind(("uid", user_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|_| ()).ok_or(AppError::Forbidden)
}

async fn require_participant(state: &AppState, channel_id: &str, user_id: &str) -> Result<()> {
    // Check direct participation.
    let raw: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM participant \
             WHERE channel = type::thing($ch) AND user = type::thing($uid) LIMIT 1",
        )
        .bind(("ch", channel_id.to_owned()))
        .bind(("uid", user_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if raw.is_some() {
        return Ok(());
    }
    // Also allow server members of the channel's server.
    let ch = get_channel_or_404(state, channel_id).await?;
    if let Some(server_id) = ch.server {
        return require_member(state, &server_id, user_id).await;
    }
    Err(AppError::Forbidden)
}

async fn get_channel_or_404(state: &AppState, channel_id: &str) -> Result<Channel> {
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($id) LIMIT 1")
        .bind(("id", channel_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|v| serde_json::from_value::<Channel>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
        .ok_or(AppError::NotFound)
}

async fn get_category_or_404(state: &AppState, cat_id: &str) -> Result<Category> {
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($id) LIMIT 1")
        .bind(("id", cat_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|v| {
        serde_json::from_value::<Category>(v).map_err(|e| AppError::Internal(e.to_string()))
    })
    .transpose()?
    .ok_or(AppError::NotFound)
}

fn channel_to_response(ch: Channel) -> ChannelResponse {
    ChannelResponse {
        id: ch.id.clone().unwrap_or_default(),
        server_id: ch.server.clone(),
        category_id: ch.category.clone(),
        name: ch.name,
        kind: ch.kind,
        position: ch.position,
    }
}

/// Deserialise a `Vec<serde_json::Value>` into `Vec<T>`.
fn from_values<T: serde::de::DeserializeOwned>(raw: Vec<serde_json::Value>) -> Result<Vec<T>> {
    raw.into_iter()
        .map(|v| serde_json::from_value::<T>(v).map_err(|e| AppError::Internal(e.to_string())))
        .collect()
}

/// Check user can access a channel — used by callers outside this module.
pub async fn assert_channel_access(
    state: &AppState,
    channel_id: &str,
    user_id: &str,
) -> Result<Channel> {
    let ch = get_channel_or_404(state, channel_id).await?;
    if ch.server.is_some() {
        require_member(state, ch.server.as_deref().unwrap_or(""), user_id).await?;
    } else {
        require_participant(state, channel_id, user_id).await?;
    }
    Ok(ch)
}

// Suppress unused import warning — Participant is used in list_participants signature type.
const _: fn() = || {
    let _ = std::mem::size_of::<Participant>();
};
