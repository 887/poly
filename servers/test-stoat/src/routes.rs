//! Stoat/Revolt REST API route handlers.
//!
//! Implements the subset of the Revolt API that poly-stoat calls.
//! All handlers take `State<std::sync::Arc<StoatState>>` and return JSON responses.

use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::state::{StoatEvent, StoatState};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Stoat uses `x-session-token` header instead of Bearer auth.
fn session_user(state: &StoatState, headers: &HeaderMap) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let token = headers
        .get("x-session-token")
        .and_then(|v| v.to_str().ok());
    // Validate the raw token directly instead of wrapping in Bearer format.
    let user_id = token.and_then(|t| state.auth.validate(t));
    user_id.ok_or_else(|| revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession"))
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
pub async fn server_config(headers: HeaderMap) -> impl IntoResponse {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    Json(serde_json::json!({
        "revolt": "0.7.0",
        "features": {
            "captcha": { "enabled": false },
            "email": false,
            "invite_only": false,
        },
        "ws": format!("ws://{}/bonfire", host),
        "app": format!("http://{}", host),
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
    State(state): State<std::sync::Arc<StoatState>>,
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
        "result": "Success",
        "_id": user_id,
        "token": token,
        "user_id": user_id,
        "name": "Poly",
        "last_seen": "1970-01-01T00:00:00.000Z",
    }))
    .into_response()
}

/// POST /auth/session/logout
pub async fn logout(
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    match state.users.get(&user_id) {
        Some(user) => {
            // Build friend relations: all other known users are friends.
            let relations: Vec<(String, String)> = state
                .users
                .iter()
                .filter(|e| e.key() != &user_id)
                .map(|e| (e.key().clone(), "Friend".to_string()))
                .collect();
            let relation_refs: Vec<(&str, &str)> = relations
                .iter()
                .map(|(id, status)| (id.as_str(), status.as_str()))
                .collect();
            Json(user_to_json_with_relations(&user, &relation_refs)).into_response()
        }
        None => revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    }
}

/// GET /users/@me/servers — list servers the authenticated user belongs to
pub async fn get_my_servers(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    let servers: Vec<serde_json::Value> = state
        .servers
        .iter()
        .filter(|entry| entry.value().members.contains(&user_id))
        .map(|entry| {
            let srv = entry.value();
            serde_json::json!({
                "_id": srv.id,
                "name": srv.name,
                "owner": srv.owner,
                "icon": srv.icon_url.as_ref().map(|url| serde_json::json!({
                    "_id": format!("icon_{}", srv.id),
                    "tag": "icons",
                    "filename": "icon.png",
                    "content_type": "image/png",
                    "size": 1024,
                })),
                "categories": srv.categories.iter().map(|cat| serde_json::json!({
                    "id": cat.id,
                    "title": cat.title,
                    "channels": cat.channels,
                })).collect::<Vec<_>>(),
                "channels": srv.channels,
            })
        })
        .collect();

    Json(servers).into_response()
}

/// GET /users/:id
pub async fn get_user(
    State(state): State<std::sync::Arc<StoatState>>,
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
    user_to_json_with_relations(user, &[])
}

fn user_to_json_with_relations(user: &crate::state::User, relations: &[(&str, &str)]) -> serde_json::Value {
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
        "relations": relations.iter().map(|(id, status)| serde_json::json!({
            "_id": id,
            "status": status,
        })).collect::<Vec<_>>(),
    })
}

/// GET /users/dms — all DM and group channels for the authenticated user
pub async fn get_dms(
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
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

// ---------------------------------------------------------------------------
// Moderation (B-ST) — kick, ban, unban, list-bans, member-edit, delete-message,
//                     update-channel
// ---------------------------------------------------------------------------

/// DELETE /servers/:server_id/members/:member_id — kick a member from the server.
pub async fn kick_member(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path((server_id, member_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let mut srv = match state.servers.get_mut(&server_id) {
        Some(s) => s,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    let before = srv.members.len();
    srv.members.retain(|uid| uid != &member_id);
    if srv.members.len() == before {
        return revolt_error(StatusCode::NOT_FOUND, "NotMember").into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

/// PUT /servers/:server_id/bans/:user_id — ban a user.
pub async fn ban_member(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path((server_id, user_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    if state.servers.get(&server_id).is_none() {
        return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response();
    }

    let reason = body.get("reason").and_then(|r| r.as_str()).map(str::to_string);
    let key = crate::state::StoatState::member_key(&server_id, &user_id);
    state.bans.insert(key, crate::state::BanRecord {
        server_id: server_id.clone(),
        user_id: user_id.clone(),
        reason,
    });

    // Also remove from server members if present.
    if let Some(mut srv) = state.servers.get_mut(&server_id) {
        srv.members.retain(|uid| uid != &user_id);
    }

    StatusCode::NO_CONTENT.into_response()
}

/// DELETE /servers/:server_id/bans/:user_id — unban a user.
pub async fn unban_member(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path((server_id, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let key = crate::state::StoatState::member_key(&server_id, &user_id);
    if state.bans.remove(&key).is_none() {
        return revolt_error(StatusCode::NOT_FOUND, "BanNotFound").into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

/// GET /servers/:server_id/bans — list all bans for a server.
pub async fn list_bans(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    if state.servers.get(&server_id).is_none() {
        return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response();
    }

    let bans: Vec<serde_json::Value> = state
        .bans
        .iter()
        .filter(|entry| entry.value().server_id == server_id)
        .map(|entry| {
            let ban = entry.value();
            serde_json::json!({
                "_id": { "server": ban.server_id, "user": ban.user_id },
                "reason": ban.reason,
            })
        })
        .collect();

    let user_ids: Vec<String> = bans
        .iter()
        .filter_map(|b| b.get("_id").and_then(|id| id.get("user")).and_then(|u| u.as_str()).map(str::to_string))
        .collect();

    let users: Vec<serde_json::Value> = user_ids
        .iter()
        .filter_map(|uid| state.users.get(uid).map(|u| user_to_json(&u)))
        .collect();

    Json(serde_json::json!({ "bans": bans, "users": users })).into_response()
}

/// PATCH /servers/:server_id/members/:member_id — edit member (timeout / clear timeout).
pub async fn edit_member(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path((server_id, member_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    if state.servers.get(&server_id).is_none() {
        return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response();
    }

    let key = crate::state::StoatState::member_key(&server_id, &member_id);

    // Handle remove: ["Timeout"] to clear a timeout.
    if let Some(remove) = body.get("remove").and_then(|r| r.as_array()) {
        if remove.iter().any(|v| v.as_str() == Some("Timeout")) {
            state.member_mod.entry(key.clone()).and_modify(|m| m.timeout = None);
        }
    }

    // Handle timeout field to set a timeout.
    if let Some(timeout) = body.get("timeout").and_then(|t| t.as_str()) {
        state.member_mod.insert(key, crate::state::MemberModState {
            timeout: Some(timeout.to_string()),
        });
    }

    StatusCode::NO_CONTENT.into_response()
}

/// GET /servers/:server_id/members/@me — get my own member record.
pub async fn get_my_member(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path(server_id): Path<String>,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    let srv = match state.servers.get(&server_id) {
        Some(s) => s,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    if !srv.members.contains(&user_id) {
        return revolt_error(StatusCode::NOT_FOUND, "NotMember").into_response();
    }

    let key = crate::state::StoatState::member_key(&server_id, &user_id);
    let mod_state = state.member_mod.get(&key).map(|m| m.clone()).unwrap_or_default();

    Json(serde_json::json!({
        "_id": { "server": server_id, "user": user_id },
        "joined_at": "2026-01-01T00:00:00.000Z",
        "roles": [],
        "timeout": mod_state.timeout,
    })).into_response()
}

/// DELETE /channels/:channel_id/messages/:message_id — delete a message.
pub async fn delete_message(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let mut timeline = match state.messages.get_mut(&channel_id) {
        Some(t) => t,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    let before = timeline.len();
    timeline.retain(|m| m.id != message_id);
    if timeline.len() == before {
        return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response();
    }

    // Broadcast delete event.
    state.events.publish(StoatEvent::MessageDelete {
        channel_id: channel_id.clone(),
        message_id: message_id.clone(),
    });

    StatusCode::NO_CONTENT.into_response()
}

#[derive(serde::Deserialize)]
pub struct ChannelEditBody {
    pub name: Option<String>,
    pub description: Option<String>,
    pub slowmode: Option<u32>,
    pub nsfw: Option<bool>,
}

/// PATCH /channels/:channel_id — update channel settings.
pub async fn update_channel(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(body): Json<ChannelEditBody>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let mut ch = match state.channels.get_mut(&channel_id) {
        Some(c) => c,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    if let Some(name) = body.name {
        ch.name = name;
    }
    if let Some(description) = body.description {
        ch.description = Some(description);
    }
    // slowmode and nsfw are accepted but test-stoat doesn't store them yet; ignore for now.
    let _ = (body.slowmode, body.nsfw);

    StatusCode::NO_CONTENT.into_response()
}

/// GET /channels/:id
pub async fn get_channel(
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
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

/// GET /channels/:channel_id/messages/:message_id — fetch a single message
pub async fn get_message(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if session_user(&state, &headers).is_err() {
        return revolt_error(StatusCode::UNAUTHORIZED, "InvalidSession").into_response();
    }

    let timeline = match state.messages.get(&channel_id) {
        Some(t) => t.clone(),
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    match timeline.iter().find(|m| m.id == message_id) {
        Some(msg) => Json(message_to_json(msg)).into_response(),
        None => revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    }
}

/// POST /channels/:id/messages
pub async fn send_message(
    State(state): State<std::sync::Arc<StoatState>>,
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
    State(state): State<std::sync::Arc<StoatState>>,
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

/// GET /avatars/:id — serve PNG avatar for test users
pub async fn serve_avatar(Path(id): Path<String>) -> impl IntoResponse {
    static STOAT_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/stoat.png");
    static RACCOON_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/raccoon.png");
    static LEMMING_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/lemming.png");

    let bytes: &[u8] = match id.as_str() {
        "av_STOAT01" => STOAT_PNG,
        "av_RACCOON01" => RACCOON_PNG,
        "av_LEMMING01" => LEMMING_PNG,
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    (
        StatusCode::OK,
        [("content-type", "image/png")],
        bytes,
    ).into_response()
}

// ---------------------------------------------------------------------------
// Test-only easy-signin (no password required)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TestTokenRequest {
    pub username: String,
}

/// POST /test/auth/token — return a session token without password verification.
///
/// Only present in test servers (localhost, never deployed to production).
/// Used by `test_signin` MCP tool and the UI "Quick Login" button.
pub async fn test_auth_token(
    State(state): State<std::sync::Arc<StoatState>>,
    Json(body): Json<TestTokenRequest>,
) -> impl IntoResponse {
    let user = state
        .users
        .iter()
        .find(|entry| entry.username == body.username || entry.id == body.username);

    let user = match user {
        Some(u) => u,
        None => return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response(),
    };

    let user_id = user.id.clone();
    let display_name = user.display_name.clone();
    drop(user);

    let token = state.auth.create_token(&user_id);

    Json(serde_json::json!({
        "result": "Success",
        "_id": user_id,
        "token": token,
        "user_id": user_id,
        "display_name": display_name,
    }))
    .into_response()
}

/// POST /seed
pub async fn seed(State(state): State<std::sync::Arc<StoatState>>) -> impl IntoResponse {
    state.seed();
    Json(serde_json::json!({ "status": "seeded" }))
}

/// POST /reset
pub async fn reset(State(state): State<std::sync::Arc<StoatState>>) -> impl IntoResponse {
    state.reset();
    Json(serde_json::json!({ "status": "reset" }))
}

/// POST /reseed
pub async fn reseed(State(state): State<std::sync::Arc<StoatState>>) -> impl IntoResponse {
    state.reseed();
    Json(serde_json::json!({ "status": "reseeded" }))
}

// ---------------------------------------------------------------------------
// Real-time: typing indicator + Bonfire WebSocket
// ---------------------------------------------------------------------------

/// POST /channels/:id/typing — broadcast a ChannelStartTyping event.
pub async fn channel_start_typing(
    State(state): State<std::sync::Arc<StoatState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    let user_id = match session_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    if !state.channels.contains_key(&channel_id) {
        return revolt_error(StatusCode::NOT_FOUND, "NotFound").into_response();
    }

    state.events.publish(StoatEvent::ChannelStartTyping { channel_id, user_id });
    StatusCode::NO_CONTENT.into_response()
}

/// GET /bonfire — WebSocket upgrade endpoint (Revolt Bonfire protocol).
pub async fn bonfire_ws(
    State(state): State<std::sync::Arc<StoatState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| bonfire_handler(socket, state))
}

async fn bonfire_handler(mut socket: WebSocket, state: std::sync::Arc<StoatState>) {
    // Step 1: wait for Authenticate message
    let token = loop {
        match socket.recv().await {
            Some(Ok(WsMessage::Text(text))) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
                    && json.get("type").and_then(|t| t.as_str()) == Some("Authenticate")
                    && let Some(t) = json.get("token").and_then(|t| t.as_str())
                {
                    break t.to_string();
                }
            }
            Some(Ok(WsMessage::Close(_))) | None => return,
            _ => {}
        }
    };

    // Step 2: validate token
    let _user_id = match state.auth.validate(&token) {
        Some(uid) => uid,
        None => {
            let _ = socket
                .send(WsMessage::Text(
                    serde_json::json!({"type":"InvalidSession"}).to_string().into(),
                ))
                .await;
            return;
        }
    };

    // Step 3: send Authenticated
    if socket
        .send(WsMessage::Text(
            serde_json::json!({"type":"Authenticated"}).to_string().into(),
        ))
        .await
        .is_err()
    {
        return;
    }

    // Step 4: subscribe to events and forward to client
    let mut events_rx = state.events.subscribe();

    loop {
        tokio::select! {
            event = events_rx.recv() => {
                match event {
                    Ok(ev) => {
                        let msg = match ev {
                            StoatEvent::Message { channel_id, message } => {
                                serde_json::json!({
                                    "type": "Message",
                                    "channel": channel_id,
                                    "message": message,
                                })
                            }
                            StoatEvent::ChannelStartTyping { channel_id, user_id } => {
                                serde_json::json!({
                                    "type": "ChannelStartTyping",
                                    "id": channel_id,
                                    "user": user_id,
                                })
                            }
                            StoatEvent::MessageUpdate { channel_id, message_id, data } => {
                                serde_json::json!({
                                    "type": "MessageUpdate",
                                    "channel": channel_id,
                                    "id": message_id,
                                    "data": data,
                                })
                            }
                            StoatEvent::MessageDelete { channel_id, message_id } => {
                                serde_json::json!({
                                    "type": "MessageDelete",
                                    "channel": channel_id,
                                    "id": message_id,
                                })
                            }
                            StoatEvent::UserUpdate { user_id, data } => {
                                serde_json::json!({
                                    "type": "UserUpdate",
                                    "id": user_id,
                                    "data": data,
                                })
                            }
                        };
                        if socket
                            .send(WsMessage::Text(msg.to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Skip lagged events — client may have missed some
                    }
                }
            }
            client_msg = socket.recv() => {
                match client_msg {
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
