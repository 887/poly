use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{FriendRequestStatus, UserProfile};

/// All event types pushed from server → client over WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum ServerEvent {
    /// A new message was posted in a channel the user can see.
    MessageCreated(MessagePayload),
    /// A message was edited.
    MessageEdited(MessagePayload),
    /// A message was soft-deleted.
    MessageDeleted {
        message_id: String,
        channel_id: String,
    },
    /// A reaction was added to a message.
    ReactionAdded {
        message_id: String,
        channel_id: String,
        user_id: String,
        emoji: String,
    },
    /// A reaction was removed.
    ReactionRemoved {
        message_id: String,
        channel_id: String,
        user_id: String,
        emoji: String,
    },
    /// A user started typing (3 s TTL — client shows "X is typing…").
    TypingStart {
        channel_id: String,
        user: UserProfile,
    },
    /// A user's presence changed.
    PresenceUpdate { user_id: String, online: bool },
    /// This device's session was revoked by the owner. Client must sign out.
    DeviceRevoked,
    /// A user joined or left a voice channel.
    VoiceStateUpdate {
        channel_id: String,
        user_id: String,
        joined: bool,
    },
    /// Someone sent this user a friend request.
    FriendRequestReceived {
        request_id: String,
        from: UserProfile,
    },
    /// A friend request was accepted.
    FriendRequestAccepted {
        request_id: String,
        status: FriendRequestStatus,
    },
    /// A user joined a server the current user is in.
    ServerMemberJoined {
        server_id: String,
        user: UserProfile,
    },
    /// A user left/was kicked from a server.
    ServerMemberLeft { server_id: String, user_id: String },
    /// A server's metadata changed.
    ServerUpdated {
        server_id: String,
        name: String,
        icon_url: Option<String>,
    },
    /// A channel was created in a server.
    ChannelCreated {
        channel_id: String,
        server_id: Option<String>,
        name: String,
    },
    /// A channel was deleted.
    ChannelDeleted {
        channel_id: String,
        server_id: Option<String>,
    },
    /// Server ping for keepalive.
    Ping,
    /// Relay a WebRTC SDP/ICE signal from one peer to another.
    ///
    /// The server simply forwards this to the target user — no interpretation
    /// of the SDP/ICE content is performed.
    VoiceSignalRelay {
        /// The user who sent the signal.
        from_user_id: String,
        /// Raw SDP or ICE candidate JSON string.
        sdp: String,
    },
}

/// Wire representation of a message in events.
///
/// All ID fields are plain strings (SurrealDB RecordIds serialise as `"table:id"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    pub id: String,
    pub channel_id: String,
    /// ID of the author (`"user:xxxx"`).
    pub author_id: String,
    pub content: String,
    /// ID of the message being replied to, if any.
    pub reply_to_id: Option<String>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted: bool,
    /// IDs of attached files (`"attachment:xxxx"`).
    pub attachments: Vec<String>,
    pub created_at: DateTime<Utc>,
}
