//! Discord API v10 wire types.
//!
//! These are deserialize-friendly wrappers over the subset of fields we read
//! from Discord / Spacebar responses. Typed fields (`Id<>`, `ChannelType`)
//! come from `twilight-model` (ISC) — no AGPL code from Spacebar/Fosscord.
//!
//! We don't use `twilight_model::user::User`, `::guild::Guild`, etc. directly
//! because those require ~30–45 fields per struct (the full Discord
//! representation), which is impractical for Spacebar compatibility and for
//! our mock server. Our wrapper uses `#[serde(default)]` on optional fields
//! so Spacebar can omit them freely.

use serde::{Deserialize, Serialize};
use twilight_model::channel::ChannelType;
use twilight_model::id::marker::{ChannelMarker, EmojiMarker, GuildMarker, MessageMarker, UserMarker};
use twilight_model::id::Id;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordUser {
    pub id: Id<UserMarker>,
    pub username: String,
    #[serde(default)]
    pub discriminator: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub global_name: Option<String>,
    /// Nitro subscription tier (E.1 — Phase E).
    ///
    /// | Value | Tier          |
    /// |-------|---------------|
    /// | 0     | None          |
    /// | 1     | Nitro Classic |
    /// | 2     | Nitro         |
    /// | 3     | Nitro Basic   |
    ///
    /// Absent when the field is omitted by the server (treated as 0 / None).
    /// Source: discord-api-types v10 `UserPremiumType`.
    #[serde(default)]
    pub premium_type: Option<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordGuild {
    pub id: Id<GuildMarker>,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    /// Guild banner hash. Present when the guild has the `BANNER` feature
    /// (Boost Tier 2). Full URL: `https://cdn.discordapp.com/banners/{id}/{hash}.png`
    #[serde(default)]
    pub banner: Option<String>,
    /// Discord's designated system / welcome channel for the guild.
    /// Maps to `system_channel_id` in the Discord API (snake_case on the wire).
    #[serde(default)]
    pub system_channel_id: Option<String>,
    /// The user ID of the guild owner.
    #[serde(default)]
    pub owner_id: Option<String>,
    /// Approximate member count, present when `with_counts=true` is passed to
    /// `GET /guilds/{id}`. Absent from the list endpoint and from test servers
    /// that don't include it.
    #[serde(default)]
    pub approximate_member_count: Option<u32>,
}

/// A tag available in a Discord forum channel.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordForumTag {
    pub id: Id<ChannelMarker>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub moderated: bool,
    /// Custom emoji ID (snowflake). One of `emoji_id` or `emoji_name` will be set.
    #[serde(default)]
    pub emoji_id: Option<Id<EmojiMarker>>,
    /// Unicode emoji name (e.g. `"😀"`).
    #[serde(default)]
    pub emoji_name: Option<String>,
}

/// Thread metadata embedded in a Discord thread channel object.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordThreadMetadata {
    pub archived: bool,
    /// Minutes before auto-archive: 60, 1440, 4320, or 10080.
    #[serde(default)]
    pub auto_archive_duration: u32,
    /// ISO 8601 timestamp of when the thread was archived (absent when not archived).
    #[serde(default)]
    pub archive_timestamp: Option<String>,
    #[serde(default)]
    pub locked: bool,
    /// ISO 8601 timestamp of when the thread was created.
    /// Only present in threads created after 2022-01-09.
    #[serde(default)]
    pub create_timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordChannel {
    pub id: Id<ChannelMarker>,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    #[serde(default)]
    pub guild_id: Option<Id<GuildMarker>>,
    #[serde(default)]
    pub parent_id: Option<Id<ChannelMarker>>,
    #[serde(default)]
    pub topic: Option<String>,
    /// Forum channels: available tags. Absent for non-forum channels.
    #[serde(default)]
    pub available_tags: Option<Vec<DiscordForumTag>>,
    /// Forum channels: default sort order (0 = LatestActivity, 1 = CreationDate).
    #[serde(default)]
    pub default_sort_order: Option<u8>,
    /// Thread channels: thread metadata. Absent for non-thread channels.
    #[serde(default)]
    pub thread_metadata: Option<DiscordThreadMetadata>,
    /// Forum/text-thread channels: tag IDs applied to this post thread.
    #[serde(default)]
    pub applied_tags: Option<Vec<Id<ChannelMarker>>>,
    /// Thread channels: number of messages in the thread.
    ///
    /// Note: `total_message_sent` includes deleted messages; `message_count` does not.
    /// We use `message_count` for display (mirrors Discord's own client).
    #[serde(default)]
    pub message_count: Option<u32>,
    /// Thread channels: number of members who joined the thread.
    #[serde(default)]
    pub member_count: Option<u32>,
    /// Thread channels: ID of the message that started this thread.
    /// Absent for forum posts created as standalone threads.
    #[serde(default)]
    pub owner_id: Option<Id<UserMarker>>,
    /// Slow-mode: seconds a user must wait between messages (0 = disabled).
    /// Used by `SlowModeGuard` (Phase D.5).
    #[serde(default)]
    pub rate_limit_per_user: Option<u32>,
}

/// Response shape from `GET /guilds/{id}/threads/active`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordActiveThreadsResponse {
    pub threads: Vec<DiscordChannel>,
    #[serde(default)]
    pub has_more: bool,
}

/// Response shape from `GET /channels/{id}/threads/archived/public`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordArchivedThreadsResponse {
    pub threads: Vec<DiscordChannel>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordMessage {
    pub id: Id<MessageMarker>,
    pub content: String,
    pub author: DiscordUser,
    pub channel_id: Id<ChannelMarker>,
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub referenced_message: Option<Box<DiscordMessage>>,
    /// If this message spawned a thread, the thread channel object is embedded here.
    #[serde(default)]
    pub thread: Option<DiscordChannel>,
}

// ── Moderation wire types (B-DS) ──────────────────────────────────────────────

/// A single role in a Discord guild.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordRole {
    pub id: Id<twilight_model::id::marker::RoleMarker>,
    pub name: String,
    /// Bitfield of permissions as a string-serialised i64.
    pub permissions: String,
    #[serde(default)]
    pub position: u32,
    /// RGB colour (0 = no colour set).
    #[serde(default)]
    pub color: u32,
}

/// The guild member object for the authenticated user (`GET /guilds/{id}/members/@me`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordGuildMember {
    /// Role IDs assigned to this member.
    #[serde(default)]
    pub roles: Vec<Id<twilight_model::id::marker::RoleMarker>>,
    /// ISO8601 timestamp — `Some` means the member is in timeout until this time.
    #[serde(default)]
    pub communication_disabled_until: Option<String>,
}

/// A single entry from `GET /users/@me/relationships`.
///
/// `type` codes (Discord):
///   1 = friend, 2 = block, 3 = incoming request, 4 = outgoing request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordRelationship {
    pub id: String,
    #[serde(rename = "type")]
    pub relationship_type: u8,
    pub user: DiscordUser,
}

/// A banned member entry from `GET /guilds/{id}/bans`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordBan {
    #[serde(default)]
    pub reason: Option<String>,
    pub user: DiscordUser,
}

/// A single Discord audit-log entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordAuditLogEntry {
    pub id: Id<twilight_model::id::marker::GenericMarker>,
    /// Numeric action type (20=kick, 22=ban, 23=unban, 12=channel_update, 72=msg_delete).
    pub action_type: u32,
    /// Moderator who performed the action.
    #[serde(default)]
    pub user_id: Option<Id<UserMarker>>,
    /// Target (user, channel, message) that was acted upon.
    #[serde(default)]
    pub target_id: Option<String>,
    /// Optional reason the moderator provided.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Response from `GET /guilds/{id}/audit-logs`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordAuditLogResponse {
    pub audit_log_entries: Vec<DiscordAuditLogEntry>,
    #[serde(default)]
    pub users: Vec<DiscordUser>,
}

// `DiscordChannelUpdate` removed — `PATCH /channels/{id}` returns a full
// `DiscordChannel` object; callers use `DiscordChannel` directly.
