//! Shared data types used across all messenger backends.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Identifies which messenger backend a resource belongs to.
///
/// A string-based newtype so that new backends can be added without
/// changing this crate. Known slugs: `"stoat"`, `"matrix"`, `"discord"`,
/// `"teams"`, `"demo"`, `"demo_forum"`, `"poly"`, `"hackernews"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BackendId(String);

/// Backwards-compatible type alias — all `BackendType` type annotations
/// continue to compile unchanged; only the `BackendType::Variant` enum
/// constructors need to be replaced.
pub type BackendType = BackendId;

impl BackendId {
    /// Construct a `BackendId` from any string slug.
    pub fn new(slug: impl Into<String>) -> Self {
        Self(slug.into())
    }

    /// Return the slug as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Human-readable display name for known slugs; falls back to the slug
    /// itself for unknown backends.
    pub fn display_name(&self) -> &str {
        match self.0.as_str() {
            "stoat" => "Stoat",
            "matrix" => "Matrix",
            "discord" => "Discord",
            "teams" => "Teams",
            "demo" => "Demo",
            "demo_forum" => "Demo Forum",
            "poly" => "Poly Server",
            "hackernews" => "Hacker News",
            other => other,
        }
    }

    /// URL path segment used to identify this backend in routes.
    ///
    /// These slugs appear in every account-scoped URL:
    /// `/:backend/:account_id/dms`, `/:backend/:account_id/channels/…`, etc.
    pub fn slug(&self) -> &str {
        self.as_str()
    }

    /// Parse a backend slug from a URL path segment.
    ///
    /// All strings are valid — returns `Self` directly (no `Option`).
    pub fn from_slug(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for BackendId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for BackendId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl PartialEq<&str> for BackendId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<str> for BackendId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

/// The live connection state of a backend account to its remote server.
///
/// Updated by the event-stream consumer in each backend. The `ClientManager`
/// stores one entry per active account and exposes it for UI overlay dots.
// DECISION(DX-2.12.1): Connection status stored in ClientManager, not inside
// each ClientBackend, because the UI needs a synchronous non-async read path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Successfully authenticated and event stream / WebSocket is live.
    Connected,
    /// Attempting initial connection or reconnecting after a drop.
    Connecting,
    /// Explicitly disconnected by the user (e.g. truly-offline / appear-offline mode).
    Disconnected,
    /// Authentication rejected (4xx) or network unreachable (5xx / timeout).
    Error(String),
}

impl ConnectionStatus {
    /// Short CSS class suffix for styling, e.g. `"status-dot--connected"`.
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Error(_) => "error",
        }
    }

    /// Small indicator emoji shown on the account icon top-left badge.
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Connected => "●",
            Self::Connecting => "◌",
            Self::Disconnected => "○",
            Self::Error(_) => "✕",
        }
    }
}

/// The user-chosen availability / presence status for an account.
///
/// Stored per-account in `ClientManager` and persisted to local storage
/// so the preference survives restarts. This is a *user-chosen* setting
/// (what the user wants to appear as), distinct from [`PresenceStatus`]
/// which reflects what a remote backend reports about another user.
// DECISION(DX-2.12.2): Presence is user-chosen, not inferred from network
// state, because the user may want to appear online while actually being
// away (e.g. monitoring notifications with DnD on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccountPresence {
    /// Fully online — accepting notifications, shown as available.
    #[default]
    Online,
    /// Idle / away — typically auto-set after inactivity.
    Away,
    /// Do not disturb — suppresses notifications, still connected.
    DoNotDisturb,
    /// Appears offline to contacts but backend connection is live.
    AppearOffline,
    /// Truly offline — no backend connection is made; UI shows cached data.
    Offline,
}

impl AccountPresence {
    /// Short CSS class suffix, e.g. `"presence-dot--online"`.
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Away => "away",
            Self::DoNotDisturb => "dnd",
            Self::AppearOffline => "appear-offline",
            Self::Offline => "offline",
        }
    }

    /// Small indicator emoji shown on the account icon bottom-left badge.
    pub fn emoji(self) -> &'static str {
        match self {
            Self::Online => "●",
            Self::Away => "◑",
            Self::DoNotDisturb => "⊗",
            Self::AppearOffline => "○",
            Self::Offline => "○",
        }
    }

    /// Display name for UI labels.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::Away => "Away",
            Self::DoNotDisturb => "Do Not Disturb",
            Self::AppearOffline => "Appear Offline",
            Self::Offline => "Offline",
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
    /// Poly server Ed25519 challenge-response authentication.
    ///
    /// The `server_url` is the base URL of the poly-server instance.
    /// `private_key_bytes` are the raw 32-byte Ed25519 signing key.
    /// On signup, `username`, `email`, and `display_name` are also provided.
    /// On signin, `selected_user_id` optionally selects which server account to
    /// authenticate when multiple accounts share the same identity key.
    PolyServer {
        /// Base URL of the poly-server instance (e.g. `http://127.0.0.1:7080`).
        server_url: String,
        /// Raw 32-byte Ed25519 private key.
        private_key_bytes: Vec<u8>,
        /// Username (used for signup only).
        username: Option<String>,
        /// Email address (used for signup only).
        email: Option<String>,
        /// Display name (used for signup only).
        display_name: Option<String>,
        /// Selected server account ID for signin when one identity key maps to
        /// multiple Poly Server accounts.
        selected_user_id: Option<String>,
        /// Whether this is a signup (true) or signin (false).
        is_signup: bool,
    },
}

/// An authenticated session with a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// The authenticated user.
    pub user: User,
    /// Session token for subsequent requests.
    pub token: String,
    /// Which backend this session is for.
    pub backend: BackendType,
    /// Optional emoji/icon to visually distinguish this account in the sidebar.
    ///
    /// When `Some`, the favorites bar shows this emoji instead of the first
    /// letter of the account ID. Useful for demo accounts and for backends
    /// that wish to show a distinctive icon per account.
    pub icon_emoji: Option<String>,
    /// The federated instance/homeserver this account belongs to.
    ///
    /// Used as the `:instance_id` URL segment, enabling multiple accounts on
    /// different homeservers of the same protocol (e.g. two Matrix accounts on
    /// different homeservers) to coexist in routing.
    ///
    /// Examples: `"demo"` for demo accounts, `"matrix.org"` for a Matrix
    /// homeserver, `"discord.com"` for Discord, `"my-poly.server.com"` for
    /// a self-hosted Poly server.
    pub instance_id: String,
    /// Full backend base URL (with protocol) for reconnection after restart.
    ///
    /// Set by backends that need a URL for re-authentication (e.g. poly server
    /// stores `"http://127.0.0.1:7080"` here).  `None` for backends that do
    /// not require a URL (demo, built-in services).
    #[serde(default)]
    pub backend_url: Option<String>,
}

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
    /// Sourced via [`ClientBackend::get_server`]; `None` falls back to a
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
    /// Used by Lemmy, Reddit, and Discord Forums.
    Forum,
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
}

/// Content that can be sent in a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Native-only raw file bytes for outbound upload flows.
    ///
    /// This is populated by host-side composers before a backend send so
    /// native backends can upload files to their remote media services.
    /// Persisted / inbound attachments leave this as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_bytes: Option<Vec<u8>>,
}

impl Attachment {
    /// Construct an attachment that already exists on a remote backend.
    #[must_use]
    pub fn remote(
        id: String,
        filename: String,
        content_type: String,
        url: String,
        size: u64,
    ) -> Self {
        Self {
            id,
            filename,
            content_type,
            url,
            size,
            upload_bytes: None,
        }
    }
}

/// Lightweight preview metadata for a replied-to message.
///
/// Loaded from the backend with each message so the UI can render a Discord-like
/// reply header without fetching the original message separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageReplyPreview {
    /// Backend-specific ID of the original message.
    pub message_id: String,
    /// Author ID of the original message.
    pub author_id: String,
    /// Display name of the original message author.
    pub author_display_name: String,
    /// Optional avatar URL of the original message author.
    pub author_avatar_url: Option<String>,
    /// Short text snippet shown in the reply preview line.
    pub snippet: String,
}

/// A chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Preview of the replied-to message, if this message is a reply.
    #[serde(default)]
    pub reply_to: Option<MessageReplyPreview>,
    /// Whether the message has been edited.
    pub edited: bool,
}

/// A reaction on a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    /// Emoji or custom reaction identifier.
    pub emoji: String,
    /// Number of users who reacted with this.
    pub count: u32,
    /// Whether the authenticated user has reacted.
    pub me: bool,
}

/// A custom emoji available to the current channel/account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEmoji {
    /// Backend-specific emoji ID.
    pub id: String,
    /// Shortcode without surrounding colons (e.g. `"party_parrot"`).
    pub shortcode: String,
    /// Optional image URL for custom emoji.
    pub image_url: Option<String>,
    /// Optional Unicode fallback glyph when available.
    pub unicode_fallback: Option<String>,
    /// Whether the emoji is animated.
    pub animated: bool,
    /// Optional server/community that owns this emoji.
    pub server_id: Option<String>,
    /// Human-readable source label shown in search results.
    pub source_name: Option<String>,
}

/// A sticker available to the current channel/account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StickerItem {
    /// Backend-specific sticker ID.
    pub id: String,
    /// Sticker display name.
    pub name: String,
    /// URL to the sticker preview/full asset.
    pub image_url: String,
    /// Optional pack or collection name.
    pub pack_name: Option<String>,
    /// Optional descriptive text.
    pub description: Option<String>,
    /// Optional server/community that owns this sticker.
    pub server_id: Option<String>,
    /// Human-readable source label shown in search results.
    pub source_name: Option<String>,
    /// Asset format (e.g. `"png"`, `"apng"`, `"json"`, `"lottie"`).
    pub format: String,
}

/// Query options for fetching messages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageQuery {
    /// Fetch messages before this message ID.
    pub before: Option<String>,
    /// Fetch messages after this message ID.
    pub after: Option<String>,
    /// Fetch a window of messages centered around this message ID.
    ///
    /// Used for jump-to-message flows (search results, pinned messages,
    /// notifications) where the UI needs surrounding history even if the
    /// target message is far outside the currently loaded window.
    pub around: Option<String>,
    /// Maximum number of messages to return.
    pub limit: Option<u32>,
}

/// Query options for backend-native message search.
///
/// Models Discord-like search primitives while remaining generic enough for
/// backends that expose different server-side search APIs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSearchQuery {
    /// Free-text search string.
    pub text: String,
    /// Restrict search to a specific channel, if supported.
    pub channel_id: Option<String>,
    /// Restrict search to a specific server/community, if supported.
    pub server_id: Option<String>,
    /// Restrict search to a specific author, if supported.
    pub author_id: Option<String>,
    /// Restrict search to messages containing a link.
    pub has_link: bool,
    /// Restrict search to messages mentioning a specific user.
    pub mentions_user_id: Option<String>,
    /// Maximum number of hits to return.
    pub limit: Option<u32>,
}

/// A backend search result hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSearchHit {
    /// Channel containing the hit.
    pub channel_id: String,
    /// Optional display name for the channel containing the hit.
    pub channel_name: Option<String>,
    /// Optional server/community containing the hit.
    pub server_id: Option<String>,
    /// The matched message.
    pub message: Message,
}

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Generic notification.
    Other(String),
}

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

/// Identifies a configured account (backend + credentials).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// The scope in which a slash command is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandScope {
    /// Available everywhere — any channel, DM, and group DM.
    Global,
    /// Available in server text channels only (not DMs).
    Channel,
    /// Available in DMs and group DMs only.
    DirectMessage,
}

/// A slash command available in a channel.
///
/// Returned by [`ClientBackend::get_channel_commands`] to populate the `/`
/// autocomplete popup in the composer. Built-in Poly commands are added by the
/// UI layer; backend- or bot-provided commands are injected by each client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCommand {
    /// Command name without the leading `/` (e.g. `"shrug"`).
    pub name: String,
    /// Short description shown in the autocomplete popup.
    pub description: String,
    /// Display name of the app or bot providing this command
    /// (e.g. `"Built-in"`, `"MusicCat"`, `"ModBot"`).
    pub provider: String,
    /// Whether this is a Poly built-in command (shown in a separate section).
    pub is_builtin: bool,
    /// Optional usage hint shown after the command name (e.g. `"<song URL>"`).
    pub usage: Option<String>,
    /// Scope in which this command is available.
    pub scope: CommandScope,
}
