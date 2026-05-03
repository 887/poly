//! User, presence, group, and DM channel types.

use serde::{Deserialize, Serialize};

use super::backend::BackendType;

/// A user on a messaging platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
///
/// Note: `last_message` field is in `message.rs` due to module ordering.
/// Group lives here alongside User since it's primarily a user-relations type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// Group ID.
    pub id: String,
    /// Group members.
    pub members: Vec<User>,
    /// Optional group name.
    pub name: Option<String>,
    /// Last message in the group.
    pub last_message: Option<super::message::Message>,
    /// Which backend this group is from.
    pub backend: BackendType,
    /// Which account this group comes from (multi-account support).
    pub account_id: String,
}

/// A direct message channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmChannel {
    /// DM channel ID.
    pub id: String,
    /// The other user in the DM.
    pub user: User,
    /// Last message in the DM.
    pub last_message: Option<super::message::Message>,
    /// Number of unread messages.
    pub unread_count: u32,
    /// Which backend this DM is from.
    pub backend: BackendType,
    /// Which account this DM comes from (multi-account support).
    pub account_id: String,
}
