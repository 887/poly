use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── User ─────────────────────────────────────────────────────────────────────

/// A registered user account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Option<String>,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Public user profile (no sensitive fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Database record — includes password hash (never returned to clients).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: Option<String>,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

// ── Device ───────────────────────────────────────────────────────────────────

/// A logged-in device session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    /// RecordId serialised as `"user:xxxx"`.
    pub owner: String,
    pub name: String,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub revoked: bool,
}

// ── Server (guild) ───────────────────────────────────────────────────────────

/// A chat server (guild/community).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: Option<String>,
    pub name: String,
    pub icon_url: Option<String>,
    /// RecordId serialised as `"user:xxxx"`.
    pub owner: String,
    pub created_at: DateTime<Utc>,
}

/// User membership in a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub id: Option<String>,
    pub user: String,
    pub server: String,
    pub joined_at: DateTime<Utc>,
}

/// Channel category (groups channels inside a server).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: Option<String>,
    pub server: String,
    pub name: String,
    pub position: i64,
}

// ── Channel ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// Text-based messaging channel.
    Text,
    /// Voice / video channel (signalling only — audio/video is WebRTC P2P).
    Voice,
}

/// A channel — may belong to a server or be a DM/group channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: Option<String>,
    /// `None` for DMs and group chats.
    pub server: Option<String>,
    pub category: Option<String>,
    pub name: String,
    pub kind: ChannelKind,
    pub position: i64,
    pub created_at: DateTime<Utc>,
}

/// Participation record for DM/group channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub id: Option<String>,
    pub user: String,
    pub channel: String,
    pub added_at: DateTime<Utc>,
}

// ── Message ──────────────────────────────────────────────────────────────────

/// A chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Option<String>,
    pub channel: String,
    pub author: String,
    pub content: String,
    /// Populated when this is a reply to another message.
    pub reply_to: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
    /// Soft-delete flag — content replaced with `[deleted]` on the wire.
    pub deleted: bool,
    pub created_at: DateTime<Utc>,
}

/// Emoji reaction on a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub id: Option<String>,
    pub message: String,
    pub user: String,
    pub emoji: String,
}

// ── Friend requests ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendRequestStatus {
    Pending,
    Accepted,
    Rejected,
}

/// Friend request between two users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequest {
    pub id: Option<String>,
    pub from: String,
    pub to: String,
    pub status: FriendRequestStatus,
    pub created_at: DateTime<Utc>,
}

// ── Voice sessions ───────────────────────────────────────────────────────────

/// Ephemeral record — who is currently in a voice channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSession {
    pub id: Option<String>,
    pub user: String,
    pub channel: String,
    pub joined_at: DateTime<Utc>,
}

// ── Attachments ──────────────────────────────────────────────────────────────

/// A file uploaded by a user. Linked to a message after sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: Option<String>,
    pub uploaded_by: String,
    /// Populated once the attachment is linked to a sent message.
    pub message: Option<String>,
    /// Original filename provided by the client.
    pub filename: String,
    /// UUID-based name used for on-disk storage (safe, no path traversal).
    pub storage_name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

// ── Invite codes ─────────────────────────────────────────────────────────────

/// Server invite link.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
