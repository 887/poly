//! Mock Microsoft Graph API handlers for Teams.
//!
//! Implements the Graph API subset that `poly-teams` calls.
//! Auth: `Authorization: Bearer TOKEN` (mock OAuth2 token).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::state::{Message, TeamsState};

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

fn graph_error(status: StatusCode, code: &str, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({
        "error": { "code": code, "message": message }
    })))
}

fn auth_user(state: &TeamsState, headers: &HeaderMap) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let raw = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = raw.strip_prefix("Bearer ").unwrap_or(raw);
    state.auth.validate(token)
        .ok_or_else(|| graph_error(StatusCode::UNAUTHORIZED, "InvalidAuthenticationToken", "Access token is empty or invalid."))
}

// ---------------------------------------------------------------------------
// Test-only easy-signin
// ---------------------------------------------------------------------------

pub async fn test_auth_token(
    State(state): State<Arc<TeamsState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let identifier = body.get("username").and_then(|v| v.as_str()).unwrap_or("teams_test");
    // Resolve display_name / email → user ID; fall back to treating identifier as ID.
    let user_id = state
        .users
        .iter()
        .find(|u| u.display_name == identifier || u.email == identifier)
        .map(|u| u.id.clone())
        .unwrap_or_else(|| identifier.to_string());
    let token = state.auth.create_token(&user_id);
    Json(serde_json::json!({
        "result": "Success",
        "token": token,
        "user_id": user_id,
    }))
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub login: String,
    pub password: String,
}

/// POST /test/auth/login — email+password → Bearer token, mirrors test-discord.
pub async fn login(
    State(state): State<Arc<TeamsState>>,
    Json(body): Json<LoginBody>,
) -> impl IntoResponse {
    let user = state
        .users
        .iter()
        .find(|u| u.email == body.login || u.display_name == body.login)
        .map(|u| u.clone());
    let Some(user) = user else {
        return graph_error(StatusCode::UNAUTHORIZED, "InvalidAuthenticationRequest", "Unknown user").into_response();
    };
    if user.password != body.password {
        return graph_error(StatusCode::UNAUTHORIZED, "InvalidAuthenticationRequest", "Incorrect password").into_response();
    }
    let token = state.auth.create_token(&user.id);
    Json(serde_json::json!({
        "token": token,
        "user_id": user.id,
    })).into_response()
}

// ---------------------------------------------------------------------------
// Current user (GET /v1.0/me)
// ---------------------------------------------------------------------------

pub async fn get_me(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            match state.users.get(&user_id) {
                Some(u) => Json(user_to_json(&u)).into_response(),
                None => Json(serde_json::json!({
                    "id": user_id,
                    "displayName": user_id,
                    "mail": format!("{}@contoso.com", user_id),
                    "userPrincipalName": format!("{}@contoso.com", user_id),
                })).into_response(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Teams (GET /v1.0/me/joinedTeams)
// ---------------------------------------------------------------------------

pub async fn get_joined_teams(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let teams: Vec<serde_json::Value> = state.teams.iter()
                .filter(|t| t.members.contains(&user_id))
                .map(|t| team_to_json(&t))
                .collect();
            Json(serde_json::json!({ "value": teams })).into_response()
        }
    }
}

/// GET /v1.0/teams/:team_id
pub async fn get_team(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => match state.teams.get(&team_id) {
            Some(t) => Json(team_to_json(&t)).into_response(),
            None => graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Team not found").into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

/// GET /v1.0/teams/:team_id/channels
pub async fn get_channels(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let channels: Vec<serde_json::Value> = state.channels.iter()
                .filter(|c| c.team_id == team_id)
                .map(|c| channel_to_json(&c))
                .collect();
            Json(serde_json::json!({ "value": channels })).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MessagesQuery {
    #[serde(rename = "$top")]
    pub top: Option<u32>,
}

/// GET /v1.0/teams/:team_id/channels/:channel_id/messages
pub async fn get_channel_messages(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path((_team_id, channel_id)): Path<(String, String)>,
    Query(query): Query<MessagesQuery>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let msgs = state.messages.get(&channel_id).map(|v| v.clone()).unwrap_or_default();
            let top = query.top.unwrap_or(50).min(100) as usize;
            let value: Vec<serde_json::Value> = msgs.iter().rev().take(top)
                .map(|m| message_to_json(m, &state))
                .collect();
            Json(serde_json::json!({ "value": value })).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct SendMessageBody {
    pub body: Option<serde_json::Value>,
}

/// POST /v1.0/teams/:team_id/channels/:channel_id/messages
pub async fn send_channel_message(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path((_team_id, channel_id)): Path<(String, String)>,
    Json(body): Json<SendMessageBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let content = body.body
                .and_then(|b| b.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()))
                .unwrap_or_default();
            let msg_id = format!("MSG{}", state.messages.len() + 100);
            let msg = Message {
                id: msg_id.clone(),
                body_content: content,
                from_user_id: user_id,
                channel_or_chat_id: channel_id.clone(),
                created_date_time: chrono::Utc::now().to_rfc3339(),
                last_modified_date_time: None,
                deleted_date_time: None,
            };
            let json = message_to_json(&msg, &state);
            state.messages.entry(channel_id).or_default().push(msg);
            (StatusCode::CREATED, Json(json)).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct EditMessageBody {
    pub body: Option<serde_json::Value>,
}

/// PATCH /v1.0/teams/:team_id/channels/:channel_id/messages/:message_id
pub async fn edit_channel_message(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path((_team_id, channel_id, message_id)): Path<(String, String, String)>,
    Json(body): Json<EditMessageBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let new_content = body.body
                .and_then(|b| b.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()));
            let Some(new_content) = new_content else {
                return graph_error(StatusCode::BAD_REQUEST, "InvalidRequest", "Missing body.content").into_response();
            };
            let mut entry = match state.messages.get_mut(&channel_id) {
                Some(e) => e,
                None => return graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Channel not found").into_response(),
            };
            let Some(msg) = entry.iter_mut().find(|m| m.id == message_id) else {
                return graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Message not found").into_response();
            };
            if msg.from_user_id != user_id {
                return graph_error(StatusCode::FORBIDDEN, "Forbidden", "Not message author").into_response();
            }
            if msg.deleted_date_time.is_some() {
                return graph_error(StatusCode::GONE, "Gone", "Message is deleted").into_response();
            }
            msg.body_content = new_content;
            msg.last_modified_date_time = Some(chrono::Utc::now().to_rfc3339());
            let json = message_to_json(msg, &state);
            Json(json).into_response()
        }
    }
}

/// DELETE /v1.0/teams/:team_id/channels/:channel_id/messages/:message_id
/// Soft delete (Graph sets deletedDateTime, row stays).
pub async fn delete_channel_message(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path((_team_id, channel_id, message_id)): Path<(String, String, String)>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let mut entry = match state.messages.get_mut(&channel_id) {
                Some(e) => e,
                None => return graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Channel not found").into_response(),
            };
            let Some(msg) = entry.iter_mut().find(|m| m.id == message_id) else {
                return graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Message not found").into_response();
            };
            if msg.from_user_id != user_id {
                return graph_error(StatusCode::FORBIDDEN, "Forbidden", "Not message author").into_response();
            }
            msg.deleted_date_time = Some(chrono::Utc::now().to_rfc3339());
            msg.body_content = String::new();
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Chats / DMs  (GET /v1.0/me/chats)
// ---------------------------------------------------------------------------

pub async fn get_chats(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let chats: Vec<serde_json::Value> = state.chats.iter()
                .filter(|c| c.members.contains(&user_id))
                .map(|c| chat_to_json(&c))
                .collect();
            Json(serde_json::json!({ "value": chats })).into_response()
        }
    }
}

/// GET /v1.0/chats/:chat_id/messages
pub async fn get_chat_messages(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path(chat_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let Some(chat) = state.chats.get(&chat_id) else {
                return graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Chat not found").into_response();
            };
            if !chat.members.contains(&user_id) {
                return graph_error(StatusCode::FORBIDDEN, "Forbidden", "Not a chat member").into_response();
            }
            drop(chat);
            let msgs = state.messages.get(&chat_id).map(|v| v.clone()).unwrap_or_default();
            let top = query.top.unwrap_or(50).min(100) as usize;
            let value: Vec<serde_json::Value> = msgs.iter().rev().take(top)
                .map(|m| message_to_json(m, &state))
                .collect();
            Json(serde_json::json!({ "value": value })).into_response()
        }
    }
}

/// POST /v1.0/chats/:chat_id/messages
pub async fn send_chat_message(
    State(state): State<Arc<TeamsState>>,
    headers: HeaderMap,
    Path(chat_id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let Some(chat) = state.chats.get(&chat_id) else {
                return graph_error(StatusCode::NOT_FOUND, "ResourceNotFound", "Chat not found").into_response();
            };
            if !chat.members.contains(&user_id) {
                return graph_error(StatusCode::FORBIDDEN, "Forbidden", "Not a chat member").into_response();
            }
            drop(chat);
            let content = body.body
                .and_then(|b| b.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()))
                .unwrap_or_default();
            let msg_id = format!("MSG{}", state.messages.len() + 100);
            let msg = Message {
                id: msg_id.clone(),
                body_content: content,
                from_user_id: user_id,
                channel_or_chat_id: chat_id.clone(),
                created_date_time: chrono::Utc::now().to_rfc3339(),
                last_modified_date_time: None,
                deleted_date_time: None,
            };
            let json = message_to_json(&msg, &state);
            state.messages.entry(chat_id).or_default().push(msg);
            (StatusCode::CREATED, Json(json)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

pub async fn seed(State(state): State<Arc<TeamsState>>) -> impl IntoResponse {
    state.seed();
    Json(serde_json::json!({ "ok": true }))
}

pub async fn reset(State(state): State<Arc<TeamsState>>) -> impl IntoResponse {
    state.reset();
    Json(serde_json::json!({ "ok": true }))
}

pub async fn reseed(State(state): State<Arc<TeamsState>>) -> impl IntoResponse {
    state.reseed();
    Json(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// JSON serializers
// ---------------------------------------------------------------------------

fn user_to_json(u: &crate::state::User) -> serde_json::Value {
    serde_json::json!({
        "id": u.id,
        "displayName": u.display_name,
        "mail": u.email,
        "userPrincipalName": u.email,
    })
}

fn team_to_json(t: &crate::state::Team) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "displayName": t.display_name,
        "description": t.description,
    })
}

fn channel_to_json(c: &crate::state::Channel) -> serde_json::Value {
    serde_json::json!({
        "id": c.id,
        "displayName": c.display_name,
        "membershipType": "standard",
    })
}

fn chat_to_json(c: &crate::state::Chat) -> serde_json::Value {
    serde_json::json!({
        "id": c.id,
        "chatType": c.chat_type,
    })
}

fn message_to_json(m: &Message, state: &TeamsState) -> serde_json::Value {
    let from_user = state.users.get(&m.from_user_id).map(|u| serde_json::json!({
        "user": { "id": u.id, "displayName": u.display_name }
    })).unwrap_or_else(|| serde_json::json!({
        "user": { "id": m.from_user_id, "displayName": m.from_user_id }
    }));
    serde_json::json!({
        "id": m.id,
        "body": { "content": m.body_content, "contentType": "text" },
        "from": from_user,
        "channelIdentity": { "channelId": m.channel_or_chat_id },
        "createdDateTime": m.created_date_time,
        "lastModifiedDateTime": m.last_modified_date_time,
        "deletedDateTime": m.deleted_date_time,
        "messageType": "message",
        "attachments": [],
    })
}
