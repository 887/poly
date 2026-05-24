//! Social-policy routes: blocks, ignores, friend requests (by ID), relationship
//! metadata (nickname, note), conversation mutes, group-DM lifecycle, and
//! targeted server invites.

use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, patch, post},
};
use serde::Deserialize;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
};

pub fn router() -> Router<AppState> {
    // axum 0.7+ path capture syntax: `{capture}` (the old `:capture`
    // syntax now panics at router-build time).
    Router::new()
        // Blocks
        .route("/api/v1/relationships/block", post(block_user))
        .route("/api/v1/relationships/block/{user_id}", delete(unblock_user))
        // Ignores
        .route("/api/v1/relationships/ignore", post(ignore_user))
        .route("/api/v1/relationships/ignore/{user_id}", delete(unignore_user))
        // Friends (by user ID, complementing the existing username-based flow)
        .route("/api/v1/relationships/friend", post(add_friend))
        .route("/api/v1/relationships/friend/{user_id}", delete(remove_friend))
        // Relationship metadata
        .route("/api/v1/relationships/{user_id}/nickname", patch(set_nickname))
        .route("/api/v1/relationships/{user_id}/note", patch(set_note))
        // DM close (hide from list)
        .route("/api/v1/dm/{channel_id}/close", post(close_dm))
        // Conversation mute
        .route("/api/v1/conversation/{channel_id}/mute", post(mute_conversation))
        .route(
            "/api/v1/conversation/{channel_id}/mute",
            delete(unmute_conversation),
        )
        // Group DM
        .route("/api/v1/group-dm/{channel_id}/leave", post(leave_group_dm))
        .route("/api/v1/group-dm/{channel_id}", patch(edit_group_dm))
        .route("/api/v1/group-dm/{channel_id}/members", post(add_group_dm_members))
        // Server invite (targeted)
        .route("/api/v1/server/{server_id}/invite-user", post(invite_user_to_server))
}

// ── Request types ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UserIdBody {
    user_id: String,
}

#[derive(Debug, Deserialize)]
struct NicknameBody {
    nickname: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NoteBody {
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MuteBody {
    /// RFC-3339 timestamp; absent means "indefinitely".
    until: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EditGroupDmBody {
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AddGroupDmMembersBody {
    user_ids: Vec<String>,
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn block_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<UserIdBody>,
) -> Result<Json<serde_json::Value>> {
    if body.user_id == auth.user_id {
        return Err(AppError::BadRequest("cannot block yourself".into()));
    }
    state.db.block_user(&auth.user_id, &body.user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn unblock_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state.db.unblock_user(&auth.user_id, &user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn ignore_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<UserIdBody>,
) -> Result<Json<serde_json::Value>> {
    if body.user_id == auth.user_id {
        return Err(AppError::BadRequest("cannot ignore yourself".into()));
    }
    state.db.ignore_user(&auth.user_id, &body.user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn unignore_user(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state.db.unignore_user(&auth.user_id, &user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn add_friend(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(body): Json<UserIdBody>,
) -> Result<Json<serde_json::Value>> {
    if body.user_id == auth.user_id {
        return Err(AppError::BadRequest("cannot friend yourself".into()));
    }
    // Verify target exists.
    state
        .db
        .get_user(&body.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    state
        .db
        .create_friend_request_by_id(&auth.user_id, &body.user_id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn remove_friend(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state.db.remove_friend(&auth.user_id, &user_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_nickname(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<String>,
    Json(body): Json<NicknameBody>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .set_relationship_nickname(&auth.user_id, &user_id, body.nickname.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn set_note(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<String>,
    Json(body): Json<NoteBody>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .set_user_note(&auth.user_id, &user_id, body.note.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn close_dm(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .close_dm_channel(&auth.user_id, &channel_id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn mute_conversation(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(body): Json<MuteBody>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .mute_conversation(&auth.user_id, &channel_id, body.until.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn unmute_conversation(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .unmute_conversation(&auth.user_id, &channel_id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn leave_group_dm(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // Verify the user is actually a participant.
    let is_p = state.db.is_participant(&auth.user_id, &channel_id).await?;
    if !is_p {
        return Err(AppError::Forbidden);
    }
    state
        .db
        .delete_participant(&auth.user_id, &channel_id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn edit_group_dm(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(body): Json<EditGroupDmBody>,
) -> Result<Json<serde_json::Value>> {
    let is_p = state.db.is_participant(&auth.user_id, &channel_id).await?;
    if !is_p {
        return Err(AppError::Forbidden);
    }
    state
        .db
        .update_group_dm(&channel_id, body.name.as_deref(), body.avatar_url.as_deref())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn add_group_dm_members(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(body): Json<AddGroupDmMembersBody>,
) -> Result<Json<serde_json::Value>> {
    let is_p = state.db.is_participant(&auth.user_id, &channel_id).await?;
    if !is_p {
        return Err(AppError::Forbidden);
    }
    let refs: Vec<&str> = body.user_ids.iter().map(String::as_str).collect();
    state.db.create_participants(&refs, &channel_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn invite_user_to_server(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(server_id): Path<String>,
    Json(body): Json<UserIdBody>,
) -> Result<Json<serde_json::Value>> {
    // Require the caller to be a member of the server.
    state
        .db
        .get_membership(&auth.user_id, &server_id)
        .await?
        .ok_or(AppError::Forbidden)?;
    // Verify target user exists.
    state
        .db
        .get_user(&body.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    state
        .db
        .create_user_invite(&server_id, &auth.user_id, &body.user_id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
