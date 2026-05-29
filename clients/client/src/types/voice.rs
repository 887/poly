//! Voice and video channel participant and connection types.

use serde::{Deserialize, Serialize};

use super::backend::BackendType;
use super::user::User;

/// A user connected to a voice or video channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Each bool tracks a distinct, independently togglable voice state; an enum
// or bitfield would not map cleanly to the WIT interface or backend events.
#[allow(clippy::struct_excessive_bools)]
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

/// What kind of live voice session the user is connected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceConnectionKind {
    /// A normal server voice/video channel.
    ServerChannel,
    /// A temporary direct/group call anchored to a DM rather than a server channel.
    TemporaryCall,
}

/// The local user's voice connection state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Voice-control toggles (mute, deafen, stream, video) are each individually
// controllable; an enum cannot represent all 2^4 combinations orthogonally.
#[allow(clippy::struct_excessive_bools)]
pub struct VoiceConnection {
    /// Channel ID we are connected to.
    pub channel_id: String,
    /// Server ID the channel belongs to.
    pub server_id: String,
    /// Display name of the connected channel.
    pub channel_name: String,
    /// Display name of the server.
    pub server_name: String,
    /// Which backend this voice connection belongs to (for routing).
    pub backend: BackendType,
    /// Account ID that owns this voice connection (for routing).
    pub account_id: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    pub instance_id: String,
    /// Whether our microphone is muted.
    pub is_muted: bool,
    /// Whether we are deafened (all audio muted).
    pub is_deafened: bool,
    /// Whether we are streaming our screen.
    pub is_streaming: bool,
    /// Whether our camera is on.
    pub is_video_on: bool,
    /// Whether this is a server voice channel or a temporary direct call.
    pub kind: VoiceConnectionKind,
    /// DM anchor for temporary direct calls.
    ///
    /// `Some(dm_id)` for temporary direct/group calls so UI affordances like the
    /// voice banner can jump back to the originating DM. `None` for server calls.
    pub dm_id: Option<String>,
    /// Remote participant user IDs for temporary calls.
    ///
    /// Server voice channels derive membership from the backend and leave this empty.
    pub participant_user_ids: Vec<String>,
}
