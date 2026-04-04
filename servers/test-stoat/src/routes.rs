//! Stoat/Revolt REST API route handlers.
//!
//! Implements the subset of the Revolt API that poly-stoat calls.
//! All handlers take `State<StoatState>` and return JSON responses.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::state::{StoatEvent, StoatState};
use poly_test_common::TokenAuth;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Stoat uses `x-session-token` header instead of Bearer auth.
fn session_user(state: &StoatState, headers: &HeaderMap) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|v| v.to_str().ok());
    state
        .auth
        .extract_user_id(token.map(|t| format!("Bearer {t}")).as_deref())
        .ok_or_else(|| revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession"))
}

fn revolt_error(
    status: StatusCode,
    error_type: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "type": error_type,
        })),
    )
}

// ---------------------------------------------------------------------------
// Server config
// ---------------------------------------------------------------------------

/// GET / — Server configuration (ws URL, features, etc.)
pub async fn server_config() -> impl IntoResponse {
    Json(serde_json::json!({
        "revolt": "0.7.0",
        "features": {
            "captcha": { "enabled": false },
            "email": false,
            "invite_only": false,
            "autumn": {
                "enabled": true,
                "url": "http://localhost:9101",
            },
        },
        "ws": "ws://localhost:9101/ws",
        "app": "http://localhost:9101",
        "vapid": "",
    }))
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: Option<String>,
    pub password: Option<String>,
}

/// POST /auth/session/login
pub async fn login(
    State(state): State<StoatState>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    let email = body.email.unwrap_or_default();
    let password = body.password.unwrap_or_default();

    // In our mock, email == username
    let user = state
        .users
        .iter()
        .find(|entry| entry.username == email || entry.id == email);

    let user = match user {
        Some(u) => u,
        None => return revolt_error(StatusCode::UNAUTHORIZED, "InvalidCredentials").into_response(),
    };

    if user.password != password {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidCredentials").into_response();
    }

    let user_id = user.id.clone();
    drop(user);

    let token = state.auth.create_token(&user_id);

    Json(serde_json::json!({
        "_id": token,
        "token": token,
        "user_id": user_id,
    }))
    .into_response()
}

/// POST /auth/session/logout
pub async fn logout(
    State(state): State<StoatState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = headers.get("x-session-token").and_then(|v| v.to_str().ok()) {
        state.auth.revoke(token);
    }
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

/// GET /users/@me
pub async fn get_me(
    State(state): State<StoatState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    match state.users.get(&user_id) {
        Some(user) => Json(user_to_json(&user)).into_response(),
        None => revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    }
}

/// GET /users/:id
pub async fn get_user(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    match state.users.get(&user_id) {
        Some(user) => Json(user_to_json(&user)).into_response(),
        None => revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    }
}

fn user_to_json(user: &crate::state::User) -> serde_json::Value {
    serde_json::json!({
        "_id": user.id,
        "username": user.username,
        "discriminator": user.discriminator,
        "display_name": user.display_name,
        "avatar": user.avatar_url.as_ref().map(|url| serde_json::json!({
            "_id": format!("av_{}", user.id),
            "tag": "avatars",
            "filename": "avatar.png",
            "metadata": { "type": "Image", "width": 128, "height": 128 },
            "content_type": "image/png",
            "size": 1024,
            "url": url,
        })),
        "status": user.status.as_ref().map(|s| serde_json::json!({
            "text": s.text,
            "presence": s.presence,
        })),
        "online": user.online,
    })
}

/// GET /users/dms — all DM and group channels for the authenticated user
pub async fn get_dms(
    State(state): State<StoatState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    let dm_ids = state
        .dm_channels
        .get(&user_id)
        .map(|v| v.clone())
        .unwrap_or_default();

    let channels: Vec<serde_json::Value> = dm_ids
        .iter()
        .filter_map(|id| state.channels.get(id).map(|ch| channel_to_json(&ch)))
        .collect();

    Json(channels).into_response()
}

/// GET /users/:id/dm — open or get DM with a specific user
pub async fn get_user_dm(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(target_id): Path<String>,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    // Find existing DM between these two users
    let dm_ids = state
        .dm_channels
        .get(&user_id)
        .map(|v| v.clone())
        .unwrap_or_default();

    for dm_id in &dm_ids {
        if let Some(ch) = state.channels.get(dm_id)
            && ch.channel_type == "DirectMessage" && ch.recipients.contains(&target_id)
        {
            return Json(channel_to_json(&ch)).into_response();
        }
    }

    revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response()
}

// ---------------------------------------------------------------------------
// Servers
// ---------------------------------------------------------------------------

/// GET /servers/:id
pub async fn get_server(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    match state.servers.get(&server_id) {
        Some(srv) => Json(serde_json::json!({
            "_id": srv.id,
            "name": srv.name,
            "owner": srv.owner,
            "icon": srv.icon_url.as_ref().map(|url| serde_json::json!({ "url": url })),
            "channels": srv.channels,
            "categories": srv.categories.iter().map(|c| serde_json::json!({
                "id": c.id,
                "title": c.title,
                "channels": c.channels,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        None => revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    }
}

/// GET /servers/:id/members
pub async fn get_server_members(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let srv = match state.servers.get(&server_id) {
        Some(s) => s,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    let members: Vec<serde_json::Value> = srv
        .members
        .iter()
        .map(|uid| {
            serde_json::json!({
                "_id": { "server": server_id, "user": uid },
                "joined_at": "2026-01-01T00:00:00.000Z",
            })
        })
        .collect();

    let users: Vec<serde_json::Value> = srv
        .members
        .iter()
        .filter_map(|uid| state.users.get(uid).map(|u| user_to_json(&u)))
        .collect();

    Json(serde_json::json!({
        "members": members,
        "users": users,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

/// GET /channels/:id
pub async fn get_channel(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    match state.channels.get(&channel_id) {
        Some(ch) => Json(channel_to_json(&ch)).into_response(),
        None => revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    }
}

fn channel_to_json(ch: &crate::state::Channel) -> serde_json::Value {
    serde_json::json!({
        "_id": ch.id,
        "channel_type": ch.channel_type,
        "name": ch.name,
        "description": ch.description,
        "server": ch.server_id,
        "recipients": ch.recipients,
        "last_message_id": ch.last_message_id,
    })
}

/// GET /channels/:id/members — group DM members
pub async fn get_channel_members(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let ch = match state.channels.get(&channel_id) {
        Some(c) => c,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    let members: Vec<serde_json::Value> = ch
        .recipients
        .iter()
        .filter_map(|uid| state.users.get(uid).map(|u| user_to_json(&u)))
        .collect();

    Json(members).into_response()
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MessagesQuery {
    pub before: Option<String>,
    pub after: Option<String>,
    pub limit: Option<usize>,
    pub include_users: Option<bool>,
}

/// GET /channels/:id/messages
pub async fn get_messages(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Query(params): Query<MessagesQuery>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let timeline = match state.messages.get(&channel_id) {
        Some(t) => t.clone(),
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    let limit = params.limit.unwrap_or(50).min(100);

    let messages: Vec<serde_json::Value> = if let Some(ref before) = params.before {
        // Messages before a given ID (backwards pagination)
        let idx = timeline
            .iter()
            .position(|m| &m.id == before)
            .unwrap_or(timeline.len());
        let start = idx.saturating_sub(limit);
        timeline.get(start..idx)
            .unwrap_or_default()
            .iter()
            .rev()
            .map(message_to_json)
            .collect()
    } else if let Some(ref after) = params.after {
        // Messages after a given ID
        let idx = timeline
            .iter()
            .position(|m| &m.id == after)
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = (idx + limit).min(timeline.len());
        timeline.get(idx..end).unwrap_or_default().iter().map(message_to_json).collect()
    } else {
        // Most recent messages
        let start = timeline.len().saturating_sub(limit);
        timeline.get(start..).unwrap_or_default().iter().rev().map(message_to_json).collect()
    };

    if params.include_users.unwrap_or(false) {
        // Return object with messages + users
        let user_ids: std::collections::HashSet<String> = messages
            .iter()
            .filter_map(|m| m.get("author").and_then(|a| a.as_str()).map(|s| s.to_string()))
            .collect();
        let users: Vec<serde_json::Value> = user_ids
            .iter()
            .filter_map(|uid| state.users.get(uid).map(|u| user_to_json(&u)))
            .collect();

        Json(serde_json::json!({
            "messages": messages,
            "users": users,
        }))
        .into_response()
    } else {
        Json(messages).into_response()
    }
}

/// POST /channels/:id/messages
pub async fn send_message(
    State(state): State<StoatState>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    if !state.channels.contains_key(&channel_id) {
        return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response();
    }

    let content = body
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();
    let nonce = body
        .get("nonce")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let replies: Option<Vec<String>> = body
        .get("replies")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.get("id")
                        .and_then(|id| id.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        });

    let msg_id = state.next_message_id();
    let msg = crate::state::Message {
        id: msg_id.clone(),
        content: content.clone(),
        author: user_id.clone(),
        channel: channel_id.clone(),
        nonce: nonce.clone(),
        replies: replies.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let msg_json = message_to_json(&msg);

    if let Some(mut timeline) = state.messages.get_mut(&channel_id) {
        timeline.push(msg);
    }

    if let Some(mut ch) = state.channels.get_mut(&channel_id) {
        ch.last_message_id = Some(msg_id);
    }

    // Broadcast event
    state.events.publish(StoatEvent::Message {
        channel_id,
        message: msg_json.clone(),
    });

    Json(msg_json).into_response()
}

fn message_to_json(msg: &crate::state::Message) -> serde_json::Value {
    serde_json::json!({
        "_id": msg.id,
        "content": msg.content,
        "author": msg.author,
        "channel": msg.channel,
        "nonce": msg.nonce,
        "replies": msg.replies,
        "createdAt": msg.created_at,
    })
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

/// GET /sync/unreads
pub async fn sync_unreads(
    State(state): State<StoatState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let unreads: Vec<serde_json::Value> = state
        .unreads
        .iter()
        .map(|entry| {
            let u = entry.value();
            serde_json::json!({
                "_id": { "channel": u.channel_id },
                "last_id": u.last_id,
                "mentions": u.mentions,
            })
        })
        .collect();

    Json(unreads).into_response()
}

// ---------------------------------------------------------------------------
// Lifecycle endpoints
// ---------------------------------------------------------------------------

/// POST /seed
pub async fn seed(State(state): State<StoatState>) -> impl IntoResponse {
    state.seed();
    Json(serde_json::json!({ "status": "seeded" }))
}

/// POST /reset
pub async fn reset(State(state): State<StoatState>) -> impl IntoResponse {
    state.reset();
    Json(serde_json::json!({ "status": "reset" }))
}

/// POST /reseed
pub async fn reseed(State(state): State<StoatState>) -> impl IntoResponse {
    state.reseed();
    Json(serde_json::json!({ "status": "reseeded" }))
}
