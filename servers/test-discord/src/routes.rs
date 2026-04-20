//! Mock Discord REST API v10 handlers.
//!
//! Implements the subset of the Discord API that `poly-discord` calls.
//! Auth: `Authorization: Bot TOKEN` or `Authorization: Bearer TOKEN`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use twilight_model::channel::ChannelType;
use twilight_model::id::marker::{ChannelMarker, GuildMarker, UserMarker};
use twilight_model::id::Id;

use crate::state::{Attachment, Channel, DiscordEvent, DiscordState, ForumTag, Message, ThreadMetadata};

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
        .and_then(Id::<UserMarker>::new_checked)
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
///
/// The URL is dynamically set on the state so that in-process tests can
/// point clients at the correct random port.
pub async fn get_gateway(State(state): State<Arc<DiscordState>>) -> impl IntoResponse {
    let url = state.gateway_url.read().await.clone();
    Json(serde_json::json!({ "url": url }))
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
// Threads — active + archived
// ---------------------------------------------------------------------------

/// GET /api/v10/guilds/{guild_id}/threads/active
pub async fn get_guild_active_threads(
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
            if parsed.and_then(|id| state.guilds.get(&id)).is_none() {
                return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response();
            }
            let guild_id_val = match parsed {
                Some(id) => id,
                None => {
                    return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild")
                        .into_response();
                }
            };
            // Collect active (non-archived) threads belonging to this guild.
            let threads: Vec<serde_json::Value> = state
                .channels
                .iter()
                .filter(|c| {
                    c.guild_id == Some(guild_id_val)
                        && is_thread_type(c.channel_type)
                        && c.thread_metadata
                            .as_ref()
                            .map(|m| !m.archived)
                            .unwrap_or(false)
                })
                .map(|c| channel_to_json(&c))
                .collect();
            Json(serde_json::json!({ "threads": threads, "has_more": false })).into_response()
        }
    }
}

/// GET /api/v10/channels/{channel_id}/threads/archived/public
pub async fn get_channel_archived_threads(
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
            let ch_id = match parsed {
                Some(id) => id,
                None => {
                    return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel")
                        .into_response();
                }
            };
            if state.channels.get(&ch_id).is_none() {
                return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel")
                    .into_response();
            }
            // Collect archived public threads whose parent is this channel.
            let threads: Vec<serde_json::Value> = state
                .channels
                .iter()
                .filter(|c| {
                    c.parent_id == Some(ch_id)
                        && c.channel_type == ChannelType::PublicThread
                        && c.thread_metadata
                            .as_ref()
                            .map(|m| m.archived)
                            .unwrap_or(false)
                })
                .map(|c| channel_to_json(&c))
                .collect();
            Json(serde_json::json!({ "threads": threads, "has_more": false })).into_response()
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
                attachments: vec![],
                thread_id: None,
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
                available_tags: vec![],
                default_forum_layout: None,
                applied_tags: vec![],
                thread_metadata: None,
                owner_id: None,
                message_count: None,
                member_count: None,
                thread_message_id: None,
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
// Gateway WebSocket — GET /gateway/ws
// ---------------------------------------------------------------------------

/// GET /gateway/ws — upgrade to WebSocket, speak a minimal Discord gateway protocol.
///
/// Protocol decisions (Phase 6.5 minimum viable gateway):
/// - Skip Hello (op 10) / Heartbeat / Resume / sharding — not needed for tests.
/// - On connection: immediately send `READY` (op 0, t "READY").
/// - IDENTIFY frames from the client are accepted but ignored (no auth check).
/// - HEARTBEAT frames (op 1) from the client receive a HEARTBEAT_ACK (op 11).
/// - Thread lifecycle events arrive via the `EventBus<DiscordEvent>` subscription.
pub async fn gateway_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DiscordState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_gateway_socket(socket, state))
}

async fn handle_gateway_socket(mut socket: WebSocket, state: Arc<DiscordState>) {
    // Subscribe to events before sending READY so no events are missed.
    let mut rx: broadcast::Receiver<DiscordEvent> = state.events.subscribe();

    // Send a minimal READY event so the client knows we accepted the session.
    let ready = serde_json::json!({
        "op": 0,
        "t": "READY",
        "s": 1,
        "d": {
            "v": 10,
            "user": { "id": "0", "username": "test-gateway", "discriminator": "0000" },
            "guilds": [],
            "session_id": "mock-session",
            "resume_gateway_url": ""
        }
    });
    if socket
        .send(WsMessage::Text(ready.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            // Forward broadcast events to this WS client.
            event_result = rx.recv() => {
                match event_result {
                    Ok(event) => {
                        let frame = discord_event_to_ws_frame(&event);
                        if let Some(text) = frame {
                            if socket.send(WsMessage::Text(text.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(target: "poly_test_discord::gateway", "ws client lagged by {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Handle incoming frames from the client.
            msg = socket.recv() => {
                match msg {
                    Some(Ok(WsMessage::Text(txt))) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                            let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
                            // op 1 = HEARTBEAT → reply with HEARTBEAT_ACK (op 11)
                            if op == 1 {
                                let ack = serde_json::json!({ "op": 11 });
                                if socket.send(WsMessage::Text(ack.to_string().into())).await.is_err() {
                                    break;
                                }
                            }
                            // op 2 = IDENTIFY → accepted silently
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

/// Convert a `DiscordEvent` into a Discord gateway JSON frame string.
/// Returns `None` for events that don't have a gateway representation yet.
fn discord_event_to_ws_frame(event: &DiscordEvent) -> Option<String> {
    let (event_name, data) = match event {
        DiscordEvent::ThreadCreate { thread } => (
            "THREAD_CREATE",
            thread.clone(),
        ),
        DiscordEvent::ThreadUpdate { thread } => (
            "THREAD_UPDATE",
            thread.clone(),
        ),
        DiscordEvent::ThreadDelete { thread_id, guild_id, parent_id } => (
            "THREAD_DELETE",
            serde_json::json!({
                "id": thread_id,
                "guild_id": guild_id,
                "parent_id": parent_id,
                "type": 11
            }),
        ),
        DiscordEvent::ThreadListSync { guild_id, threads } => (
            "THREAD_LIST_SYNC",
            serde_json::json!({
                "guild_id": guild_id,
                "threads": threads,
                "members": []
            }),
        ),
        DiscordEvent::MessageCreate { message, .. } => ("MESSAGE_CREATE", message.clone()),
        _ => return None,
    };

    let frame = serde_json::json!({
        "op": 0,
        "t": event_name,
        "s": null,
        "d": data
    });
    Some(frame.to_string())
}

// ---------------------------------------------------------------------------
// Testhook — POST /testhook/emit_thread_event
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct EmitThreadEventBody {
    /// One of: THREAD_CREATE, THREAD_UPDATE, THREAD_DELETE, THREAD_LIST_SYNC
    pub event_type: String,
    /// Full thread channel JSON (required for THREAD_CREATE / THREAD_UPDATE / THREAD_LIST_SYNC threads)
    pub thread: Option<serde_json::Value>,
    /// Required for THREAD_DELETE
    pub thread_id: Option<String>,
    pub guild_id: Option<String>,
    pub parent_id: Option<String>,
    /// Required for THREAD_LIST_SYNC
    pub threads: Option<Vec<serde_json::Value>>,
}

/// POST /testhook/emit_thread_event — inject a gateway thread event to all connected WS clients.
pub async fn emit_thread_event(
    State(state): State<Arc<DiscordState>>,
    Json(body): Json<EmitThreadEventBody>,
) -> impl IntoResponse {
    let event = match body.event_type.as_str() {
        "THREAD_CREATE" => {
            let thread = match body.thread {
                Some(t) => t,
                None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "thread required" }))).into_response(),
            };
            DiscordEvent::ThreadCreate { thread }
        }
        "THREAD_UPDATE" => {
            let thread = match body.thread {
                Some(t) => t,
                None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "thread required" }))).into_response(),
            };
            DiscordEvent::ThreadUpdate { thread }
        }
        "THREAD_DELETE" => {
            let thread_id = body.thread_id.unwrap_or_default();
            let guild_id = body.guild_id.unwrap_or_default();
            let parent_id = body.parent_id.unwrap_or_default();
            if thread_id.is_empty() {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "thread_id required" }))).into_response();
            }
            DiscordEvent::ThreadDelete { thread_id, guild_id, parent_id }
        }
        "THREAD_LIST_SYNC" => {
            let guild_id = body.guild_id.unwrap_or_default();
            let threads = body.threads.unwrap_or_default();
            DiscordEvent::ThreadListSync { guild_id, threads }
        }
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": format!("unknown event_type: {other}") })),
            ).into_response();
        }
    };

    let receivers = state.events.publish(event);
    Json(serde_json::json!({ "ok": true, "receivers": receivers })).into_response()
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

fn forum_tag_to_json(t: &ForumTag) -> serde_json::Value {
    serde_json::json!({
        "id": t.id.to_string(),
        "name": t.name,
        "moderated": t.moderated,
        "emoji_id": null,
        "emoji_name": t.emoji_name,
    })
}

fn thread_metadata_to_json(m: &ThreadMetadata) -> serde_json::Value {
    serde_json::json!({
        "archived": m.archived,
        "locked": m.locked,
        "auto_archive_duration": m.auto_archive_duration,
        "archive_timestamp": m.archive_timestamp,
        "create_timestamp": m.create_timestamp,
    })
}

fn attachment_to_json(a: &Attachment) -> serde_json::Value {
    serde_json::json!({
        "id": a.id.to_string(),
        "filename": a.filename,
        "content_type": a.content_type,
        "size": a.size,
        "url": a.url,
        "proxy_url": a.proxy_url,
        "width": a.width,
        "height": a.height,
    })
}

fn channel_to_json(c: &Channel) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "id": c.id.to_string(),
        "name": c.name,
        "type": u8::from(c.channel_type),
        "guild_id": c.guild_id.map(|id| id.to_string()),
        "parent_id": c.parent_id.map(|id| id.to_string()),
        "position": 0,
        "topic": null,
    });

    // Forum channel fields
    if !c.available_tags.is_empty() {
        let tags: Vec<serde_json::Value> = c.available_tags.iter().map(forum_tag_to_json).collect();
        obj["available_tags"] = serde_json::Value::Array(tags);
    } else {
        obj["available_tags"] = serde_json::json!([]);
    }
    if let Some(layout) = c.default_forum_layout {
        obj["default_forum_layout"] = serde_json::json!(layout);
    }

    // Thread fields
    if !c.applied_tags.is_empty() {
        let tags: Vec<serde_json::Value> = c.applied_tags.iter().map(|id| serde_json::json!(id.to_string())).collect();
        obj["applied_tags"] = serde_json::Value::Array(tags);
    } else {
        obj["applied_tags"] = serde_json::json!([]);
    }
    if let Some(ref meta) = c.thread_metadata {
        obj["thread_metadata"] = thread_metadata_to_json(meta);
    }
    if let Some(owner_id) = c.owner_id {
        obj["owner_id"] = serde_json::json!(owner_id.to_string());
    }
    if let Some(mc) = c.message_count {
        obj["message_count"] = serde_json::json!(mc);
    }
    if let Some(mc) = c.member_count {
        obj["member_count"] = serde_json::json!(mc);
    }

    obj
}

fn message_to_json(m: &Message, state: &DiscordState) -> serde_json::Value {
    let author = state.users.get(&m.author_id).map(|u| user_to_json(&u)).unwrap_or_else(|| {
        serde_json::json!({
            "id": m.author_id.to_string(),
            "username": m.author_id.to_string(),
            "discriminator": "0000"
        })
    });

    let attachments: Vec<serde_json::Value> = m.attachments.iter().map(attachment_to_json).collect();

    let mut obj = serde_json::json!({
        "id": m.id.to_string(),
        "channel_id": m.channel_id.to_string(),
        "content": m.content,
        "author": author,
        "timestamp": m.timestamp,
        "type": 0,
        "attachments": attachments,
        "embeds": [],
        "reactions": [],
        "mentions": [],
    });

    // Inline thread reference
    if let Some(thread_id) = m.thread_id {
        if let Some(thread_ch) = state.channels.get(&thread_id) {
            obj["thread"] = channel_to_json(&thread_ch);
        }
    }

    obj
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_thread_type(ct: ChannelType) -> bool {
    matches!(
        ct,
        ChannelType::PublicThread
            | ChannelType::PrivateThread
            | ChannelType::AnnouncementThread
    )
}
