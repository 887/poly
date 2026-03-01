//! Real-time event types from messenger backends.

use crate::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A real-time event from a messenger backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientEvent {
    /// A new message was received.
    MessageReceived {
        channel_id: String,
        message: Message,
    },

    /// An existing message was edited.
    MessageEdited {
        channel_id: String,
        message: Message,
    },

    /// A message was deleted.
    MessageDeleted {
        channel_id: String,
        message_id: String,
    },

    /// A user's presence status changed.
    PresenceChanged {
        user_id: String,
        status: PresenceStatus,
    },

    /// A notification was received.
    NotificationReceived(Notification),

    /// A user started typing in a channel.
    TypingStarted {
        channel_id: String,
        user_id: String,
        timestamp: DateTime<Utc>,
    },

    /// A channel was updated (name, topic, etc.).
    ChannelUpdated(Channel),

    /// A server was updated.
    ServerUpdated(Server),

    /// A friend request was received.
    FriendRequestReceived { from_user: User },

    /// Connection state changed.
    ConnectionStateChanged {
        backend: BackendType,
        connected: bool,
    },

    /// A user joined a voice channel.
    VoiceUserJoined {
        channel_id: String,
        participant: VoiceParticipant,
    },

    /// A user left a voice channel.
    VoiceUserLeft { channel_id: String, user_id: String },

    /// A voice participant's state changed (mute, deafen, stream, etc.).
    VoiceStateUpdated {
        channel_id: String,
        participant: VoiceParticipant,
    },
}
