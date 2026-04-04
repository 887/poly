//! Matrix Client-Server API route handlers.
//!
//! Implements the subset of the Matrix CS API that poly-matrix calls.
//! All handlers take `State<MatrixState>` and return JSON responses.

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
    State(state): State<MatrixState>,
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
    State(state): State<MatrixState>,
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
    State(state): State<MatrixState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match bearer_user(&state, &headers) {
        Ok(uid) => uid,
        Err(e) => return e.into_response(),
    };
    Json(serde_json::json!({
        "user_id": user_id,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Profile
// ---------------------------------------------------------------------------

/// GET /_matrix/client/v3/profile/:user_id
pub async fn get_profile(
    State(state): State<MatrixState>,
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
    State(state): State<MatrixState>,
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
    State(state): State<MatrixState>,
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
pub async fn room_members(
    State(state): State<MatrixState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    if bearer_user(&state, &headers).is_err() {
        return matrix_error(StatusCode::UNAUTHORIZED, "M_UNKNOWN_TOKEN", "Invalid token").into_response();
    }

    let room = match state.rooms.get(&room_id) {
        Some(r) => r,
        None => return matrix_error(StatusCode::NOT_FOUND, "M_NOT_FOUND", "Room not found").into_response(),
    };

    let member_events: Vec<serde_json::Value> = room
        .state_events
        .iter()
        .filter(|ev| ev.get("type").and_then(|t| t.as_str()) == Some("m.room.member"))
        .cloned()
        .collect();

    Json(serde_json::json!({ "chunk": member_events })).into_response()
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
    State(state): State<MatrixState>,
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
        let end = (from_idx + limit).min(timeline.len());
        timeline.get(from_idx..end).unwrap_or_default().to_vec()
    };

    let end_token = if dir == "b" {
        from_idx.saturating_sub(limit).to_string()
    } else {
        (from_idx + chunk.len()).to_string()
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
    State(state): State<MatrixState>,
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
    State(state): State<MatrixState>,
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
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);

        // Wait for an event or timeout
        let got_event = tokio::select! {
            result = rx.recv() => result.is_ok(),
            _ = tokio::time::sleep_until(deadline) => false,
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

        let timeline_events: Vec<serde_json::Value> = state
            .timelines
            .get(room_id)
            .map(|t| {
                // For initial sync (since=0), return all events
                // For incremental sync, return events after `since` index
                if since == 0 {
                    t.clone()
                } else {
                    // since token is the event counter at that point; use it as offset
                    // For simplicity, return events from index `since` onwards (capped)
                    let start = (since as usize).min(t.len());
                    t.get(start..).unwrap_or_default().to_vec()
                }
            })
            .unwrap_or_default();

        // Only include state for initial sync
        let state_events: Vec<serde_json::Value> = if since == 0 {
            room.state_events.clone()
        } else {
            vec![]
        };

        join.insert(
            room_id.clone(),
            serde_json::json!({
                "timeline": {
                    "events": timeline_events,
                    "prev_batch": since.to_string(),
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
    State(state): State<MatrixState>,
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
        .filter_map(|ev| ev.get("state_key").and_then(|k| k.as_str()).map(|s| s.to_string()))
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
    State(state): State<MatrixState>,
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
    State(state): State<MatrixState>,
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
// Account data
// ---------------------------------------------------------------------------

/// GET /_matrix/client/v3/user/:user_id/account_data/:data_type
pub async fn get_account_data(
    State(state): State<MatrixState>,
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
// Lifecycle endpoints
// ---------------------------------------------------------------------------

/// POST /seed
pub async fn seed(State(state): State<MatrixState>) -> impl IntoResponse {
    state.seed();
    Json(serde_json::json!({ "status": "seeded" }))
}

/// POST /reset
pub async fn reset(State(state): State<MatrixState>) -> impl IntoResponse {
    state.reset();
    Json(serde_json::json!({ "status": "reset" }))
}

/// POST /reseed
pub async fn reseed(State(state): State<MatrixState>) -> impl IntoResponse {
    state.reseed();
    Json(serde_json::json!({ "status": "reseeded" }))
}
