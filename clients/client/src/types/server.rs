//! Server, category, channel, and forum channel types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::backend::BackendType;

/// A server/community/workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    /// Backend-specific server ID.
    pub id: String,
    /// Server display name.
    pub name: String,
    /// URL to the server icon/avatar.
    pub icon_url: Option<String>,
    /// Optional URL for a server banner image displayed at the top of the
    /// channel list sidebar. Wide-format image (e.g. 960×360) recommended.
    /// Sourced via [`crate::ClientBackend::get_server`]; `None` falls back to a
    /// gradient derived from the server's color.
    #[serde(default)]
    pub banner_url: Option<String>,
    /// Channel categories within this server.
    pub categories: Vec<Category>,
    /// Which backend this server belongs to.
    pub backend: BackendType,
    /// Total unread message count across all channels.
    pub unread_count: u32,
    /// Total @mention count across all channels in this server.
    ///
    /// Only increments when the current user is directly @mentioned
    /// (by @username, @here, @everyone, or a group they belong to),
    /// distinct from [`unread_count`] which counts all unread messages.
    #[serde(default)]
    pub mention_count: u32,
    /// Which account this server comes from (multi-account support).
    pub account_id: String,
    /// Display name of the account that owns this server.
    pub account_display_name: String,
    /// Backend-designated welcome / default channel (Discord
    /// `system_channel_id`; equivalents on other backends if any).
    /// When set, the host prefers this id when the user navigates to a
    /// stale or absent channel id rather than falling back to the first
    /// text channel. Always `None` for backends without the concept.
    #[serde(default)]
    pub default_channel_id: Option<String>,
    /// Optional short description for the server/repository/space.
    /// Used by forum and repo backends (GitHub, Forgejo, Lemmy).
    /// Always `None` for chat-only backends (Discord, Teams, Matrix, Stoat).
    #[serde(default)]
    pub description: Option<String>,
    /// Star/fave count, used by repo backends (GitHub, Forgejo, HN).
    /// Always `None` for chat backends.
    #[serde(default)]
    pub star_count: Option<u64>,
    /// Primary programming language, used by repo backends (GitHub, Forgejo).
    /// Always `None` for non-repo backends.
    #[serde(default)]
    pub language: Option<String>,
    /// Fork count, used by repo backends (GitHub, Forgejo).
    /// Always `None` for non-repo backends.
    #[serde(default)]
    pub forks_count: Option<u64>,
    /// Open issues + PRs count, used by repo backends (GitHub, Forgejo).
    /// Maps to "open issues in this repo" — Poly treats issues as channels,
    /// so this number is informative on the repo card.
    /// Always `None` for non-repo backends.
    #[serde(default)]
    pub open_issues_count: Option<u64>,
}

/// A category/folder that groups channels within a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Forum channel (Lemmy/Reddit-style: posts with threaded comments).
    ///
    /// Each post is a top-level message; replies form a thread.
    /// Used by Lemmy, Reddit, and Discord Forums (GUILD_FORUM type 15,
    /// GUILD_MEDIA type 16).
    Forum,
    /// Hacker News–style feed channel (title + URL + score + comment count).
    ///
    /// Rendered with HN-specific UI: Discord-style channel list sidebar,
    /// client-side text filter instead of Lemmy sort dropdown, infinite scroll.
    HackerNews,
    /// Code repository explorer (file tree + file content view).
    ///
    /// Rendered as a two-pane explorer instead of a message log.
    /// Used by GitHub / GitHub Enterprise repo channels.
    Code,
    /// A thread within a text or forum channel.
    ///
    /// Covers Discord PUBLIC_THREAD (11), PRIVATE_THREAD (12), and
    /// ANNOUNCEMENT_THREAD (10). Treated as text-like for message fetch.
    Thread,
    /// Announcement / news channel (Discord GUILD_ANNOUNCEMENT, type 5).
    ///
    /// Treated as text-like for message fetch.
    Announcement,
}

/// A tag available in a forum channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumTag {
    /// Backend-specific tag ID.
    pub id: String,
    /// Display name of the tag.
    pub name: String,
    /// Unicode emoji or custom emoji ID for the tag.
    pub emoji: Option<String>,
    /// When `true`, only moderators can apply this tag.
    pub moderated: bool,
}

/// Lightweight thread info carried on a message or as a thread channel summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadInfo {
    /// Backend-specific thread channel ID.
    pub thread_id: String,
    /// ID of the parent text or forum channel.
    pub parent_channel_id: String,
    /// Number of messages in the thread.
    pub message_count: u32,
    /// Number of members who have joined the thread.
    pub member_count: u32,
}

/// Metadata for a thread channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadMetadata {
    /// Whether the thread has been archived.
    pub archived: bool,
    /// Number of minutes of inactivity before the thread auto-archives.
    pub auto_archive_minutes: u32,
    /// When the thread was archived (absent when not archived).
    pub archived_at: Option<DateTime<Utc>>,
    /// Whether the thread is locked (no new messages allowed).
    pub locked: bool,
    /// When the thread was created.
    pub created_at: DateTime<Utc>,
}

/// Sort order for forum channel posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForumSortOrder {
    /// Sort by most recent activity (default).
    LatestActivity,
    /// Sort by creation date.
    CreationDate,
}

/// A forum post (thread within a forum channel).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumPost {
    /// Thread info for the backing thread channel.
    pub thread: ThreadInfo,
    /// Tag IDs applied to this post.
    pub applied_tags: Vec<String>,
    /// ID of the starter message (first post in the thread).
    pub starter_message_id: Option<String>,
}

/// A channel within a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Number of @mention notifications in this channel.
    ///
    /// Only increments when the current user is directly @mentioned
    /// (by @username, @here, @everyone, or a group they belong to),
    /// distinct from [`unread_count`] which counts all unread messages.
    /// Displayed as a red badge in the channel list; plain unread_count
    /// is shown as bold text only.
    #[serde(default)]
    pub mention_count: u32,
    /// ID of the last message (for ordering).
    pub last_message_id: Option<String>,
    /// For `Forum` channels: available tags. `None` for non-forum channels.
    #[serde(default)]
    pub forum_tags: Option<Vec<ForumTag>>,
    /// For `Thread` channels: ID of the parent text or forum channel.
    #[serde(default)]
    pub parent_channel_id: Option<String>,
    /// For `Thread` channels: thread metadata (archived, locked, auto-archive).
    #[serde(default)]
    pub thread_metadata: Option<ThreadMetadata>,
}

/// Parameters for updating a channel.
///
/// All fields are optional. The backend ignores fields it doesn't support.
/// `slow_mode_secs` uses `Option<Option<u32>>` — `None` = leave alone,
/// `Some(None)` = clear / disable, `Some(Some(n))` = set to n seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateChannelParams {
    pub name: Option<String>,
    pub topic: Option<String>,
    /// New position index for display ordering (0-based).
    pub position: Option<u32>,
    /// Slow-mode interval in seconds (0 = disabled).
    pub slow_mode_secs: Option<u32>,
    /// Whether the channel is NSFW / age-gated.
    pub nsfw: Option<bool>,
}
