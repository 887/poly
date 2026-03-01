//! Shared data types used across all messenger backends.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identifies which messenger backend a resource belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendType {
    /// Stoat (formerly Revolt) messenger.
    Stoat,
    /// Matrix protocol.
    Matrix,
    /// Discord.
    Discord,
    /// Microsoft Teams.
    Teams,
    /// Demo/mock client for UI testing.
    Demo,
}

impl BackendType {
    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Stoat => "Stoat",
            Self::Matrix => "Matrix",
            Self::Discord => "Discord",
            Self::Teams => "Teams",
            Self::Demo => "Demo",
        }
    }
}

/// Authentication credentials for logging in to a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthCredentials {
    /// Token-based authentication.
    Token(String),
    /// Email + password authentication.
    EmailPassword { email: String, password: String },
    /// OAuth2 flow (stores the resulting token).
    OAuth { token: String },
    /// Microsoft device code flow for Teams.
    DeviceCode { code: String },
}

/// An authenticated session with a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// The authenticated user.
    pub user: User,
    /// Session token for subsequent requests.
    pub token: String,
    /// Which backend this session is for.
    pub backend: BackendType,
}

/// A server/community/workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    /// Backend-specific server ID.
    pub id: String,
    /// Server display name.
    pub name: String,
    /// URL to the server icon/avatar.
    pub icon_url: Option<String>,
    /// Channel categories within this server.
    pub categories: Vec<Category>,
    /// Which backend this server belongs to.
    pub backend: BackendType,
    /// Total unread message count across all channels.
    pub unread_count: u32,
    /// Which account this server comes from (multi-account support).
    pub account_id: String,
    /// Display name of the account that owns this server.
    pub account_display_name: String,
}

/// A category/folder that groups channels within a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    /// Category ID.
    pub id: String,
    /// Category display name.
    pub name: String,
    /// Channel IDs in this category.
    pub channel_ids: Vec<String>,
}

/// The type of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelType {
    /// Text chat channel.
    Text,
    /// Voice channel.
    Voice,
    /// Video channel.
    Video,
}

/// A channel within a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Backend-specific channel ID.
    pub id: String,
    /// Channel display name.
    pub name: String,
    /// Type of channel (text, voice, video).
    pub channel_type: ChannelType,
    /// Server this channel belongs to.
    pub server_id: String,
    /// Number of unread messages.
    pub unread_count: u32,
    /// ID of the last message (for ordering).
    pub last_message_id: Option<String>,
}

/// Content that can be sent in a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageContent {
    /// Plain text message.
    Text(String),
    /// Message with text and attachments.
    WithAttachments {
        text: String,
        attachments: Vec<Attachment>,
    },
}

/// A file attachment in a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: String,
    /// Original filename.
    pub filename: String,
    /// MIME content type.
    pub content_type: String,
    /// URL to download the attachment.
    pub url: String,
    /// File size in bytes.
    pub size: u64,
}

/// A chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Backend-specific message ID.
    pub id: String,
    /// Author of the message.
    pub author: User,
    /// Message content.
    pub content: MessageContent,
    /// When the message was sent.
    pub timestamp: DateTime<Utc>,
    /// Attached files/images.
    pub attachments: Vec<Attachment>,
    /// Reactions on this message.
    pub reactions: Vec<Reaction>,
    /// Whether the message has been edited.
    pub edited: bool,
}

/// A reaction on a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    /// Emoji or custom reaction identifier.
    pub emoji: String,
    /// Number of users who reacted with this.
    pub count: u32,
    /// Whether the authenticated user has reacted.
    pub me: bool,
}

/// Query options for fetching messages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageQuery {
    /// Fetch messages before this message ID.
    pub before: Option<String>,
    /// Fetch messages after this message ID.
    pub after: Option<String>,
    /// Maximum number of messages to return.
    pub limit: Option<u32>,
}

/// A user on a messaging platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Backend-specific user ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// URL to the user's avatar.
    pub avatar_url: Option<String>,
    /// Current online presence.
    pub presence: PresenceStatus,
    /// Which backend this user is from.
    pub backend: BackendType,
}

/// Online presence status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceStatus {
    /// User is online and active.
    Online,
    /// User is idle/away.
    Idle,
    /// User is set to do not disturb.
    DoNotDisturb,
    /// User is invisible (appears offline).
    Invisible,
    /// User is offline.
    Offline,
}

/// A group chat (multi-user DM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Group ID.
    pub id: String,
    /// Group members.
    pub members: Vec<User>,
    /// Optional group name.
    pub name: Option<String>,
    /// Last message in the group.
    pub last_message: Option<Message>,
    /// Which backend this group is from.
    pub backend: BackendType,
    /// Which account this group comes from (multi-account support).
    pub account_id: String,
}

/// A direct message channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmChannel {
    /// DM channel ID.
    pub id: String,
    /// The other user in the DM.
    pub user: User,
    /// Last message in the DM.
    pub last_message: Option<Message>,
    /// Number of unread messages.
    pub unread_count: u32,
    /// Which backend this DM is from.
    pub backend: BackendType,
    /// Which account this DM comes from (multi-account support).
    pub account_id: String,
}

/// A notification from a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Notification ID.
    pub id: String,
    /// Type of notification.
    pub kind: NotificationKind,
    /// Which backend sent this notification.
    pub backend: BackendType,
    /// When the notification was created.
    pub timestamp: DateTime<Utc>,
    /// Whether the user has read this notification.
    pub read: bool,
    /// Preview text for the notification.
    pub preview: String,
}

/// The kind of notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationKind {
    /// New message mention.
    Mention {
        channel_id: String,
        message_id: String,
    },
    /// Friend request received.
    FriendRequest { from_user_id: String },
    /// Invited to a server.
    ServerInvite { server_id: String },
    /// Generic notification.
    Other(String),
}

/// Identifies a configured account (backend + credentials).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Unique account ID (local, generated by Poly — opaque string, typically UUID v4 format).
    pub id: String,
    /// Which backend this account connects to.
    pub backend: BackendType,
    /// Display name for this account.
    pub display_name: String,
    /// Whether this account is currently connected.
    pub connected: bool,
}

/// A user connected to a voice or video channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceParticipant {
    /// The user in the voice channel.
    pub user: User,
    /// Whether the user has muted their microphone.
    pub is_muted: bool,
    /// Whether the user has deafened (muted all audio).
    pub is_deafened: bool,
    /// Whether the user is sharing their screen.
    pub is_streaming: bool,
    /// Whether the user has their camera on.
    pub is_video_on: bool,
    /// Whether the user is currently speaking (activity indicator).
    pub is_speaking: bool,
}

/// The local user's voice connection state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceConnection {
    /// Channel ID we are connected to.
    pub channel_id: String,
    /// Server ID the channel belongs to.
    pub server_id: String,
    /// Display name of the connected channel.
    pub channel_name: String,
    /// Display name of the server.
    pub server_name: String,
    /// Whether our microphone is muted.
    pub is_muted: bool,
    /// Whether we are deafened (all audio muted).
    pub is_deafened: bool,
    /// Whether we are streaming our screen.
    pub is_streaming: bool,
    /// Whether our camera is on.
    pub is_video_on: bool,
}
