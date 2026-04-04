use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{get, patch},
};
use serde::Deserialize;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
    models::{FriendRequest, UserProfile, UserRecord},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users/me", get(me).patch(update_me))
        .route("/users/{id}", get(get_user))
        .route(
            "/users/me/friends",
            get(list_friends).post(send_friend_request),
        )
        .route(
            "/users/me/friends/{id}",
            patch(respond_friend_request).delete(remove_friend),
        )
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct UpdateMeRequest {
    display_name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendFriendRequest {
    username: String,
}

#[derive(Debug, Deserialize)]
struct RespondFriendRequest {
    status: String, // "accepted" | "rejected"
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn me(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<UserProfile>> {
    let user: UserRecord = state
        .db
        .get_user(&auth.user_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(user_to_self_profile(user)?))
}

async fn update_me(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<UserProfile>> {
    let user: UserRecord = state
        .db
        .update_user(&auth.user_id, req.display_name, req.avatar_url)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(user_to_profile(user)?))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<UserProfile>> {
    let user: UserRecord = state
        .db
        .get_user(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(user_to_profile(user)?))
}

async fn list_friends(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<UserProfile>>> {
    let records = state.db.list_friends_raw(&auth.user_id).await?;

    let profiles: Vec<UserProfile> = records
        .into_iter()
        .filter_map(|v| {
            let from_id = v.get("from")?.get("id")?.as_str()?.to_owned();
            let is_from = from_id == auth.user_id;
            let side = if is_from {
                v.get("to")?
            } else {
                v.get("from")?
            };
            Some(UserProfile {
                id: side.get("id")?.as_str()?.to_owned(),
                username: side.get("username")?.as_str()?.to_owned(),
                email: None,
                display_name: side.get("display_name")?.as_str()?.to_owned(),
                avatar_url: side
                    .get("avatar_url")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
            })
        })
        .collect();

    Ok(Json(profiles))
}

async fn send_friend_request(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<SendFriendRequest>,
) -> Result<Json<FriendRequest>> {
    let target: UserRecord = state
        .db
        .get_user_by_username(&req.username)
        .await?
        .ok_or(AppError::NotFound)?;
    let target_id = target
        .id
        .ok_or_else(|| AppError::Internal("missing id".into()))?;

    let created: FriendRequest = state
        .db
        .create_friend_request(&auth.user_id, &target_id)
        .await?
        .ok_or_else(|| AppError::Internal("no record returned".into()))?;
    Ok(Json(created))
}

async fn respond_friend_request(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(request_id): Path<String>,
    Json(req): Json<RespondFriendRequest>,
) -> Result<Json<FriendRequest>> {
    let status = req.status.as_str();
    if status != "accepted" && status != "rejected" {
        return Err(AppError::BadRequest(
            "status must be accepted or rejected".into(),
        ));
    }
    // Verify this user is the recipient.
    let fr: FriendRequest = state
        .db
        .get_friend_request(&request_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if fr.to != auth.user_id {
        return Err(AppError::Forbidden);
    }

    let updated: FriendRequest = state
        .db
        .update_friend_request_status(&request_id, status)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(updated))
}

async fn remove_friend(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(target_user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state
        .db
        .remove_friend(&auth.user_id, &target_user_id)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Convert a `UserRecord` to its public `UserProfile`.
pub fn user_to_profile(user: UserRecord) -> Result<UserProfile> {
    let id = user
        .id
        .ok_or_else(|| AppError::Internal("missing user id".into()))?;
    Ok(UserProfile {
        id,
        username: user.username,
        email: None,
        display_name: user.display_name,
        avatar_url: user.avatar_url,
    })
}

/// Convert a `UserRecord` to the private `UserProfile` shape returned by `/users/me`.
pub fn user_to_self_profile(user: UserRecord) -> Result<UserProfile> {
    let id = user
        .id
        .ok_or_else(|| AppError::Internal("missing user id".into()))?;
    Ok(UserProfile {
        id,
        username: user.username,
        email: Some(user.email),
        display_name: user.display_name,
        avatar_url: user.avatar_url,
    })
}
