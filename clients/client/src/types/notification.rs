//! Notification types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::backend::BackendType;

/// A notification from a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    /// Notification ID.
    pub id: String,
    /// Type of notification.
    pub kind: NotificationKind,
    /// Which backend sent this notification.
    pub backend: BackendType,
    /// The account ID that owns this notification.
    pub account_id: String,
    /// When the notification was created.
    pub timestamp: DateTime<Utc>,
    /// Whether the user has read this notification.
    pub read: bool,
    /// Preview text for the notification.
    pub preview: String,
}

/// The kind of notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Invited to join a voice channel.
    VoiceChannelInvite {
        /// Server the voice channel belongs to.
        server_id: String,
        /// Voice channel ID.
        channel_id: String,
        /// Human-readable name of the voice channel.
        channel_name: String,
        /// User ID of the person who sent the invite.
        inviter_user_id: String,
    },
    /// Stored auth token was rejected (401). User must sign in again for this
    /// account before it can be used. Carries the backend slug so the UI can
    /// route "Reconnect" clicks straight to the right signup flow.
    ReauthRequired { backend_slug: String },
    /// Generic notification.
    Other(String),
}
