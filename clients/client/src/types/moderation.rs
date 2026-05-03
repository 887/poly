//! Moderation, content policy, permissions, roles, and blocked-user types.

use serde::{Deserialize, Serialize};

use super::user::User;

/// Sensitive content filter level for different DM/channel contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SensitiveContentLevel {
    /// Always show content without warning.
    Show,
    /// Always hide content behind a click-to-reveal overlay.
    #[default]
    Hide,
    /// Show a warning before revealing content.
    WarnFirst,
}

impl SensitiveContentLevel {
    /// Display label for this level.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Show => "Show",
            Self::Hide => "Hide",
            Self::WarnFirst => "Warn First",
        }
    }
}

/// DM spam filter aggressiveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DmSpamFilterLevel {
    /// Filter all unsolicited DMs.
    FilterAll,
    /// Filter DMs from users who are not friends (default).
    #[default]
    FilterNonFriends,
    /// Do not filter any DMs.
    DoNotFilter,
}

impl DmSpamFilterLevel {
    /// Display label for this level.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::FilterAll => "Filter all messages from non-friends",
            Self::FilterNonFriends => "Filter messages from non-friends",
            Self::DoNotFilter => "Do not filter",
        }
    }
}

/// Content and social policy settings for an account.
///
/// Controls what content is shown, who can send DMs, and friend request
/// permissions. These are stored per-account and come from the client backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentPolicy {
    /// Sensitive media filter for DMs from friends.
    pub sensitive_content_dm_friends: SensitiveContentLevel,
    /// Sensitive media filter for DMs from non-friends.
    pub sensitive_content_dm_others: SensitiveContentLevel,
    /// Sensitive media filter in server channels.
    pub sensitive_content_server_channels: SensitiveContentLevel,
    /// How aggressively to filter unsolicited DMs.
    pub dm_spam_filter: DmSpamFilterLevel,
    /// Whether age-restricted (NSFW) servers are accessible.
    pub allow_age_restricted_servers: bool,
    /// Whether age-restricted slash commands are accessible in DMs.
    pub allow_age_restricted_commands_in_dms: bool,
    /// Whether server members can initiate DMs without a prior relationship.
    pub allow_dms_from_server_members: bool,
    /// Whether message requests from unknown users are enabled.
    pub allow_message_requests: bool,
    /// Whether to accept friend requests from anyone.
    pub friend_request_from_everyone: bool,
    /// Whether to accept friend requests from friends-of-friends.
    pub friend_request_from_friends_of_friends: bool,
    /// Whether to accept friend requests from server members.
    pub friend_request_from_server_members: bool,
}

impl Default for ContentPolicy {
    fn default() -> Self {
        Self {
            sensitive_content_dm_friends: SensitiveContentLevel::Hide,
            sensitive_content_dm_others: SensitiveContentLevel::Hide,
            sensitive_content_server_channels: SensitiveContentLevel::Hide,
            dm_spam_filter: DmSpamFilterLevel::FilterNonFriends,
            allow_age_restricted_servers: false,
            allow_age_restricted_commands_in_dms: false,
            allow_dms_from_server_members: true,
            allow_message_requests: true,
            friend_request_from_everyone: true,
            friend_request_from_friends_of_friends: true,
            friend_request_from_server_members: true,
        }
    }
}

/// A user that the authenticated user has blocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedUser {
    /// Backend-specific user ID.
    pub user_id: String,
    /// Display name of the blocked user.
    pub display_name: String,
    /// Optional avatar URL.
    pub avatar_url: Option<String>,
}

/// The calling user's effective permissions in a server or channel.
///
/// Boolean flags — the host uses these to gate UI affordances without knowing
/// which backend-specific role system produced them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MemberPermissions {
    /// Can manage the server itself (rename, change settings, delete).
    pub manage_server: bool,
    /// Can manage channels (create, rename, delete, reorder).
    pub manage_channels: bool,
    /// Can manage roles (create, edit, assign).
    pub manage_roles: bool,
    /// Can kick members from the server.
    pub kick_members: bool,
    /// Can ban members from the server.
    pub ban_members: bool,
    /// Can delete or suppress messages by other users.
    pub manage_messages: bool,
    /// Can put members in timeout / mute.
    pub timeout_members: bool,
    /// The user's display role (highest role name, or "Owner", "Admin", "Member").
    pub display_role: String,
    /// Numeric power level for backends that use one (Matrix, custom). `None` for
    /// bitfield/enum backends that don't expose a numeric level.
    pub power_level: Option<i64>,
}

/// Backend-specific role assignment for `update_member_role`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberRole {
    /// Role represented by its backend-specific ID string (Discord role ID, poly-server role name).
    ById(String),
    /// Matrix power level integer.
    PowerLevel(i64),
}

/// A currently banned member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannedMember {
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub reason: Option<String>,
    /// RFC3339 timestamp when the ban expires; `None` = permanent.
    pub expires_at: Option<String>,
    /// RFC3339 timestamp when the ban was applied.
    pub banned_at: Option<String>,
}

/// A single entry in the server's moderation log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationLogEntry {
    pub id: String,
    pub action: ModerationAction,
    pub moderator: User,
    pub target_user_id: Option<String>,
    pub target_display_name: Option<String>,
    pub channel_id: Option<String>,
    pub message_id: Option<String>,
    pub reason: Option<String>,
    /// RFC3339 timestamp.
    pub timestamp: String,
}

/// What moderation action was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationAction {
    MemberKicked,
    MemberBanned,
    MemberUnbanned,
    MemberTimedOut,
    MemberRoleUpdated,
    MessageDeleted,
    ChannelUpdated,
    Other(String),
}

/// Read-only role descriptor (v1 — no editing, just display).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// Backend-specific role ID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional hex colour string (e.g. `"#5865F2"`).
    pub color: Option<String>,
    /// Permissions this role grants.
    pub permissions: MemberPermissions,
    /// Sort position (lower = lower in hierarchy).
    pub position: u32,
}
