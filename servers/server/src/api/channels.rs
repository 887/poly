use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
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
    let channels = state.db.get_server_channels(&server_id).await?;
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
    let kind_val = serde_json::to_value(req.kind).map_err(|e| AppError::Internal(e.to_string()))?;
    let ch = state
        .db
        .create_channel(
            &server_id,
            req.category_id.as_deref(),
            &req.name,
            kind_val,
            req.position.unwrap_or(0),
        )
        .await?
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
    let ch = state
        .db
        .update_channel(&channel_id, req.name, req.category_id, req.position)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(channel_to_response(ch)))
}

async fn delete_channel(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let ch = get_channel_or_404(&state, &channel_id).await?;
    let server_id = ch.server.clone().ok_or(AppError::Forbidden)?;
    require_owner(&state, &server_id, &auth.user_id).await?;
    state.db.delete_channel(&channel_id).await?;
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
    let cat = state
        .db
        .create_category(&server_id, &req.name, req.position.unwrap_or(0))
        .await?
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
    let updated = state
        .db
        .update_category(&cat_id, req.name, req.position)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(updated))
}

async fn delete_category(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(cat_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let cat = get_category_or_404(&state, &cat_id).await?;
    require_owner(&state, &cat.server, &auth.user_id).await?;
    state.db.delete_category(&cat_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── DMs ────────────────────────────────────────────────────────────────────────

async fn list_dms(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<ChannelResponse>>> {
    let channels = state.db.list_dms(&auth.user_id).await?;
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
    if let Some(ch) = state.db.find_dm(&auth.user_id, &req.user_id).await? {
        return Ok((StatusCode::OK, Json(channel_to_response(ch))));
    }

    // Create new DM channel.
    let ch = state
        .db
        .create_dm_channel(&format!("dm-{}", Uuid::new_v4()))
        .await?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    let ch_id = ch
        .id
        .clone()
        .ok_or_else(|| AppError::Internal("no id".into()))?;

    // Add both participants.
    state
        .db
        .create_participants(&[&auth.user_id, &req.user_id], &ch_id)
        .await?;

    Ok((StatusCode::CREATED, Json(channel_to_response(ch))))
}

// ── Group DMs ─────────────────────────────────────────────────────────────────

async fn create_group(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<(StatusCode, Json<ChannelResponse>)> {
    let ch = state
        .db
        .create_dm_channel(&req.name)
        .await?
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
    let refs: Vec<&str> = all_members.iter().map(|s| s.as_str()).collect();
    state.db.create_participants(&refs, &ch_id).await?;

    Ok((StatusCode::CREATED, Json(channel_to_response(ch))))
}

async fn add_group_member(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(req): Json<AddGroupMemberRequest>,
) -> Result<Json<serde_json::Value>> {
    require_participant(&state, &channel_id, &auth.user_id).await?;
    state
        .db
        .create_participants(&[req.user_id.as_str()], &channel_id)
        .await?;
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
    state.db.delete_participant(&user_id, &channel_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_participants(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>> {
    require_participant(&state, &channel_id, &auth.user_id).await?;
    let parts = state.db.list_participants(&channel_id).await?;
    Ok(Json(parts))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn require_member(state: &AppState, server_id: &str, user_id: &str) -> Result<()> {
    state
        .db
        .get_membership(user_id, server_id)
        .await?
        .map(|_| ())
        .ok_or(AppError::Forbidden)
}

async fn require_owner(state: &AppState, server_id: &str, user_id: &str) -> Result<()> {
    if !state.db.is_server_owner(server_id, user_id).await? {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn require_participant(state: &AppState, channel_id: &str, user_id: &str) -> Result<()> {
    // Check direct participation.
    if state.db.is_participant(user_id, channel_id).await? {
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
    state
        .db
        .get_channel(channel_id)
        .await?
        .ok_or(AppError::NotFound)
}

async fn get_category_or_404(state: &AppState, cat_id: &str) -> Result<Category> {
    state
        .db
        .get_category(cat_id)
        .await?
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
