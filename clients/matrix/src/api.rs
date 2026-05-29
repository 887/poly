//! Typed Matrix client-server API models used by the native client
//! implementation.
//!
//! These types are intentionally kept internal to `poly-matrix` so external
//! app crates stay isolated from Matrix-specific protocol details.
//!
//! Reference: https://spec.matrix.org/latest/client-server-api/

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Authentication (§5 Authentication)
// ---------------------------------------------------------------------------

/// Request body for `POST /_matrix/client/v3/login`.
#[derive(Debug, Serialize)]
pub struct LoginRequest {
    /// Login type, e.g. `m.login.password` or `m.login.token`.
    #[serde(rename = "type")]
    pub login_type: String,

    /// Login identifier (for password login).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<LoginIdentifier>,

    /// Password (for `m.login.password`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// Login token (for `m.login.token`, from SSO redirect).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// Device display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_device_display_name: Option<String>,
}

/// User identifier for login.
#[derive(Debug, Serialize)]
pub struct LoginIdentifier {
    /// Identifier type, e.g. `m.id.user`.
    #[serde(rename = "type")]
    pub id_type: String,

    /// The Matrix user ID or username.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// Response body for `POST /_matrix/client/v3/login`.
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    /// Fully-qualified user ID (e.g. `@alice:matrix.org`).
    pub user_id: String,

    /// Access token for subsequent requests.
    pub access_token: String,

    /// Device ID assigned by the homeserver.
    pub device_id: String,

}

/// Response from `GET /_matrix/client/v3/account/whoami`.
#[derive(Debug, Deserialize)]
pub struct WhoAmIResponse {
    pub user_id: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

// ---------------------------------------------------------------------------
// User profile (§10 User Data)
// ---------------------------------------------------------------------------

/// Response from `GET /_matrix/client/v3/profile/{userId}`.
#[derive(Debug, Deserialize)]
pub struct ProfileResponse {
    /// Display name.
    #[serde(default)]
    pub displayname: Option<String>,

    /// Avatar MXC URL.
    #[serde(default)]
    pub avatar_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Sync (§7 Syncing)
// ---------------------------------------------------------------------------

/// Response body for `GET /_matrix/client/v3/sync`.
#[derive(Debug, Deserialize)]
pub struct SyncResponse {
    /// Opaque token for the next sync request.
    pub next_batch: String,

    /// Room-related updates.
    #[serde(default)]
    pub rooms: Option<SyncRooms>,
}

/// Rooms section of a sync response.
#[derive(Debug, Deserialize)]
pub struct SyncRooms {
    /// Joined rooms and their updates.
    #[serde(default)]
    pub join: Option<std::collections::HashMap<String, JoinedRoom>>,
}

/// Updates for a single joined room in a sync response.
#[derive(Debug, Deserialize)]
pub struct JoinedRoom {
    /// Timeline events.
    #[serde(default)]
    pub timeline: Option<Timeline>,

    /// Ephemeral events (typing, receipts).
    #[serde(default)]
    pub ephemeral: Option<Ephemeral>,
}

/// Timeline section of a joined room.
#[derive(Debug, Deserialize)]
pub struct Timeline {
    /// List of timeline events.
    #[serde(default)]
    pub events: Vec<RoomEvent>,

    /// Pagination token for earlier events.
    #[serde(default)]
    pub prev_batch: Option<String>,
}

/// Ephemeral events section.
#[derive(Debug, Deserialize)]
pub struct Ephemeral {
    /// List of ephemeral events.
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Room events
// ---------------------------------------------------------------------------

/// A Matrix room event (state or timeline).
#[derive(Debug, Deserialize)]
pub struct RoomEvent {
    /// Event type, e.g. `m.room.message`, `m.room.name`.
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event ID.
    #[serde(default)]
    pub event_id: Option<String>,

    /// Sender user ID.
    #[serde(default)]
    pub sender: Option<String>,

    /// Server timestamp (ms since epoch).
    #[serde(default)]
    pub origin_server_ts: Option<u64>,

    /// State key (only for state events).
    #[serde(default)]
    pub state_key: Option<String>,

    /// Event content (type-dependent).
    #[serde(default)]
    pub content: serde_json::Value,

    /// For `m.room.redaction` events: the event ID being redacted.
    #[serde(default)]
    pub redacts: Option<String>,

    /// Unsigned data block — carries `prev_content` for state events, used
    /// when projecting `m.room.member` transitions into moderation entries.
    #[serde(default)]
    pub unsigned: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Messages (§11 Messaging)
// ---------------------------------------------------------------------------

/// Request body for `PUT /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}`.
#[derive(Debug, Serialize)]
pub struct SendMessageRequest {
    /// Message type, e.g. `m.text`, `m.image`.
    pub msgtype: String,

    /// Message body text.
    pub body: String,

    /// Formatted body (HTML).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatted_body: Option<String>,

    /// Format type when formatted_body is present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// For replies: the event being replied to.
    #[serde(rename = "m.relates_to", skip_serializing_if = "Option::is_none")]
    pub relates_to: Option<RelatesTo>,
}

/// Relationship metadata for replies and threads.
#[derive(Debug, Serialize, Deserialize)]
pub struct RelatesTo {
    /// In-reply-to reference.
    #[serde(rename = "m.in_reply_to", skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<InReplyTo>,
}

/// Reference to the event being replied to.
#[derive(Debug, Serialize, Deserialize)]
pub struct InReplyTo {
    /// Event ID of the parent message.
    pub event_id: String,
}

/// Response from sending a message event.
#[derive(Debug, Deserialize)]
pub struct SendEventResponse {
    /// Event ID assigned by the homeserver.
    pub event_id: String,
}

// ---------------------------------------------------------------------------
// Room directory & Spaces
// ---------------------------------------------------------------------------

/// Response from `GET /_matrix/client/v3/joined_rooms`.
#[derive(Debug, Deserialize)]
pub struct JoinedRoomsResponse {
    /// List of room IDs the user has joined.
    pub joined_rooms: Vec<String>,
}

/// Response from `GET /_matrix/client/v1/rooms/{roomId}/hierarchy`.
#[derive(Debug, Deserialize)]
pub struct SpaceHierarchyResponse {
    /// Rooms in the Space hierarchy.
    #[serde(default)]
    pub rooms: Vec<SpaceHierarchyRoom>,
}

/// A room entry in a Space hierarchy response.
#[derive(Debug, Deserialize)]
pub struct SpaceHierarchyRoom {
    /// Room ID.
    pub room_id: String,

    /// Room name.
    #[serde(default)]
    pub name: Option<String>,

    /// Room type (e.g. `m.space` for Spaces).
    #[serde(default)]
    pub room_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Room members
// ---------------------------------------------------------------------------

/// Response from `GET /_matrix/client/v3/rooms/{roomId}/members`.
#[derive(Debug, Deserialize)]
pub struct RoomMembersResponse {
    /// Member state events.
    #[serde(default)]
    pub chunk: Vec<RoomEvent>,
}

/// Paginated messages response from
/// `GET /_matrix/client/v3/rooms/{roomId}/messages`.
#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    /// Message events (most recent first when `dir=b`).
    #[serde(default)]
    pub chunk: Vec<RoomEvent>,
}

// ---------------------------------------------------------------------------
// Moderation (B-MX — plan-permissions-moderation.md §1.2)
// ---------------------------------------------------------------------------

/// Request body for `POST /_matrix/client/v3/rooms/{roomId}/kick`.
#[derive(Debug, Serialize)]
pub struct KickRequest {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request body for `POST /_matrix/client/v3/rooms/{roomId}/ban`.
#[derive(Debug, Serialize)]
pub struct BanRequest {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request body for `POST /_matrix/client/v3/rooms/{roomId}/unban`.
#[derive(Debug, Serialize)]
pub struct UnbanRequest {
    pub user_id: String,
}

/// Request body for `PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}`.
#[derive(Debug, Serialize)]
pub struct RedactRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request body for `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.name`.
#[derive(Debug, Serialize)]
pub struct RoomNameRequest {
    pub name: String,
}

/// Request body for `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.topic`.
#[derive(Debug, Serialize)]
pub struct RoomTopicRequest {
    pub topic: String,
}

/// The `m.room.power_levels` content — only the fields needed for `get_my_permissions`.
///
/// Subset of the full Matrix spec content; extra fields are deserialised-and-dropped
/// (serde ignores unknown fields by default). All fields use Matrix spec defaults when
/// absent: `ban=50`, `kick=50`, `redact=50`, `state_default=50`, `users_default=0`.
#[derive(Debug, Default, Deserialize)]
pub struct PowerLevelsContent {
    #[serde(default = "default_50")]
    pub ban: i64,
    #[serde(default = "default_50")]
    pub kick: i64,
    #[serde(default = "default_50")]
    pub redact: i64,
    #[serde(default = "default_50")]
    pub state_default: i64,
    #[serde(default)]
    pub users_default: i64,
    /// Per-user overrides: user_id → power level.
    #[serde(default)]
    pub users: std::collections::HashMap<String, i64>,
}

const fn default_50() -> i64 {
    50
}

impl PowerLevelsContent {
    /// Return the power level for the given user_id (falls back to `users_default`).
    pub fn user_level(&self, user_id: &str) -> i64 {
        self.users.get(user_id).copied().unwrap_or(self.users_default)
    }
}

// ---------------------------------------------------------------------------
// Ignored users (account data)
// ---------------------------------------------------------------------------

/// Content of the `m.ignored_user_list` account data event.
///
/// Spec: https://spec.matrix.org/v1.8/client-server-api/#mignored_user_list
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct IgnoredUserListContent {
    /// Map from user_id to an empty object `{}`.
    pub ignored_users: std::collections::HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Push rules (notifications / mute)
// ---------------------------------------------------------------------------

/// Request body for `PUT /_matrix/client/v3/pushrules/global/room/{roomId}`.
///
/// Spec: https://spec.matrix.org/v1.8/client-server-api/#put_matrixclientv3pushrulesscopekindruleid
#[derive(Debug, Serialize)]
pub struct PushRuleRequest {
    /// Actions — `["dont_notify"]` to mute.
    pub actions: Vec<serde_json::Value>,
    /// Conditions (empty for room-level rules).
    #[serde(default)]
    pub conditions: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Room invite
// ---------------------------------------------------------------------------

/// Request body for `POST /_matrix/client/v3/rooms/{roomId}/invite`.
#[derive(Debug, Serialize)]
pub struct InviteRequest {
    pub user_id: String,
}

// ---------------------------------------------------------------------------
// Room avatar
// ---------------------------------------------------------------------------

/// Request body for `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.avatar/`.
#[derive(Debug, Serialize)]
pub struct RoomAvatarRequest {
    pub url: String,
}

// ---------------------------------------------------------------------------
// Message search (C.2 — SOLID audit Phase C)
// ---------------------------------------------------------------------------

/// Room-events search category body.
///
/// Reference: https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3search
#[derive(Debug, Serialize)]
pub struct RoomEventsFilter {
    /// Free-text search string.
    pub search_term: String,

    /// Restrict results to a specific room ID (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<SearchFilter>,

    /// Maximum number of results to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Optional filter to narrow down search results.
#[derive(Debug, Serialize)]
pub struct SearchFilter {
    /// Room IDs to restrict the search to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rooms: Option<Vec<String>>,
}

/// Outer request body for `POST /_matrix/client/v3/search`.
#[derive(Debug, Serialize)]
pub struct SearchRequest {
    pub search_categories: SearchCategories,
}

/// The search categories container (only room_events is used).
#[derive(Debug, Serialize)]
pub struct SearchCategories {
    pub room_events: RoomEventsFilter,
}

/// A single result item from the search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchResult {
    /// The matching event.
    pub result: RoomEvent,
}

/// Results for the `room_events` category.
#[derive(Debug, Deserialize)]
pub struct RoomEventsSearchResult {
    /// The list of results.
    #[serde(default)]
    pub results: Vec<SearchResult>,
}

/// Outer response body for `POST /_matrix/client/v3/search`.
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub search_categories: SearchCategoriesResponse,
}

/// The search categories response container.
#[derive(Debug, Deserialize)]
pub struct SearchCategoriesResponse {
    pub room_events: RoomEventsSearchResult,
}

// ---------------------------------------------------------------------------
// Pinned events (C.3 — SOLID audit Phase C)
// ---------------------------------------------------------------------------

/// Content of the `m.room.pinned_events` state event.
///
/// Reference: https://spec.matrix.org/v1.11/client-server-api/#mroompinned_events
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PinnedEventsContent {
    /// Ordered list of pinned event IDs (oldest first).
    #[serde(default)]
    pub pinned: Vec<String>,
}

// ---------------------------------------------------------------------------
// Room creation (C.4 — SOLID audit Phase C)
// ---------------------------------------------------------------------------

/// Request body for `POST /_matrix/client/v3/createRoom`.
///
/// Reference: https://spec.matrix.org/v1.11/client-server-api/#post_matrixclientv3createroom
#[derive(Debug, Default, Serialize)]
pub struct CreateRoomRequest {
    /// Room preset governing default power levels and join rules.
    ///
    /// `"public_chat"` — open join, world-readable history.
    /// `"private_chat"` — invite-only.
    /// `"trusted_private_chat"` — invite-only, all invitees are admins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,

    /// Display name for the room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional machine-readable local alias (without `#` or server suffix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_alias_name: Option<String>,

    /// Room type: `None` for plain rooms, `"m.space"` for Spaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_type: Option<String>,

    /// Initial state events to send (e.g. setting the parent Space).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub initial_state: Vec<InitialStateEvent>,
}

/// A state event to include in the initial room state during `createRoom`.
#[derive(Debug, Serialize)]
pub struct InitialStateEvent {
    /// Event type, e.g. `"m.space.child"`.
    #[serde(rename = "type")]
    pub event_type: String,
    /// State key (empty string for most events).
    #[serde(default)]
    pub state_key: String,
    /// Event content.
    pub content: serde_json::Value,
}

/// Response body for `POST /_matrix/client/v3/createRoom`.
#[derive(Debug, Deserialize)]
pub struct CreateRoomResponse {
    /// The fully-qualified room ID of the newly created room.
    pub room_id: String,
}

// ---------------------------------------------------------------------------
// Presence (D.2 — SOLID audit Phase D)
// ---------------------------------------------------------------------------

/// Response body for `GET /_matrix/client/v3/presence/{userId}/status`.
///
/// Reference: https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3presenceuseridstatus
#[derive(Debug, Deserialize)]
// lint-allow-unused: protocol-level deserialization fields kept for future readers of presence body
#[allow(dead_code)]
pub struct PresenceStatusResponse {
    /// One of `online`, `unavailable`, `offline`.
    pub presence: String,

    /// Free-form status message.
    #[serde(default)]
    pub status_msg: Option<String>,

    /// Milliseconds since the user was last active (only when `currently_active` is false).
    #[serde(default)]
    pub last_active_ago: Option<u64>,

    /// Whether the user is currently active.
    #[serde(default)]
    pub currently_active: Option<bool>,
}

/// Request body for `PUT /_matrix/client/v3/presence/{userId}/status`.
#[derive(Debug, Serialize)]
pub struct PutPresenceRequest {
    /// One of `online`, `unavailable`, `offline`.
    pub presence: String,

    /// Optional free-form status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_msg: Option<String>,
}

// ---------------------------------------------------------------------------
// Member events (D.1 — moderation log synthesis)
// ---------------------------------------------------------------------------

/// Content of an `m.room.member` state event.
///
/// Reference: https://spec.matrix.org/v1.11/client-server-api/#mroommember
#[derive(Debug, Deserialize)]
// lint-allow-unused: protocol-level deserialization fields kept for future readers of member content
#[allow(dead_code)]
pub struct MemberEventContent {
    /// Membership transition target: `invite`, `join`, `leave`, `ban`, `knock`.
    pub membership: String,

    /// Optional display name as set in this membership event.
    #[serde(default)]
    pub displayname: Option<String>,

    /// Optional avatar MXC URL as set in this membership event.
    #[serde(default)]
    pub avatar_url: Option<String>,

    /// Optional reason for the transition (set on leave/ban/kick).
    #[serde(default)]
    pub reason: Option<String>,
}

/// Optional `unsigned` block for state events, used to recover the previous
/// membership when projecting m.room.member transitions.
#[derive(Debug, Deserialize, Default)]
pub struct UnsignedData {
    /// Previous content of this state event (key in unsigned block).
    #[serde(default)]
    pub prev_content: Option<MemberEventContent>,
}

/// Content of an `m.room.redaction` event (the redactor's reason).
#[derive(Debug, Deserialize, Default)]
pub struct RedactionEventContent {
    /// Optional reason for the redaction.
    #[serde(default)]
    pub reason: Option<String>,
}

