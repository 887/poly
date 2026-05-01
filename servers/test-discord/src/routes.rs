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

use crate::state::{
    AuditLogEntry, Attachment, Ban, Channel, DiscordEvent, DiscordState, ForumTag, Message,
    Role, ThreadMetadata,
};

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

/// `PATCH /api/v10/guilds/{guild_id}` — update guild fields.
///
/// Accepts partial JSON: `name`, `banner`. Returns the updated guild object.
pub async fn patch_guild(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    match auth_user(&state, &headers) {
        Err(e) => e.into_response(),
        Ok(_) => {
            let parsed = guild_id
                .parse::<u64>()
                .ok()
                .and_then(Id::<GuildMarker>::new_checked);
            match parsed {
                None => discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
                Some(id) => {
                    let mut guild = match state.guilds.get_mut(&id) {
                        Some(g) => g,
                        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
                    };
                    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
                        guild.name = name.to_string();
                    }
                    // `banner` is accepted as a URL string (test convenience) or
                    // null to clear. Real Discord expects a base64 data URI.
                    if body.get("banner").is_some() {
                        guild.banner = body["banner"].as_str().map(str::to_string);
                    }
                    Json(guild_to_json(&guild)).into_response()
                }
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
                            .is_some_and(|m| !m.archived)
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
                            .is_some_and(|m| m.archived)
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
            let limit = usize::try_from(query.limit.unwrap_or(50).min(100)).unwrap_or(50);
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
            let msg_id_u64 = 1_000_u64.saturating_add(u64::try_from(state.messages.len()).unwrap_or(u64::MAX));
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
            // Broadcast a MESSAGE_CREATE gateway event to any connected WS clients.
            state.events.publish(crate::state::DiscordEvent::MessageCreate {
                channel_id: ch_id,
                message: json.clone(),
            });
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
            let dm_id_u64 = 10_000_u64.saturating_add(user_id.get().wrapping_mul(31).wrapping_add(recipient_id.get()));
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
    state.seed_moderation();
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

// lint-allow-unused: Discord Gateway WS opcodes/sequences are bare integer literals in serde_json::json! payloads
#[allow(clippy::default_numeric_fallback)]
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
                        if let Some(text) = frame
                            && socket.send(WsMessage::Text(text.into())).await.is_err()
                        {
                            break;
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
                            let op = v.get("op").and_then(serde_json::Value::as_u64).unwrap_or(0);
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
                    Some(Ok(WsMessage::Close(_)) | Err(_)) | None => break,
                    // lint-allow-unused: any other ws frame (Binary/Ping/Pong) is silently ignored
                    #[allow(clippy::wildcard_enum_match_arm)]
                    _ => {}
                }
            }
        }
    }
}

/// Convert a `DiscordEvent` into a Discord gateway JSON frame string.
/// Returns `None` for events that don't have a gateway representation yet.
// lint-allow-unused: Gateway WS opcodes are bare integer literals + new event variants intentionally drop
#[allow(clippy::default_numeric_fallback, clippy::wildcard_enum_match_arm)]
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
// Moderation routes (B-DS)
// ---------------------------------------------------------------------------

/// `GET /api/v10/guilds/{guild_id}/members/@me` — member object for the caller.
pub async fn get_guild_member_me(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    let user_id = match auth_user(&state, &headers) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    // Get role IDs for this member.
    let roles = state
        .member_roles
        .get(&(guild_id_parsed, user_id))
        .map(|v| v.clone())
        .unwrap_or_default();
    let role_ids: Vec<String> = roles.iter().map(std::string::ToString::to_string).collect();
    Json(serde_json::json!({
        "user": { "id": user_id.to_string() },
        "roles": role_ids,
        "communication_disabled_until": null,
    }))
    .into_response()
}

/// `GET /api/v10/guilds/{guild_id}/roles` — list all roles in the guild.
pub async fn get_guild_roles(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    let roles = state
        .guild_roles
        .get(&guild_id_parsed)
        .map(|v| v.clone())
        .unwrap_or_default();
    let json: Vec<serde_json::Value> = roles.iter().map(role_to_json).collect();
    Json(json).into_response()
}

/// `DELETE /api/v10/guilds/{guild_id}/members/{user_id}` — kick a member.
pub async fn kick_member(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let moderator_id = match auth_user(&state, &headers) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    let target_id_parsed = match user_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<UserMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response(),
    };
    // Remove target from guild members list.
    if let Some(mut guild) = state.guilds.get_mut(&guild_id_parsed) {
        guild.members.retain(|&m| m != target_id_parsed);
    }
    // Log to audit log.
    let entry_id = state.next_audit_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .audit_log
        .entry(guild_id_parsed)
        .or_default()
        .insert(0, AuditLogEntry {
            id: entry_id,
            action_type: 20,
            user_id: Some(moderator_id),
            target_id: Some(user_id),
            reason: None,
        });
    StatusCode::NO_CONTENT.into_response()
}

/// `PUT /api/v10/guilds/{guild_id}/bans/{user_id}` — ban a member.
pub async fn ban_member(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let moderator_id = match auth_user(&state, &headers) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    let target_id_parsed = match user_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<UserMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response(),
    };
    let reason = body.get("reason").and_then(|v| v.as_str()).map(str::to_string);
    // Add to bans list.
    let mut bans = state.bans.entry(guild_id_parsed).or_default();
    if !bans.iter().any(|b| b.user_id == target_id_parsed) {
        bans.push(Ban { user_id: target_id_parsed, reason });
    }
    drop(bans);
    // Remove from guild members.
    if let Some(mut guild) = state.guilds.get_mut(&guild_id_parsed) {
        guild.members.retain(|&m| m != target_id_parsed);
    }
    // Log.
    let entry_id = state.next_audit_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .audit_log
        .entry(guild_id_parsed)
        .or_default()
        .insert(0, AuditLogEntry {
            id: entry_id,
            action_type: 22,
            user_id: Some(moderator_id),
            target_id: Some(user_id),
            reason: None,
        });
    StatusCode::NO_CONTENT.into_response()
}

/// `DELETE /api/v10/guilds/{guild_id}/bans/{user_id}` — unban a member.
pub async fn unban_member(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let moderator_id = match auth_user(&state, &headers) {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    let target_id_parsed = match user_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<UserMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response(),
    };
    if let Some(mut bans) = state.bans.get_mut(&guild_id_parsed) {
        bans.retain(|b| b.user_id != target_id_parsed);
    }
    // Log.
    let entry_id = state.next_audit_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state
        .audit_log
        .entry(guild_id_parsed)
        .or_default()
        .insert(0, AuditLogEntry {
            id: entry_id,
            action_type: 23,
            user_id: Some(moderator_id),
            target_id: Some(user_id),
            reason: None,
        });
    StatusCode::NO_CONTENT.into_response()
}

/// `GET /api/v10/guilds/{guild_id}/bans` — list bans.
pub async fn get_bans(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    let bans = state
        .bans
        .get(&guild_id_parsed)
        .map(|v| v.clone())
        .unwrap_or_default();
    let json: Vec<serde_json::Value> = bans
        .iter()
        .map(|b| {
            let user = state.users.get(&b.user_id).map_or_else(
                || serde_json::json!({
                    "id": b.user_id.to_string(),
                    "username": b.user_id.to_string(),
                    "discriminator": "0000",
                }),
                |u| user_to_json(&u),
            );
            serde_json::json!({
                "reason": b.reason,
                "user": user,
            })
        })
        .collect();
    Json(json).into_response()
}

/// `PATCH /api/v10/guilds/{guild_id}/members/{user_id}` — set timeout.
pub async fn patch_guild_member(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path((guild_id, user_id)): Path<(String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    // For the test server we just acknowledge the request.
    // A real implementation would store the timeout on the member record.
    let _ = (guild_id, user_id, body, state);
    StatusCode::OK.into_response()
}

/// `DELETE /api/v10/channels/{channel_id}/messages/{message_id}` — delete a message.
pub async fn delete_message(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let ch_id = match channel_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<ChannelMarker>::new_checked)
    {
        Some(id) => id,
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response()
        }
    };
    let msg_id = match message_id
        .parse::<u64>()
        .ok()
        .and_then(twilight_model::id::Id::<twilight_model::id::marker::MessageMarker>::new_checked)
    {
        Some(id) => id,
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10008, "Unknown Message").into_response()
        }
    };
    if let Some(mut msgs) = state.messages.get_mut(&ch_id) {
        msgs.retain(|m| m.id != msg_id);
    }
    StatusCode::NO_CONTENT.into_response()
}

/// `PATCH /api/v10/channels/{channel_id}` — update channel metadata.
pub async fn patch_channel(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let ch_id = match channel_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<ChannelMarker>::new_checked)
    {
        Some(id) => id,
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response()
        }
    };
    let mut ch = match state.channels.get_mut(&ch_id) {
        Some(c) => c,
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response()
        }
    };
    if let Some(name) = body.get("name").and_then(|v| v.as_str()) {
        ch.name = name.to_string();
    }
    let result = channel_to_json(&ch);
    drop(ch);
    Json(result).into_response()
}

/// `PATCH /api/v10/guilds/{guild_id}/channels` — reorder channels.
pub async fn reorder_guild_channels(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
    Json(body): Json<Vec<serde_json::Value>>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let _ = (guild_id, body, state);
    StatusCode::NO_CONTENT.into_response()
}

/// `GET /api/v10/guilds/{guild_id}/audit-logs` — moderation log.
pub async fn get_audit_log(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(guild_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let guild_id_parsed = match guild_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<GuildMarker>::new_checked)
    {
        Some(id) => id,
        None => return discord_error(StatusCode::NOT_FOUND, 10004, "Unknown Guild").into_response(),
    };
    let entries = state
        .audit_log
        .get(&guild_id_parsed)
        .map(|v| v.clone())
        .unwrap_or_default();

    // Collect unique user IDs from entries to embed in the response.
    let mut seen_users: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for e in &entries {
        if let Some(uid) = e.user_id {
            seen_users.insert(uid.get());
        }
    }
    let users: Vec<serde_json::Value> = seen_users
        .iter()
        .filter_map(|&uid| {
            let id = Id::<UserMarker>::new_checked(uid)?;
            state.users.get(&id).map(|u| user_to_json(&u))
        })
        .collect();

    let audit_log_entries: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id.to_string(),
                "action_type": e.action_type,
                "user_id": e.user_id.map(|id| id.to_string()),
                "target_id": e.target_id,
                "reason": e.reason,
            })
        })
        .collect();

    Json(serde_json::json!({
        "audit_log_entries": audit_log_entries,
        "users": users,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Social / Relationship routes
// ---------------------------------------------------------------------------

/// `PUT /api/v10/users/@me/relationships/{user_id}` — add friend (type=1) or block (type=2).
pub async fn put_relationship(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    // Validate the user_id parses as a known user (404 if not found).
    let parsed = user_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<UserMarker>::new_checked);
    if let Some(uid) = parsed
        && state.users.get(&uid).is_none()
    {
        return discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response();
    }
    let rel_type = body.get("type").and_then(serde_json::Value::as_u64).unwrap_or(1);
    tracing::debug!(target: "poly_test_discord::relationships", user_id, rel_type, "PUT relationship");
    StatusCode::NO_CONTENT.into_response()
}

/// `DELETE /api/v10/users/@me/relationships/{user_id}` — remove friend or unblock.
pub async fn delete_relationship(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let parsed = user_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<UserMarker>::new_checked);
    if let Some(uid) = parsed
        && state.users.get(&uid).is_none()
    {
        return discord_error(StatusCode::NOT_FOUND, 10013, "Unknown User").into_response();
    }
    tracing::debug!(target: "poly_test_discord::relationships", user_id, "DELETE relationship");
    StatusCode::NO_CONTENT.into_response()
}

/// `PUT /api/v10/users/@me/notes/{user_id}` — set or clear a private user note.
pub async fn put_user_note(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let note = body.get("note").and_then(|v| v.as_str()).unwrap_or("");
    tracing::debug!(target: "poly_test_discord::notes", user_id, note, "PUT user note");
    StatusCode::NO_CONTENT.into_response()
}

/// `DELETE /api/v10/channels/{channel_id}` — close a DM or leave a group DM.
pub async fn delete_channel(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let ch_id = match channel_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<ChannelMarker>::new_checked)
    {
        Some(id) => id,
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response()
        }
    };
    match state.channels.get(&ch_id) {
        Some(ch) => {
            let ch_type = ch.channel_type;
            drop(ch);
            // Only allow deleting DM / Group DM channels in the test server.
            if ch_type == ChannelType::Private || ch_type == ChannelType::Group {
                state.channels.remove(&ch_id);
                state.messages.remove(&ch_id);
            }
        }
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response()
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// `PUT /api/v10/channels/{channel_id}/recipients/{user_id}` — add user to group DM.
pub async fn put_group_dm_recipient(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path((channel_id, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    tracing::debug!(target: "poly_test_discord::group_dm", channel_id, user_id, "PUT group DM recipient");
    StatusCode::NO_CONTENT.into_response()
}

/// `POST /api/v10/channels/{channel_id}/invites` — create a channel invite.
///
/// Returns a minimal invite object with a synthetic code.
// lint-allow-unused: serde_json::json! macros use bare integer literals for invite metadata fields
#[allow(clippy::default_numeric_fallback)]
pub async fn create_invite(
    State(state): State<Arc<DiscordState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = auth_user(&state, &headers) {
        return e.into_response();
    }
    let ch_id = match channel_id
        .parse::<u64>()
        .ok()
        .and_then(Id::<ChannelMarker>::new_checked)
    {
        Some(id) => id,
        None => {
            return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response()
        }
    };
    if state.channels.get(&ch_id).is_none() {
        return discord_error(StatusCode::NOT_FOUND, 10003, "Unknown Channel").into_response();
    }
    // Return a synthetic invite code based on channel ID.
    let code = format!("test-{channel_id}");
    Json(serde_json::json!({
        "code": code,
        "channel": { "id": channel_id },
        "guild": null,
        "inviter": { "id": "1", "username": "koala" },
        "max_age": 86400,
        "max_uses": 0,
        "uses": 0,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// JSON serializers
// ---------------------------------------------------------------------------

fn role_to_json(r: &Role) -> serde_json::Value {
    serde_json::json!({
        "id": r.id.to_string(),
        "name": r.name,
        "permissions": r.permissions,
        "position": r.position,
        "color": r.color,
    })
}

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
        "banner": g.banner,
        "permissions": "0",
    })
}

/// Serves bundled demo avatar bytes for the seeded fixture users so chat
/// avatars resolve in dev. Real Discord serves these from cdn.discordapp.com;
/// the test server stands in by mapping the avatar hash to embedded asset
/// bytes. The `_user_id` is part of the URL shape but not used for lookup.
pub async fn serve_avatar(
    Path((_user_id, file)): Path<(String, String)>,
) -> impl IntoResponse {
    static KOALA_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/koala.png");
    static KANGAROO_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/kangaroo.png");
    static PLATYPUS_PNG: &[u8] = include_bytes!("../../../clients/demo/assets/platypus.png");

    let hash = file.trim_end_matches(".png");
    let bytes: &[u8] = match hash {
        "koala" => KOALA_PNG,
        "kangaroo" => KANGAROO_PNG,
        "platypus" => PLATYPUS_PNG,
        _ => return (StatusCode::NOT_FOUND, "unknown avatar").into_response(),
    };
    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        bytes,
    )
        .into_response()
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

// lint-allow-unused: serde_json::json! macros use bare integer literals for channel position/type fields
#[allow(clippy::default_numeric_fallback)]
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

// lint-allow-unused: serde_json::json! macros use bare integer literals for message type fields
#[allow(clippy::default_numeric_fallback)]
fn message_to_json(m: &Message, state: &DiscordState) -> serde_json::Value {
    let author = state.users.get(&m.author_id).map_or_else(
        || serde_json::json!({
            "id": m.author_id.to_string(),
            "username": m.author_id.to_string(),
            "discriminator": "0000"
        }),
        |u| user_to_json(&u),
    );

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
    if let Some(thread_id) = m.thread_id
        && let Some(thread_ch) = state.channels.get(&thread_id)
    {
        obj["thread"] = channel_to_json(&thread_ch);
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
