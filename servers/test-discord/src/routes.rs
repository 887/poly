//! Mock Discord REST API v10 handlers.
//!
//! Implements the subset of the Discord API that `poly-discord` calls.
//! Auth: `Authorization: Bot TOKEN` or `Authorization: Bearer TOKEN`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

use crate::state::{Channel, DiscordState, Message};

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

fn discord_error(status: StatusCode, code: u32, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "code": code, "message": message })))
}

/// Extract user_id from `Authorization: Bot TOKEN` or `Authorization: Bearer TOKEN`.
fn auth_user(state: &DiscordState, headers: &HeaderMap) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    let raw = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = raw
        .strip_prefix("Bot ")
        .or_else(|| raw.strip_prefix("Bearer "))
        .unwrap_or(raw);
    state
        .auth
        .validate(token)
        .ok_or_else(|| discord_error(StatusCode::UNAUTHORIZED, 40001, "401: Unauthorized"))
}

// ---------------------------------------------------------------------------
// Test-only easy-signin
// ---------------------------------------------------------------------------

pub async fn test_auth_token(
    State(state): State<Arc<DiscordState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let identifier = body
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("discord_test");
    // Look up by username first, fall back to treating identifier as a user ID.
    let user_id = state
        .users
        .iter()
        .find(|u| u.username == identifier)
        .map(|u| u.id.clone())
        .unwrap_or_else(|| identifier.to_string());
    let token = state.auth.create_token(&user_id);
    Json(serde_json::json!({
        "result": "Success",
        "token": token,
        "user_id": user_id,
    }))
}

// ---------------------------------------------------------------------------
// Current user
// ---------------------------------------------------------------------------

/// GET /api/v10/users/@me
pub async fn get_me(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            match state.users.get(&user_id) {
                Some(u) => Json(user_to_json(&u)).into_response(),
                None => {
                    // User not found in map — create a synthetic one from the token username
                    Json(serde_json::json!({
                        "id": user_id,
                        "username": user_id,
                        "discriminator": "0000",
                        "avatar": null,
                        "bot": false,
                    }))
                    .into_response()
                }
            }
        }
    }
}

/// GET /api/v10/users/:user_id
pub async fn get_user(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => match state.users.get(&user_id) {
            Some(u) => Json(user_to_json(&u)).into_response(),
            None => discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Guilds (servers)
// ---------------------------------------------------------------------------

/// GET /api/v10/users/@me/guilds
pub async fn get_my_guilds(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let guilds: Vec<serde_json::Value> = state
                .guilds
                .iter()
                .filter(|entry| entry.members.contains(&user_id))
                .map(|entry| guild_to_json(&entry))
                .collect();
            Json(guilds).into_response()
        }
    }
}

/// GET /api/v10/guilds/:guild_id
pub async fn get_guild(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => match state.guilds.get(&guild_id) {
            Some(g) => Json(guild_to_json(&g)).into_response(),
            None => discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

/// GET /api/v10/guilds/:guild_id/channels
pub async fn get_guild_channels(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let guild = match state.guilds.get(&guild_id) {
                Some(g) => g,
                None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
            };
            let channels: Vec<serde_json::Value> = guild
                .channels
                .iter()
                .filter_map(|ch_id| state.channels.get(ch_id).map(|c| channel_to_json(&c)))
                .collect();
            Json(channels).into_response()
        }
    }
}

/// GET /api/v10/channels/:channel_id
pub async fn get_channel(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => match state.channels.get(&channel_id) {
            Some(c) => Json(channel_to_json(&c)).into_response(),
            None => discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response(),
        },
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MessagesQuery {
    pub limit: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// GET /api/v10/channels/:channel_id/messages
pub async fn get_messages(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let msgs = state.messages.get(&channel_id)
                .map(|v| v.clone())
                .unwrap_or_default();
            let limit = query.limit.unwrap_or(50).min(100) as usize;
            let slice: Vec<serde_json::Value> = msgs
                .iter()
                .rev()
                .take(limit)
                .map(|m| message_to_json(m, &state))
                .collect();
            Json(slice).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct SendMessageBody {
    pub content: Option<String>,
}

/// POST /api/v10/channels/:channel_id/messages
pub async fn send_message(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let content = body.content.unwrap_or_default();
            let msg_id = format!("M{}", state.messages.len() + 100);
            let msg = Message {
                id: msg_id.clone(),
                content: content.clone(),
                author_id: user_id.clone(),
                channel_id: channel_id.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let json = message_to_json(&msg, &state);
            state.messages.entry(channel_id).or_default().push(msg);
            (StatusCode::OK, Json(json)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// DMs
// ---------------------------------------------------------------------------

/// GET /api/v10/users/@me/channels  (DM list)
pub async fn get_dms(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let dms: Vec<serde_json::Value> = state
                .channels
                .iter()
                .filter(|c| c.channel_type == 1)
                .map(|c| channel_to_json(&c))
                .collect();
            Json(dms).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct OpenDmBody {
    pub recipient_id: Option<String>,
}

/// POST /api/v10/users/@me/channels  (open or get DM channel)
pub async fn open_dm(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Json(body): Json<OpenDmBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let recipient_id = body.recipient_id.unwrap_or_default();
            let dm_id = format!("DM-{}-{}", user_id, recipient_id);
            // Return existing or create new DM channel
            let ch = state.channels.entry(dm_id.clone()).or_insert_with(|| Channel {
                id: dm_id.clone(),
                name: "".into(),
                guild_id: None,
                channel_type: 1,
                parent_id: None,
            });
            Json(channel_to_json(&ch)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

pub async fn seed(State(state): State<Arc<DiscordState>>) -> impl IntoResponse {
    state.seed();
    Json(serde_json::json!({ "ok": true }))
}

pub async fn reset(State(state): State<Arc<DiscordState>>) -> impl IntoResponse {
    state.reset();
    Json(serde_json::json!({ "ok": true }))
}

pub async fn reseed(State(state): State<Arc<DiscordState>>) -> impl IntoResponse {
    state.reseed();
    Json(serde_json::json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// JSON serializers
// ---------------------------------------------------------------------------

fn user_to_json(u: &crate::state::User) -> serde_json::Value {
    serde_json::json!({
        "id": u.id,
        "username": u.username,
        "discriminator": u.discriminator,
        "avatar": u.avatar,
        "bot": false,
    })
}

fn guild_to_json(g: &crate::state::Guild) -> serde_json::Value {
    serde_json::json!({
        "id": g.id,
        "name": g.name,
        "owner_id": g.owner_id,
        "icon": null,
        "permissions": "0",
    })
}

fn channel_to_json(c: &Channel) -> serde_json::Value {
    serde_json::json!({
        "id": c.id,
        "name": c.name,
        "type": c.channel_type,
        "guild_id": c.guild_id,
        "parent_id": c.parent_id,
        "position": 0,
        "topic": null,
    })
}

fn message_to_json(m: &Message, state: &DiscordState) -> serde_json::Value {
    let author = state.users.get(&m.author_id).map(|u| user_to_json(&u)).unwrap_or_else(|| {
        serde_json::json!({ "id": m.author_id, "username": m.author_id, "discriminator": "0000" })
    });
    serde_json::json!({
        "id": m.id,
        "channel_id": m.channel_id,
        "content": m.content,
        "author": author,
        "timestamp": m.timestamp,
        "type": 0,
        "attachments": [],
        "embeds": [],
        "reactions": [],
        "mentions": [],
    })
}
