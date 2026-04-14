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
use twilight_model::channel::ChannelType;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, UserMarker};
use twilight_model::id::Id;

use crate::state::{Channel, DiscordState, Message};

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

fn discord_error(
    status: StatusCode,
    code: u32,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "code": code, "message": message })))
}

/// Extract user_id from `Authorization: Bot TOKEN` or `Authorization: Bearer TOKEN`.
fn auth_user(
    state: &DiscordState,
    headers: &HeaderMap,
) -> Result<Id<UserMarker>, (StatusCode, Json<serde_json::Value>)> {
    let raw = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = raw
        .strip_prefix("Bot ")
        .or_else(|| raw.strip_prefix("Bearer "))
        .unwrap_or(raw);
    let user_id_str = state
        .auth
        .validate(token)
        .ok_or_else(|| discord_error(StatusCode::UNAUTHORIZED, 40001, "401: Unauthorized"))?;
    user_id_str
        .parse::<u64>()
        .ok()
        .and_then(|n| Id::<UserMarker>::new_checked(n))
        .ok_or_else(|| discord_error(StatusCode::UNAUTHORIZED, 40001, "401: Unauthorized"))
}

// ---------------------------------------------------------------------------
// Auth — Spacebar-compatible /api/v10/auth/login
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginBody {
    pub login: Option<String>,
    pub password: Option<String>,
}

/// POST /api/v10/auth/login — Spacebar-compatible password login.
pub async fn login(
    State(state): State<Arc<DiscordState>>,
    Json(body): Json<LoginBody>,
) -> impl IntoResponse {
    let login = body.login.unwrap_or_default();
    let password = body.password.unwrap_or_default();

    // Match by username or by stringified user id.
    let user = state.users.iter().find(|entry| {
        entry.username == login || entry.id.get().to_string() == login
    });
    let user = match user {
        Some(u) => u,
        None => {
            return discord_error(StatusCode::UNAUTHORIZED, 50035, "Invalid login").into_response();
        }
    };
    if user.password != password {
        return discord_error(StatusCode::UNAUTHORIZED, 50035, "Invalid password").into_response();
    }

    let user_id = user.id;
    drop(user);
    let token = state.auth.create_token(&user_id.get().to_string());

    Json(serde_json::json!({
        "token": token,
        "user_id": user_id.to_string(),
        "user_settings": {},
    }))
    .into_response()
}

/// GET /api/v10/gateway — returns the WebSocket gateway URL.
pub async fn get_gateway(State(_): State<Arc<DiscordState>>) -> impl IntoResponse {
    Json(serde_json::json!({ "url": "ws://localhost:9102" }))
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
    // Look up by username; fall back to numeric identifier if it parses.
    let user_id = state
        .users
        .iter()
        .find(|u| u.username == identifier)
        .map(|u| u.id)
        .or_else(|| identifier.parse::<u64>().ok().and_then(Id::<UserMarker>::new_checked))
        .unwrap_or_else(|| Id::new(1));
    let token = state.auth.create_token(&user_id.get().to_string());
    Json(serde_json::json!({
        "result": "Success",
        "token": token,
        "user_id": user_id.to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Current user
// ---------------------------------------------------------------------------

pub async fn get_me(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => match state.users.get(&user_id) {
            Some(u) => Json(user_to_json(&u)).into_response(),
            None => Json(serde_json::json!({
                "id": user_id.to_string(),
                "username": user_id.to_string(),
                "discriminator": "0000",
                "avatar": null,
                "bot": false,
            }))
            .into_response(),
        },
    }
}

pub async fn get_user(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let parsed = user_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<UserMarker>::new_checked);
            match parsed.and_then(|id| state.users.get(&id)) {
                Some(u) => Json(user_to_json(&u)).into_response(),
                None => discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Guilds (servers)
// ---------------------------------------------------------------------------

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

pub async fn get_guild(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let parsed = guild_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<GuildMarker>::new_checked);
            match parsed.and_then(|id| state.guilds.get(&id)) {
                Some(g) => Json(guild_to_json(&g)).into_response(),
                None => discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

pub async fn get_guild_channels(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let parsed = guild_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<GuildMarker>::new_checked);
            let guild = match parsed.and_then(|id| state.guilds.get(&id)) {
                Some(g) => g,
                None => {
                    return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild")
                        .into_response();
                }
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

pub async fn get_channel(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let parsed = channel_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<ChannelMarker>::new_checked);
            match parsed.and_then(|id| state.channels.get(&id)) {
                Some(c) => Json(channel_to_json(&c)).into_response(),
                None => discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response(),
            }
        }
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

pub async fn get_messages(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let parsed = channel_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<ChannelMarker>::new_checked);
            let ch_id = match parsed {
                Some(id) => id,
                None => {
                    return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel")
                        .into_response();
                }
            };
            let msgs = state.messages.get(&ch_id).map(|v| v.clone()).unwrap_or_default();
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

pub async fn send_message(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let parsed = channel_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<ChannelMarker>::new_checked);
            let ch_id = match parsed {
                Some(id) => id,
                None => {
                    return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel")
                        .into_response();
                }
            };
            let content = body.content.unwrap_or_default();
            let msg_id_u64 = 1_000_u64 + state.messages.len() as u64;
            let msg = Message {
                id: Id::new(msg_id_u64),
                content: content.clone(),
                author_id: user_id,
                channel_id: ch_id,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let json = message_to_json(&msg, &state);
            state.messages.entry(ch_id).or_default().push(msg);
            (StatusCode::OK, Json(json)).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// DMs
// ---------------------------------------------------------------------------

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
                .filter(|c| c.channel_type == ChannelType::Private)
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

pub async fn open_dm(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Json(body): Json<OpenDmBody>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(user_id) => {
            let recipient_id = body
                .recipient_id
                .and_then(|r| r.parse::<u64>().ok())
                .and_then(Id::<UserMarker>::new_checked)
                .unwrap_or_else(|| Id::new(1));
            // Stable synthetic DM channel id: combine user halves into a u64.
            let dm_id_u64 = 10_000_u64 + user_id.get().wrapping_mul(31).wrapping_add(recipient_id.get());
            let dm_id = Id::<ChannelMarker>::new_checked(dm_id_u64).unwrap_or_else(|| Id::new(10_001));
            let ch = state.channels.entry(dm_id).or_insert_with(|| Channel {
                id: dm_id,
                name: "".into(),
                guild_id: None,
                channel_type: ChannelType::Private,
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
        "id": u.id.to_string(),
        "username": u.username,
        "discriminator": u.discriminator,
        "avatar": u.avatar,
        "bot": false,
    })
}

fn guild_to_json(g: &crate::state::Guild) -> serde_json::Value {
    serde_json::json!({
        "id": g.id.to_string(),
        "name": g.name,
        "owner_id": g.owner_id.to_string(),
        "icon": null,
        "permissions": "0",
    })
}

fn channel_to_json(c: &Channel) -> serde_json::Value {
    serde_json::json!({
        "id": c.id.to_string(),
        "name": c.name,
        "type": u8::from(c.channel_type),
        "guild_id": c.guild_id.map(|id| id.to_string()),
        "parent_id": c.parent_id.map(|id| id.to_string()),
        "position": 0,
        "topic": null,
    })
}

fn message_to_json(m: &Message, state: &DiscordState) -> serde_json::Value {
    let author = state.users.get(&m.author_id).map(|u| user_to_json(&u)).unwrap_or_else(|| {
        serde_json::json!({
            "id": m.author_id.to_string(),
            "username": m.author_id.to_string(),
            "discriminator": "0000"
        })
    });
    serde_json::json!({
        "id": m.id.to_string(),
        "channel_id": m.channel_id.to_string(),
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
