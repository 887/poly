//! Wire-format models for poly-server REST API.
//!
//! These mirror the server's JSON payloads. They are intentionally decoupled
//! from `poly-server` internals so this crate can be used independently.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Auth ─────────────────────────────────────────────────────────────────────

/// Response from `POST /auth/signup` and `POST /auth/verify`.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub device_id: String,
}

/// Response from `POST /auth/challenge`.
#[derive(Debug, Clone, Deserialize)]
pub struct ChallengeResponse {
    pub challenge: String,
    pub expires_at: String,
}

/// Response from `GET /server-info`.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub invite_only: bool,
}

// ── User ─────────────────────────────────────────────────────────────────────

/// User profile returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

// ── Server (guild) ───────────────────────────────────────────────────────────

/// A poly-server chat server / guild (wire format).
#[derive(Debug, Clone, Deserialize)]
pub struct WireServer {
    pub id: Option<String>,
    pub name: String,
    pub icon_url: Option<String>,
    pub owner: String,
    pub created_at: DateTime<Utc>,
}

/// Wrapper for membership query: `{ "user": { ... } }`.
///
/// SurrealDB's `SELECT user.* FROM membership FETCH user` returns each
/// member nested under a `"user"` key.
#[derive(Debug, Clone, Deserialize)]
pub struct MemberWrapper {
    pub user: UserProfile,
}

/// Raw server detail response from `GET /servers/:id`.
///
/// Members arrive as `[{ "user": { ...UserProfile } }]` due to the
/// SurrealDB membership query.
#[derive(Debug, Clone, Deserialize)]
struct RawServerDetail {
    pub server: WireServer,
    pub members: Vec<MemberWrapper>,
    pub channels: Vec<WireChannel>,
    pub categories: Vec<WireCategory>,
}

/// Server detail with flattened member profiles.
#[derive(Debug, Clone)]
pub struct ServerDetail {
    pub server: WireServer,
    pub members: Vec<UserProfile>,
    pub channels: Vec<WireChannel>,
    pub categories: Vec<WireCategory>,
}

impl ServerDetail {
    /// Deserialize from raw JSON, unwrapping member wrappers.
    pub fn from_value(val: serde_json::Value) -> serde_json::Result<Self> {
        let raw: RawServerDetail = serde_json::from_value(val)?;
        Ok(Self {
            server: raw.server,
            members: raw.members.into_iter().map(|w| w.user).collect(),
            channels: raw.channels,
            categories: raw.categories,
        })
    }
}

/// Wrapper for `SELECT server.* FROM membership FETCH server`.
///
/// Each entry is `{ "server": { ...WireServer } }`.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerWrapper {
    pub server: WireServer,
}

// ── Channel ──────────────────────────────────────────────────────────────────

/// Channel kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Text,
    Voice,
}

/// A channel (server channel or DM/group) — wire format.
///
/// Accepts both `ChannelResponse` (REST API) and raw DB `Channel` shapes
/// via `#[serde(alias)]`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireChannel {
    #[serde(default)]
    pub id: String,
    #[serde(default, alias = "server")]
    pub server_id: Option<String>,
    #[serde(default, alias = "category")]
    pub category_id: Option<String>,
    pub name: String,
    pub kind: ChannelKind,
    #[serde(default)]
    pub position: i64,
    /// Only present in raw DB responses; absent from `ChannelResponse`.
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

/// A channel category (groups channels inside a server).
#[derive(Debug, Clone, Deserialize)]
pub struct WireCategory {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub server: String,
    pub name: String,
    #[serde(default)]
    pub position: i64,
}

// ── Message ──────────────────────────────────────────────────────────────────

/// A chat message — wire format.
///
/// Field names match the server's `MessageResponse` (REST API).
/// Also accepts raw DB `Message` field names via `#[serde(alias)]`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireMessage {
    #[serde(default)]
    pub id: String,
    #[serde(alias = "channel")]
    pub channel_id: String,
    #[serde(alias = "author")]
    pub author_id: String,
    pub content: String,
    #[serde(default, alias = "reply_to")]
    pub reply_to_id: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub attachments: Vec<WireAttachmentRef>,
    pub created_at: DateTime<Utc>,
}

/// Slim attachment reference returned inside `MessageResponse`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireAttachmentRef {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

/// Reaction on a message.
#[derive(Debug, Clone, Deserialize)]
pub struct WireReaction {
    pub id: Option<String>,
    pub message: String,
    pub user: String,
    pub emoji: String,
}

// ── Participant ──────────────────────────────────────────────────────────────

/// Participant in a DM/group channel.
#[derive(Debug, Clone, Deserialize)]
pub struct Participant {
    pub id: Option<String>,
    pub user: String,
    pub channel: String,
    pub added_at: DateTime<Utc>,
}

// ── Device ───────────────────────────────────────────────────────────────────

/// A logged-in device session.
#[derive(Debug, Clone, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub owner: String,
    pub name: String,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub revoked: bool,
}

// ── Invite ───────────────────────────────────────────────────────────────────

/// Server invite code.
#[derive(Debug, Clone, Deserialize)]
pub struct Invite {
    pub id: Option<String>,
    pub code: String,
    pub server: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub uses: i64,
    pub max_uses: Option<i64>,
}

// ── Friend request ───────────────────────────────────────────────────────────

/// Friend request status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendRequestStatus {
    Pending,
    Accepted,
    Rejected,
}

/// A friend request.
#[derive(Debug, Clone, Deserialize)]
pub struct FriendRequest {
    pub id: Option<String>,
    pub from: String,
    pub to: String,
    pub status: FriendRequestStatus,
    pub created_at: DateTime<Utc>,
}

// ── Attachment ───────────────────────────────────────────────────────────────

/// An uploaded file attachment.
#[derive(Debug, Clone, Deserialize)]
pub struct WireAttachment {
    pub id: Option<String>,
    pub uploaded_by: String,
    pub message: Option<String>,
    pub filename: String,
    pub storage_name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

// ── WebSocket events ─────────────────────────────────────────────────────────

/// Events pushed from server → client over WebSocket.
///
/// Mirrors `poly_server::ws::ServerEvent` but defined independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum ServerEvent {
    /// New message in a channel.
    MessageCreated(MessagePayload),
    /// Message was edited.
    MessageEdited(MessagePayload),
    /// Message was soft-deleted.
    MessageDeleted {
        message_id: String,
        channel_id: String,
    },
    /// Reaction added.
    ReactionAdded {
        message_id: String,
        channel_id: String,
        user_id: String,
        emoji: String,
    },
    /// Reaction removed.
    ReactionRemoved {
        message_id: String,
        channel_id: String,
        user_id: String,
        emoji: String,
    },
    /// User started typing.
    TypingStart {
        channel_id: String,
        user: UserProfile,
    },
    /// User presence changed.
    PresenceUpdate { user_id: String, online: bool },
    /// This device's session was revoked.
    DeviceRevoked,
    /// Voice state change.
    VoiceStateUpdate {
        channel_id: String,
        user_id: String,
        joined: bool,
    },
    /// Incoming friend request.
    FriendRequestReceived {
        request_id: String,
        from: UserProfile,
    },
    /// Friend request accepted.
    FriendRequestAccepted {
        request_id: String,
        status: FriendRequestStatus,
    },
    /// User joined a server.
    ServerMemberJoined {
        server_id: String,
        user: UserProfile,
    },
    /// User left a server.
    ServerMemberLeft { server_id: String, user_id: String },
    /// Server metadata changed.
    ServerUpdated {
        server_id: String,
        name: String,
        icon_url: Option<String>,
    },
    /// Channel created.
    ChannelCreated {
        channel_id: String,
        server_id: Option<String>,
        name: String,
    },
    /// Channel deleted.
    ChannelDeleted {
        channel_id: String,
        server_id: Option<String>,
    },
    /// Keepalive ping.
    Ping,
    /// WebRTC signal relay.
    VoiceSignalRelay { from_user_id: String, sdp: String },
}

/// Wire representation of a message in events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted: bool,
    pub attachments: Vec<String>,
    pub created_at: DateTime<Utc>,
}
