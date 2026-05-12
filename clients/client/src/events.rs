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

    /// D19 — the plugin's sidebar declaration has changed; the host
    /// should re-fetch via
    /// [`ClientBackend::get_sidebar_declaration`](crate::ClientBackend::get_sidebar_declaration).
    SidebarInvalidated,

    /// Phase D.3 — an incoming DM call is ringing for the local user.
    ///
    /// Emitted from the Discord gateway on `CALL_CREATE` when the local user's
    /// ID appears in the `ringing` list. Stoat Phase H will emit the same event.
    /// UI consumer routes to `DmIncomingCall` route showing accept / decline.
    IncomingCall {
        /// DM or group channel where the call originated.
        dm_id: String,
        /// User ID of the person placing the call.
        caller_user_id: String,
        /// Whether the call includes a video stream.
        with_video: bool,
    },
}
