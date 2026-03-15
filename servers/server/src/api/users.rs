use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{get, patch},
};
use serde::Deserialize;

use crate::{
    AppState,
    auth::AuthUser,
    db_ext::{take_many, take_one},
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
    let user: UserRecord = take_one(
        &mut state
            .db
            .query("SELECT * FROM type::record($id) LIMIT 1")
            .bind(("id", auth.user_id.clone()))
            .await?,
        0,
    )?
    .ok_or(AppError::NotFound)?;
    Ok(Json(user_to_self_profile(user)?))
}

async fn update_me(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<UserProfile>> {
    let user: UserRecord = take_one(
        &mut state
            .db
            .query(
                "UPDATE type::record($id) MERGE { \
                  display_name: $dn ?? display_name, \
                  avatar_url: $av ?? avatar_url \
                } RETURN *",
            )
            .bind(("id", auth.user_id.clone()))
            .bind(("dn", req.display_name))
            .bind(("av", req.avatar_url))
            .await?,
        0,
    )?
    .ok_or(AppError::NotFound)?;
    Ok(Json(user_to_profile(user)?))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<UserProfile>> {
    let user: UserRecord = take_one(
        &mut state
            .db
            .query("SELECT * FROM type::record($id) LIMIT 1")
            .bind(("id", id))
            .await?,
        0,
    )?
    .ok_or(AppError::NotFound)?;
    Ok(Json(user_to_profile(user)?))
}

async fn list_friends(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<UserProfile>>> {
    // Friends = accepted friend_requests where this user is from or to.
    let records: Vec<serde_json::Value> = take_many(
        &mut state
            .db
            .query(
                "SELECT from.*, to.* FROM friend_request \
                 WHERE status = 'accepted' \
                   AND (from = type::record($uid) OR to = type::record($uid)) \
                 FETCH from, to",
            )
            .bind(("uid", auth.user_id.clone()))
            .await?,
        0,
    )?;

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
    let target: UserRecord = take_one(
        &mut state
            .db
            .query("SELECT * FROM user WHERE username = $u LIMIT 1")
            .bind(("u", req.username))
            .await?,
        0,
    )?
    .ok_or(AppError::NotFound)?;
    let target_id = target
        .id
        .ok_or_else(|| AppError::Internal("missing id".into()))?;

    let created: FriendRequest = take_one(
        &mut state
            .db
            .query(
                "CREATE friend_request CONTENT { \
                  `from`: type::record($from), `to`: type::record($to), \
                  status: 'pending', created_at: time::now() \
                } RETURN *",
            )
            .bind(("from", auth.user_id.clone()))
            .bind(("to", target_id))
            .await?,
        0,
    )?
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
    let fr: FriendRequest = take_one(
        &mut state
            .db
            .query("SELECT * FROM type::record($id) LIMIT 1")
            .bind(("id", request_id.clone()))
            .await?,
        0,
    )?
    .ok_or(AppError::NotFound)?;

    if fr.to != auth.user_id {
        return Err(AppError::Forbidden);
    }

    let updated: FriendRequest = take_one(
        &mut state
            .db
            .query("UPDATE type::record($id) SET status = $s RETURN *")
            .bind(("id", request_id))
            .bind(("s", status.to_owned()))
            .await?,
        0,
    )?
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
        .query(
            "DELETE friend_request WHERE status = 'accepted' AND \
             ((`from` = type::record($me) AND `to` = type::record($them)) OR \
              (`from` = type::record($them) AND `to` = type::record($me)))",
        )
        .bind(("me", auth.user_id.clone()))
        .bind(("them", target_user_id))
        .await?
        .check()
        .map_err(AppError::Db)?;
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
