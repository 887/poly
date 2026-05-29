//! Matrix Client-Server API route handlers.
//!
//! Implements the subset of the Matrix CS API that poly-matrix calls.
//! All handlers take `State<std::sync::Arc<MatrixState>>` and return JSON responses.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::state::MatrixState;
use poly_test_common::TokenAuth;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bearer_user(state: &MatrixState, headers: &HeaderMap) -> Result<String, impl IntoResponse> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok());
    state
        .auth
        .extract_user_id(auth_header)
        .ok_or_else(|| matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid or missing access token"))
}

fn matrix_error(
    status: StatusCode,
    errcode: &str,
    error: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "errcode": errcode,
            "error": error,
        })),
    )
}

// ---------------------------------------------------------------------------
// Auth endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub identifier: Option<LoginIdentifier>,
    pub password: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginIdentifier {
    pub user: Option<String>,
}

/// POST /_matrix/client/v3/login
pub async fn login(
    State(state): State<std::sync::Arc<MatrixState>>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    let username = body
        .identifier
        .and_then(|id| id.user)
        .unwrap_or_default();
    let password = body.password.unwrap_or_default();

    // Build the full user ID if not already qualified
    let user_id = if username.starts_with('@') {
        username
    } else {
        format!("@{username}:localhost")
    };

    let user = match state.users.get(&user_id) {
        Some(u) => u,
        None => {
            return matrix_error(StatusCode::FORBIDDEN, "M_FORBIDDEN", "Unknown user").into_response();
        }
    };

    if user.password != password {
        return matrix_error(StatusCode::FORBIDDEN, "M_FORBIDDEN", "Invalid password").into_response();
    }

    let device_id = user.device_id.clone();
    drop(user);

    let token = state.auth.create_token(&user_id);

    Json(serde_json::json!({
        "user_id": user_id,
        "access_token": token,
        "device_id": device_id,
    }))
    .into_response()
}

/// POST /_matrix/client/v3/logout
pub async fn logout(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok());
    if let Some(header) = auth_header {
        let token = header.strip_prefix("Bearer ").unwrap_or(header);
        state.auth.revoke(token);
    }
    Json(serde_json::json!({}))
}

/// GET /_matrix/client/v3/account/whoami
pub async fn whoami(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    let device_id = state
        .users
        .get(&user_id)
        .map(|u| u.device_id.clone())
        .unwrap_or_default();
    Json(serde_json::json!({
        "user_id": user_id,
        "device_id": device_id,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

/// GET /_matrix/client/v3/profile/:user_id
pub async fn get_profile(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    match state.users.get(&user_id) {
        Some(user) => Json(serde_json::json!({
            "displayname": user.displayname,
            "avatar_url": user.avatar_url,
        }))
        .into_response(),
        None => matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "User not found").into_response(),
    }
}

// ---------------------------------------------------------------------------
// Rooms
// ---------------------------------------------------------------------------

/// GET /_matrix/client/v3/joined_rooms
pub async fn joined_rooms(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    let rooms: Vec<String> = state
        .rooms
        .iter()
        .filter(|entry| entry.members.contains(&user_id))
        .map(|entry| entry.room_id.clone())
        .collect();

    Json(serde_json::json!({ "joined_rooms": rooms })).into_response()
}

/// GET /_matrix/client/v3/rooms/:room_id/state
pub async fn room_state(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    match state.rooms.get(&room_id) {
        Some(room) => Json(room.state_events.clone()).into_response(),
        None => matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    }
}

/// GET /_matrix/client/v3/rooms/:room_id/members
/// Supports optional `?membership=<value>` filter.
pub async fn room_members(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Query(params): Query<RoomMembersQuery>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let membership_filter = params.membership.as_deref();

    // When membership=ban, return banned_members table as m.room.member events.
    if membership_filter == Some("ban") {
        let banned = state
            .banned_members
            .get(&room_id)
            .map(|b| b.clone())
            .unwrap_or_default();

        let chunk: Vec<serde_json::Value> = banned
            .iter()
            .map(|entry| {
                let displayname = state
                    .users
                    .get(&entry.user_id)
                    .map(|u| u.displayname.clone());
                serde_json::json!({
                    "type": "m.room.member",
                    "state_key": entry.user_id,
                    "sender": "@server:localhost",
                    "event_id": format!("$ban-{}", entry.user_id),
                    "content": {
                        "membership": "ban",
                        "displayname": displayname,
                        "reason": entry.reason,
                    },
                })
            })
            .collect();

        return Json(serde_json::json!({ "chunk": chunk })).into_response();
    }

    let room = match state.rooms.get(&room_id) {
        Some(r) => r,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    let member_events: Vec<serde_json::Value> = room
        .state_events
        .iter()
        .filter(|ev| ev.get("type").and_then(|t| t.as_str()) == Some("m.room.member"))
        .filter(|ev| {
            if let Some(filter) = membership_filter {
                ev.get("content")
                    .and_then(|c| c.get("membership"))
                    .and_then(serde_json::Value::as_str)
                    == Some(filter)
            } else {
                true
            }
        })
        .cloned()
        .collect();

    Json(serde_json::json!({ "chunk": member_events })).into_response()
}

#[derive(Deserialize)]
pub struct RoomMembersQuery {
    pub membership: Option<String>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct MessagesQuery {
    pub from: Option<String>,
    pub dir: Option<String>,
    pub limit: Option<usize>,
}

/// GET /_matrix/client/v3/rooms/:room_id/messages
pub async fn get_messages(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Query(params): Query<MessagesQuery>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let timeline = match state.timelines.get(&room_id) {
        Some(t) => t.clone(),
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    let limit = params.limit.unwrap_or(50).min(100);
    let dir = params.dir.as_deref().unwrap_or("b");

    // Parse `from` as a numeric index into the timeline; default to end for backwards
    let from_idx: usize = params
        .from
        .as_ref()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| if dir == "b" { timeline.len() } else { 0 });

    let chunk: Vec<serde_json::Value> = if dir == "b" {
        // Backwards: take `limit` events before `from_idx`
        let start = from_idx.saturating_sub(limit);
        timeline.get(start..from_idx).unwrap_or_default().iter().rev().cloned().collect()
    } else {
        // Forwards: take `limit` events from `from_idx`
        let end = from_idx.saturating_add(limit).min(timeline.len());
        timeline.get(from_idx..end).unwrap_or_default().to_vec()
    };

    let end_token = if dir == "b" {
        from_idx.saturating_sub(limit).to_string()
    } else {
        from_idx.saturating_add(chunk.len()).to_string()
    };

    Json(serde_json::json!({
        "chunk": chunk,
        "start": params.from.as_deref().unwrap_or("0"),
        "end": end_token,
    }))
    .into_response()
}

/// PUT /_matrix/client/v3/rooms/:room_id/send/:event_type/:txn_id
pub async fn send_message(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path((room_id, _event_type, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let user_id = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    // Verify room exists and user is a member
    {
        let room = match state.rooms.get(&room_id) {
            Some(r) => r,
            None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
        };
        if !room.members.contains(&user_id) {
            return matrix_error(StatusCode::FORBIDDEN, "M_FORBIDDEN", "Not a member of this room").into_response();
        }
    }

    let event_id = state.next_event_id();
    let event = serde_json::json!({
        "type": "m.room.message",
        "event_id": &event_id,
        "sender": &user_id,
        "origin_server_ts": chrono::Utc::now().timestamp_millis(),
        "content": body,
    });

    // Append to timeline
    if let Some(mut timeline) = state.timelines.get_mut(&room_id) {
        timeline.push(event.clone());
    }

    // Broadcast to sync waiters
    state.events.publish(crate::state::MatrixEvent::Timeline {
        room_id,
        event,
    });

    Json(serde_json::json!({ "event_id": event_id })).into_response()
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SyncQuery {
    pub since: Option<String>,
    pub timeout: Option<u64>,
}

/// GET /_matrix/client/v3/sync
pub async fn sync(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Query(params): Query<SyncQuery>,
) -> impl IntoResponse {
    let user_id = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    let since: u64 = params
        .since
        .as_ref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let timeout_ms = params.timeout.unwrap_or(0);

    // Collect rooms the user is in
    let user_rooms: Vec<String> = state
        .rooms
        .iter()
        .filter(|entry| entry.members.contains(&user_id))
        .map(|entry| entry.room_id.clone())
        .collect();

    // If we have a since token and timeout, do long-poll: wait for new events
    if since > 0 && timeout_ms > 0 {
        let mut rx = state.events.subscribe();
        // lint-allow-unused: Instant + bounded Duration cannot overflow within long-poll budget
        #[allow(clippy::arithmetic_side_effects)]
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);

        // Wait for an event or timeout
        let got_event = tokio::select! {
            result = rx.recv() => result.is_ok(),
            () = tokio::time::sleep_until(deadline) => false,
        };

        if !got_event {
            // Timeout with no events — return empty sync
            let token = state.sync_token();
            return Json(serde_json::json!({
                "next_batch": token,
                "rooms": { "join": {} },
            }))
            .into_response();
        }
    }

    // Build full sync response with all rooms the user is in
    let mut join = serde_json::Map::new();

    for room_id in &user_rooms {
        let room = match state.rooms.get(room_id) {
            Some(r) => r,
            None => continue,
        };

        let (timeline_events, timeline_len) = state
            .timelines
            .get(room_id)
            .map(|t| {
                // For initial sync (since=0), return all events.
                // For incremental sync, filter by the global seq embedded in the
                // event_id ("$evt{N}") — only return events where N > since.
                let events = if since == 0 {
                    t.clone()
                } else {
                    t.iter()
                        .filter(|e| {
                            e.get("event_id")
                                .and_then(|id| id.as_str())
                                .and_then(|s| s.strip_prefix("$evt"))
                                .and_then(|n| n.parse::<u64>().ok())
                                .unwrap_or(0)
                                > since
                        })
                        .cloned()
                        .collect()
                };
                let len = t.len();
                (events, len)
            })
            .unwrap_or_default();

        // Only include state for initial sync
        let state_events: Vec<serde_json::Value> = if since == 0 {
            room.state_events.clone()
        } else {
            vec![]
        };

        // prev_batch points to the end of the timeline so that a backwards
        // /messages request starting there returns all seeded messages.
        let prev_batch = timeline_len.to_string();

        join.insert(
            room_id.clone(),
            serde_json::json!({
                "timeline": {
                    "events": timeline_events,
                    "prev_batch": prev_batch,
                    "limited": false,
                },
                "state": {
                    "events": state_events,
                },
                "ephemeral": {
                    "events": [],
                },
            }),
        );
    }

    // Account data (initial sync only)
    let account_data_events: Vec<serde_json::Value> = if since == 0 {
        state
            .account_data
            .get(&user_id)
            .map(|data| {
                data.iter()
                    .map(|entry| {
                        serde_json::json!({
                            "type": entry.key().clone(),
                            "content": entry.value().clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let token = state.sync_token();

    Json(serde_json::json!({
        "next_batch": token,
        "rooms": {
            "join": join,
        },
        "account_data": {
            "events": account_data_events,
        },
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Space hierarchy
// ---------------------------------------------------------------------------

/// GET /_matrix/client/v1/rooms/:room_id/hierarchy
pub async fn space_hierarchy(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let space = match state.rooms.get(&room_id) {
        Some(r) if r.is_space => r,
        Some(_) => return matrix_error(StatusCode::BAD_REQUEST, "M_BAD_REQUEST", "Not a space").into_response(),
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    // Collect child room IDs from m.space.child state events
    let child_ids: Vec<String> = space
        .state_events
        .iter()
        .filter(|ev| ev.get("type").and_then(|t| t.as_str()) == Some("m.space.child"))
        .filter_map(|ev| ev.get("state_key").and_then(|k| k.as_str()).map(std::string::ToString::to_string))
        .collect();

    let space_name = space.name.clone();
    let space_members = space.members.len();
    drop(space);

    // Build rooms list: space itself + children
    let mut rooms = vec![serde_json::json!({
        "room_id": room_id,
        "name": space_name,
        "num_joined_members": space_members,
        "room_type": "m.space",
        "children_state": child_ids.iter().map(|cid| serde_json::json!({
            "type": "m.space.child",
            "state_key": cid,
            "content": { "via": ["localhost"] },
        })).collect::<Vec<_>>(),
    })];

    for child_id in &child_ids {
        if let Some(child) = state.rooms.get(child_id) {
            rooms.push(serde_json::json!({
                "room_id": child.room_id,
                "name": child.name,
                "topic": child.topic,
                "num_joined_members": child.members.len(),
                "room_type": if child.is_space { Some("m.space") } else { None::<&str> },
                "children_state": [],
            }));
        }
    }

    Json(serde_json::json!({ "rooms": rooms })).into_response()
}

// ---------------------------------------------------------------------------
// Public rooms & join
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PublicRoomsQuery {
    pub limit: Option<usize>,
    pub since: Option<String>,
}

/// GET /_matrix/client/v3/publicRooms
pub async fn public_rooms(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Query(params): Query<PublicRoomsQuery>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let limit = params.limit.unwrap_or(50);

    let rooms: Vec<serde_json::Value> = state
        .rooms
        .iter()
        .filter(|entry| !entry.is_space) // Exclude spaces from public listing
        .take(limit)
        .map(|entry| {
            serde_json::json!({
                "room_id": entry.room_id,
                "name": entry.name,
                "topic": entry.topic,
                "num_joined_members": entry.members.len(),
                "avatar_url": entry.avatar_url,
            })
        })
        .collect();

    let total = rooms.len();

    Json(serde_json::json!({
        "chunk": rooms,
        "total_room_count_estimate": total,
    }))
    .into_response()
}

/// POST /_matrix/client/v3/join/:room_id_or_alias
pub async fn join_room(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
) -> impl IntoResponse {
    let user_id = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    // For our mock, room_id_or_alias is always a room_id
    let room_id = room_id_or_alias;

    let mut room = match state.rooms.get_mut(&room_id) {
        Some(r) => r,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    if !room.members.contains(&user_id) {
        room.members.push(user_id.clone());

        // Add m.room.member state event
        let displayname = state.users.get(&user_id).map(|u| u.displayname.clone());
        let avatar_url = state.users.get(&user_id).and_then(|u| u.avatar_url.clone());

        room.state_events.push(serde_json::json!({
            "type": "m.room.member",
            "state_key": &user_id,
            "content": {
                "membership": "join",
                "displayname": displayname,
                "avatar_url": avatar_url,
            },
            "sender": &user_id,
            "event_id": state.next_event_id(),
        }));
    }

    Json(serde_json::json!({ "room_id": room_id })).into_response()
}

// ---------------------------------------------------------------------------
// Moderation (B-MX)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct KickBanRequest {
    pub user_id: String,
    pub reason: Option<String>,
}

#[derive(Deserialize)]
pub struct UnbanRequest {
    pub user_id: String,
}

/// POST /_matrix/client/v3/rooms/:room_id/kick
pub async fn kick_member(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<KickBanRequest>,
) -> impl IntoResponse {
    let actor = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    let mut room = match state.rooms.get_mut(&room_id) {
        Some(r) => r,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    if !room.members.contains(&actor) {
        return matrix_error(StatusCode::FORBIDDEN, "M_FORBIDDEN", "Not a member of this room").into_response();
    }

    room.members.retain(|m| *m != body.user_id);

    // Update existing m.room.member state event in-place (state events are keyed by
    // state_key; in-place update ensures /members returns a single authoritative event).
    let event_id = state.next_event_id();
    let mut found = false;
    for ev in room.state_events.iter_mut() {
        if ev.get("type").and_then(serde_json::Value::as_str) == Some("m.room.member")
            && ev.get("state_key").and_then(serde_json::Value::as_str)
                == Some(body.user_id.as_str())
        {
            if let Some(obj) = ev.as_object_mut() {
                obj.insert(
                    "content".to_string(),
                    serde_json::json!({ "membership": "leave", "reason": body.reason }),
                );
                obj.insert("event_id".to_string(), serde_json::json!(&event_id));
            }
            found = true;
            break;
        }
    }
    if !found {
        room.state_events.push(serde_json::json!({
            "type": "m.room.member",
            "state_key": body.user_id,
            "content": { "membership": "leave", "reason": body.reason },
            "sender": actor,
            "event_id": event_id,
        }));
    }

    Json(serde_json::json!({})).into_response()
}

/// POST /_matrix/client/v3/rooms/:room_id/ban
pub async fn ban_member(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<KickBanRequest>,
) -> impl IntoResponse {
    let actor = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    {
        let mut room = match state.rooms.get_mut(&room_id) {
            Some(r) => r,
            None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
        };

        if !room.members.contains(&actor) {
            return matrix_error(StatusCode::FORBIDDEN, "M_FORBIDDEN", "Not a member of this room").into_response();
        }

        // Remove from active members.
        room.members.retain(|m| *m != body.user_id);

        let event_id = state.next_event_id();
        room.state_events.push(serde_json::json!({
            "type": "m.room.member",
            "state_key": body.user_id,
            "content": { "membership": "ban", "reason": body.reason },
            "sender": actor,
            "event_id": event_id,
        }));
    }

    // Store in banned_members for the membership=ban filter.
    state
        .banned_members
        .entry(room_id)
        .or_default()
        .push(crate::state::BannedEntry {
            user_id: body.user_id,
            reason: body.reason,
        });

    Json(serde_json::json!({})).into_response()
}

/// POST /_matrix/client/v3/rooms/:room_id/unban
pub async fn unban_member(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<UnbanRequest>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    // Remove from banned_members list.
    if let Some(mut banned) = state.banned_members.get_mut(&room_id) {
        banned.retain(|b| b.user_id != body.user_id);
    }

    // Update room state membership to leave.
    if let Some(mut room) = state.rooms.get_mut(&room_id) {
        let event_id = state.next_event_id();
        room.state_events.push(serde_json::json!({
            "type": "m.room.member",
            "state_key": body.user_id,
            "content": { "membership": "leave" },
            "sender": "@server:localhost",
            "event_id": event_id,
        }));
    }

    Json(serde_json::json!({})).into_response()
}

/// PUT /_matrix/client/v3/rooms/:room_id/redact/:event_id/:txn_id
pub async fn redact_event(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path((room_id, event_id, _txn_id)): Path<(String, String, String)>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    // Mark the event as redacted in the timeline.
    if let Some(mut timeline) = state.timelines.get_mut(&room_id) {
        for ev in timeline.iter_mut() {
            if ev.get("event_id").and_then(serde_json::Value::as_str) == Some(&event_id)
                && let Some(obj) = ev.as_object_mut()
            {
                obj.insert("redacted".to_string(), serde_json::json!(true));
                obj.insert("content".to_string(), serde_json::json!({}));
            }
        }
    }

    let redaction_id = state.next_event_id();
    Json(serde_json::json!({ "event_id": redaction_id })).into_response()
}

/// GET /_matrix/client/v3/rooms/:room_id/state/m.room.power_levels
pub async fn get_power_levels(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    if !state.rooms.contains_key(&room_id) {
        return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response();
    }

    match state.power_levels.get(&room_id) {
        Some(pl) => Json(pl.clone()).into_response(),
        // No explicit power_levels event → return 404 so the client uses spec defaults.
        None => matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "No power_levels state event").into_response(),
    }
}

#[derive(Deserialize)]
pub struct RoomNameBody {
    pub name: String,
}

#[derive(Deserialize)]
pub struct RoomTopicBody {
    pub topic: String,
}

/// PUT /_matrix/client/v3/rooms/:room_id/state/m.room.name
pub async fn set_room_name(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<RoomNameBody>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let mut room = match state.rooms.get_mut(&room_id) {
        Some(r) => r,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    room.name = body.name.clone();

    // Update the m.room.name state event.
    for ev in room.state_events.iter_mut() {
        if ev.get("type").and_then(serde_json::Value::as_str) == Some("m.room.name") {
            if let Some(content) = ev.get_mut("content").and_then(|c| c.as_object_mut()) {
                content.insert("name".to_string(), serde_json::json!(body.name));
            }
            let event_id = state.next_event_id();
            return Json(serde_json::json!({ "event_id": event_id })).into_response();
        }
    }

    // No existing event — push new one.
    let event_id = state.next_event_id();
    room.state_events.push(serde_json::json!({
        "type": "m.room.name",
        "state_key": "",
        "content": { "name": body.name },
        "event_id": event_id,
    }));
    Json(serde_json::json!({ "event_id": event_id })).into_response()
}

/// PUT /_matrix/client/v3/rooms/:room_id/state/m.room.topic
pub async fn set_room_topic(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<RoomTopicBody>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let mut room = match state.rooms.get_mut(&room_id) {
        Some(r) => r,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    room.topic = Some(body.topic.clone());

    // Update the m.room.topic state event if present.
    for ev in room.state_events.iter_mut() {
        if ev.get("type").and_then(serde_json::Value::as_str) == Some("m.room.topic") {
            if let Some(content) = ev.get_mut("content").and_then(|c| c.as_object_mut()) {
                content.insert("topic".to_string(), serde_json::json!(body.topic));
            }
            let event_id = state.next_event_id();
            return Json(serde_json::json!({ "event_id": event_id })).into_response();
        }
    }

    let event_id = state.next_event_id();
    room.state_events.push(serde_json::json!({
        "type": "m.room.topic",
        "state_key": "",
        "content": { "topic": body.topic },
        "event_id": event_id,
    }));
    Json(serde_json::json!({ "event_id": event_id })).into_response()
}

// ---------------------------------------------------------------------------
// Account data
// ---------------------------------------------------------------------------

/// GET /_matrix/client/v3/user/:user_id/account_data/:data_type
pub async fn get_account_data(
    State(state): State<std::sync::Arc<MatrixState>>,
    headers: HeaderMap,
    Path((user_id, data_type)): Path<(String, String)>,
) -> impl IntoResponse {
    let authed_user = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };

    // Users can only access their own account data
    if authed_user != user_id {
        return matrix_error(StatusCode::FORBIDDEN, "M_FORBIDDEN", "Cannot access other user's data").into_response();
    }

    let user_data = match state.account_data.get(&user_id) {
        Some(d) => d,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "No account data").into_response(),
    };

    match user_data.get(&data_type) {
        Some(value) => Json(value.clone()).into_response(),
        None => matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Account data type not found").into_response(),
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TestAuthTokenRequest {
    pub username: String,
}

/// POST /test/auth/token
///
/// Looks up a user by `username` (matched against displayname or the local part
/// of the user_id, e.g. "owl" matches "@owl:localhost").  Creates an auth token
/// for the resolved user_id and returns it.  Used by integration tests to obtain
/// a valid access token without going through a real login flow.
pub async fn test_auth_token(
    State(state): State<std::sync::Arc<MatrixState>>,
    Json(body): Json<TestAuthTokenRequest>,
) -> impl IntoResponse {
    // Try exact user_id match first (e.g. "@owl:localhost")
    if state.users.contains_key(&body.username) {
        let token = state.auth.create_token(&body.username);
        return Json(serde_json::json!({
            "user_id": body.username,
            "access_token": token,
        })).into_response();
    }

    // Search by displayname or local part of user_id
    let found = state.users.iter().find(|entry| {
        entry.displayname == body.username
            || entry.user_id.trim_start_matches('@')
                .split(':')
                .next()
                .unwrap_or("")
                == body.username
    }).map(|entry| (entry.user_id.clone(), entry.device_id.clone()));

    match found {
        Some((user_id, device_id)) => {
            let token = state.auth.create_token(&user_id);
            Json(serde_json::json!({
                "user_id": user_id,
                "device_id": device_id,
                "access_token": token,
            })).into_response()
        }
        None => matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "User not found").into_response(),
    }
}

// ---------------------------------------------------------------------------
/// GET /_matrix/media/v3/thumbnail/{server}/{mediaId}
/// (and the matching /download/ alias)
///
/// Serves bundled demo avatar bytes for a few well-known media IDs so the
/// chat avatars resolve in dev. Real homeservers proxy from their own media
/// store; this mock just maps ids → embedded bytes.
pub async fn media_thumbnail(
    Path((_server, media_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Strip "_avatar" suffix to get bare animal name, then delegate to shared helper.
    // Room avatar media IDs use "_avatar" suffix convention (e.g. "hollow_tree_avatar").
    // We keep those mapped here since they use compound names the shared helper doesn't know.
    static HOLLOW_TREE_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/hedgehog.svg");
    static NEON_REEF_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/parrot.svg");

    match media_id.as_str() {
        // Room avatars — compound names served inline
        "hollow_tree_avatar" => (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
                (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            HOLLOW_TREE_SVG,
        ).into_response(),
        "neon_reef_avatar" => (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
                (axum::http::header::CACHE_CONTROL, "public, max-age=3600"),
            ],
            NEON_REEF_SVG,
        ).into_response(),
        // User avatars — strip "_avatar" suffix and delegate to shared helper
        other => {
            let name = other.trim_end_matches("_avatar");
            poly_test_common::serve_animal(name)
        }
    }
}
