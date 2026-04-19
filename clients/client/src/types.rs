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

    /// Human-readable display name for the backend.
    ///
    /// # Deprecation
    ///
    /// Per D12 of `docs/plans/plan-client-ui-surface.md`, the authoritative
    /// human-readable backend name is the plugin's own `backend-name` WIT
    /// export (`ClientBackend::backend_name()` on the trait). UI callers
    /// should prefer the plugin-provided string. This method stays as a
    /// fallback for call sites that only have a `BackendType` (slug) in
    /// hand, and simply returns the slug itself — never a hardcoded brand
    /// name.
    pub fn display_name(&self) -> &str {
        self.0.as_str()
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

    /// Returns `true` for backends whose capabilities match the "forum layout"
    /// pattern — no DMs, no voice, no friend graph. Used by the UI to pick
    /// between the chat-style and forum-style channel list / badge rules.
    ///
    /// Capability-derived, not a hard-coded slug list — see [`capabilities_for_slug`].
    pub fn uses_forum_layout(&self) -> bool {
        capabilities_for_slug(self.0.as_str()).is_forum_layout()
    }
}

/// Capability lookup for a backend slug.
///
/// Mirrors each plugin's `ClientBackend::backend_capabilities()` override so
/// the UI can read a backend's shape without holding a live client instance
/// (e.g. from a `BackendType` in a server list).
pub fn capabilities_for_slug(slug: &str) -> BackendCapabilities {
    match slug {
        "hackernews" => BackendCapabilities::READ_ONLY_FEED,
        "github" | "forgejo" => BackendCapabilities {
            notifications: NotificationSupport::Activity,
            landing: LandingPage::ServerOverview,
            ..BackendCapabilities::READ_ONLY_FEED
        },
        "lemmy" | "demo_forum" => BackendCapabilities::MESSAGING_NO_SOCIAL,
        "matrix" => BackendCapabilities {
            voice: VoiceSupport::None,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "stoat" => BackendCapabilities {
            voice: VoiceSupport::None,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "teams" => BackendCapabilities::FULL_SOCIAL_CHAT,
        "discord" | "demo" | "poly" => BackendCapabilities::FULL_SOCIAL_CHAT,
        _ => BackendCapabilities::READ_ONLY_FEED,
    }
}

/// Returns `true` for backends that allow creating a server/workspace in the
/// host UI (Discord, Demo, Poly). All other backends show the unsupported
/// placeholder instead of the create-server form.
#[must_use]
pub fn slug_supports_creating_server(slug: &str) -> bool {
    matches!(slug, "discord" | "demo" | "poly")
}

/// WP-6 — Per-plugin terminology.
///
/// Container-level terminology varies by plugin: Discord calls them "servers",
/// Matrix calls them "spaces", Lemmy calls them "communities", GitHub calls
/// them "repositories". This function returns the FTL label key for the
/// container noun at various grammatical forms (`"create"`, `"list"`, etc.).
///
/// A plugin that doesn't override the terminology falls back to the generic
/// `term-container-*` keys defined in `locales/en/main.ftl`.
#[must_use]
pub fn container_label_key(slug: &str, form: ContainerLabelForm) -> &'static str {
    match (slug, form) {
        ("lemmy" | "demo_forum", ContainerLabelForm::Singular) => "term-container-community",
        ("lemmy" | "demo_forum", ContainerLabelForm::Plural) => "term-container-community-plural",
        ("lemmy" | "demo_forum", ContainerLabelForm::CreateAction) => "term-container-community-create",

        ("matrix", ContainerLabelForm::Singular) => "term-container-space",
        ("matrix", ContainerLabelForm::Plural) => "term-container-space-plural",
        ("matrix", ContainerLabelForm::CreateAction) => "term-container-space-create",

        ("teams", ContainerLabelForm::Singular) => "term-container-team",
        ("teams", ContainerLabelForm::Plural) => "term-container-team-plural",
        ("teams", ContainerLabelForm::CreateAction) => "term-container-team-create",

        ("github", ContainerLabelForm::Singular) => "term-container-repo",
        ("github", ContainerLabelForm::Plural) => "term-container-repo-plural",
        ("github", ContainerLabelForm::CreateAction) => "term-container-repo-create",

        ("hackernews", ContainerLabelForm::Singular) => "term-container-feed",
        ("hackernews", ContainerLabelForm::Plural) => "term-container-feed-plural",
        ("hackernews", ContainerLabelForm::CreateAction) => "term-container-feed-create",

        (_, ContainerLabelForm::Singular) => "term-container-server",
        (_, ContainerLabelForm::Plural) => "term-container-server-plural",
        (_, ContainerLabelForm::CreateAction) => "term-container-server-create",
    }
}

/// Grammatical form of a container label — singular noun, plural noun, or
/// the verb phrase used in the "Create" action button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerLabelForm {
    Singular,
    Plural,
    CreateAction,
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
    /// Backend rejected the stored auth token (401 / invalid session). User must
    /// sign in again. Surfaced on the account icon even for forge/forum backends
    /// that otherwise have no connection status.
    Unauthenticated(String),
    /// Transport error — network unreachable, 5xx, timeout, parse failure, etc.
    /// Distinct from `Unauthenticated` because the user can't fix it by re-logging.
    Error(String),
}

impl ConnectionStatus {
    /// Short CSS class suffix for styling, e.g. `"status-dot--connected"`.
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Unauthenticated(_) => "unauthenticated",
            Self::Error(_) => "error",
        }
    }

    /// Small indicator emoji shown on the account icon top-left badge.
    pub fn emoji(&self) -> &'static str {
        match self {
            Self::Connected => "●",
            Self::Connecting => "◌",
            Self::Disconnected => "○",
            Self::Unauthenticated(_) => "🔑",
            Self::Error(_) => "✕",
        }
    }

    /// True when the UI should surface a reauthentication affordance for this
    /// account (prominent icon + toast notification). Forge/forum backends
    /// never show a connection-status badge, but they DO show this one.
    pub fn needs_reauth(&self) -> bool {
        matches!(self, Self::Unauthenticated(_))
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
}

/// Kind of a file system entry returned by [`ClientBackend::list_files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Git submodule pointer.
    Submodule,
}

/// One entry in a directory listing returned by [`ClientBackend::list_files`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Repository-relative path of the entry (e.g. `"src/lib.rs"`).
    pub path: String,
    /// Display name (basename) of the entry.
    pub name: String,
    /// Kind of entry — file, directory, symlink, submodule.
    pub kind: FileKind,
    /// File size in bytes. `0` for directories.
    pub size: u64,
}

/// Raw file content returned by [`ClientBackend::read_file`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContent {
    /// Repository-relative path of the file.
    pub path: String,
    /// Raw file bytes (may be binary; UI decodes as needed).
    pub bytes: Vec<u8>,
    /// Whether the response was truncated by a backend size limit.
    pub truncated: bool,
}

/// Output of a host-mediated subprocess invocation made by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecOutput {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    pub stderr: Vec<u8>,
}

/// Messaging model — does the backend accept user writes?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessagingModel {
    /// No messaging surface at all (pure feed with no posts).
    None,
    /// Read-only: user can observe but not send (Hacker News, GitHub).
    ReadOnly,
    /// Full messaging: user can post and reply (Lemmy, Discord, Matrix…).
    Full,
}

/// Direct-message support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DmSupport {
    /// Backend has no concept of DMs.
    None,
    /// User-to-user DMs (Discord, Matrix, Teams).
    User,
}

/// Friend / contact list model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FriendModel {
    /// No friends concept (HN, Lemmy, GitHub).
    None,
    /// Full bidirectional friends list (Discord).
    Full,
}

/// Notification inbox model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationSupport {
    /// No notification surface.
    None,
    /// Message-style inbox (Lemmy replies, Matrix mentions).
    Inbox,
    /// Activity stream with categories (GitHub issues/PRs, Discord mentions).
    Activity,
}

/// Landing page preference — determines what the host shows when the user
/// clicks the account icon in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandingPage {
    /// Direct messages list (Discord, Matrix, Teams).
    DirectMessages,
    /// First server's channel list (HN, Lemmy).
    FirstServer,
    /// Repo/server overview with search and attention items (GitHub, Forgejo).
    ServerOverview,
}

/// Voice / video channel support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceSupport {
    /// No voice at all.
    None,
    /// Full voice channels (Discord, Teams).
    Full,
}

/// Capability declaration for a backend.
///
/// Drives which UI affordances the host renders for a given account
/// (nav buttons, composer state, notification filter chips, voice toggle)
/// and which MCP tools are honest to advertise.
///
/// See `docs/plans/phase-2.20-plugin-capabilities-plan.md` for rationale.
///
/// This struct is the minimal messaging-shape summary used by the host.
/// It covers messaging model, DMs, friends, notifications, voice, and
/// landing page — the dimensions that drive host-owned UI affordances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub messaging: MessagingModel,
    pub dms: DmSupport,
    pub friends: FriendModel,
    pub notifications: NotificationSupport,
    pub voice: VoiceSupport,
    pub landing: LandingPage,
}

impl BackendCapabilities {
    /// Read-only feed (Hacker News). No writes, no social graph, no voice.
    pub const READ_ONLY_FEED: Self = Self {
        messaging: MessagingModel::ReadOnly,
        dms: DmSupport::None,
        friends: FriendModel::None,
        notifications: NotificationSupport::None,
        voice: VoiceSupport::None,
        landing: LandingPage::FirstServer,
    };

    /// Forum-style messaging with an inbox but no friends / DMs / voice (Lemmy).
    pub const MESSAGING_NO_SOCIAL: Self = Self {
        messaging: MessagingModel::Full,
        dms: DmSupport::None,
        friends: FriendModel::None,
        notifications: NotificationSupport::Inbox,
        voice: VoiceSupport::None,
        landing: LandingPage::FirstServer,
    };

    /// `true` if this backend should render with the forum-style UI layout:
    /// no DMs, no voice, no friend graph. Drives badge hiding and channel-list
    /// rendering in the host. Does not depend on the messaging model — both
    /// read-only feeds (HN) and writeable forums (Lemmy) return `true`.
    pub const fn is_forum_layout(&self) -> bool {
        matches!(self.dms, DmSupport::None)
            && matches!(self.voice, VoiceSupport::None)
            && matches!(self.friends, FriendModel::None)
    }

    // ── Pack-F capability gates (UI affordance predicates) ─────────────────
    //
    // Small predicates used by `AccountServerBar`, `AccountBar`, `ChatView`
    // etc. to decide whether to render account-scoped affordances (DMs tab,
    // Friends tab, Notifications tab, voice mic/deafen, composer textarea).
    //
    // Keeping them as methods on `BackendCapabilities` rather than free
    // helpers in `crates/core` means the predicate logic lives next to the
    // field definitions and can be reused from plugin-level tests
    // (`clients/<name>/tests/capabilities.rs`).

    /// `true` if the DMs tab / sidebar column should be rendered.
    pub const fn should_show_dms(&self) -> bool {
        !matches!(self.dms, DmSupport::None)
    }

    /// `true` if the Friends tab / management page should be rendered.
    pub const fn should_show_friends(&self) -> bool {
        !matches!(self.friends, FriendModel::None)
    }

    /// `true` if the Notifications tab / inbox should be rendered.
    pub const fn should_show_notifications(&self) -> bool {
        !matches!(self.notifications, NotificationSupport::None)
    }

    /// `true` if voice affordances (mic, deafen, voice bar) should render.
    pub const fn should_show_voice(&self) -> bool {
        !matches!(self.voice, VoiceSupport::None)
    }

    /// `true` if the message composer (textarea + send button) should be
    /// writable. `false` for read-only feeds (HN, GitHub) — the composer
    /// is replaced by a static "this channel is read-only" notice.
    pub const fn composer_writable(&self) -> bool {
        matches!(self.messaging, MessagingModel::Full)
    }

    /// Full social chat (Discord, Matrix, Teams).
    pub const FULL_SOCIAL_CHAT: Self = Self {
        messaging: MessagingModel::Full,
        dms: DmSupport::User,
        friends: FriendModel::Full,
        notifications: NotificationSupport::Activity,
        voice: VoiceSupport::Full,
        landing: LandingPage::DirectMessages,
    };
}

impl Default for BackendCapabilities {
    fn default() -> Self {
        Self::READ_ONLY_FEED
    }
}

/// A single row in a capability summary — an FTL label key paired with the
/// FTL value key describing how the backend supports that dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityRow {
    pub label_key: &'static str,
    pub value_key: &'static str,
}

impl BackendCapabilities {
    /// Return the "shape" rows (messaging model, DMs, friends, notifications,
    /// voice) as FTL key pairs. Drives the capability details panel in
    /// Settings > Plugins.
    #[must_use]
    pub fn shape_rows(&self) -> Vec<CapabilityRow> {
        vec![
            CapabilityRow {
                label_key: "cap-label-messaging",
                value_key: match self.messaging {
                    MessagingModel::None => "cap-value-messaging-none",
                    MessagingModel::ReadOnly => "cap-value-messaging-readonly",
                    MessagingModel::Full => "cap-value-messaging-full",
                },
            },
            CapabilityRow {
                label_key: "cap-label-dms",
                value_key: match self.dms {
                    DmSupport::None => "cap-value-dms-none",
                    DmSupport::User => "cap-value-dms-user",
                },
            },
            CapabilityRow {
                label_key: "cap-label-friends",
                value_key: match self.friends {
                    FriendModel::None => "cap-value-friends-none",
                    FriendModel::Full => "cap-value-friends-full",
                },
            },
            CapabilityRow {
                label_key: "cap-label-notifications",
                value_key: match self.notifications {
                    NotificationSupport::None => "cap-value-notifications-none",
                    NotificationSupport::Inbox => "cap-value-notifications-inbox",
                    NotificationSupport::Activity => "cap-value-notifications-activity",
                },
            },
            CapabilityRow {
                label_key: "cap-label-voice",
                value_key: match self.voice {
                    VoiceSupport::None => "cap-value-voice-none",
                    VoiceSupport::Full => "cap-value-voice-full",
                },
            },
        ]
    }

}

/// A plugin's self-declared manifest.
///
/// Purely informational — the host does NOT enforce these declarations at
/// runtime. They exist so users can inspect what a loaded plugin says it
/// will do (subprocess programs, HTTP host patterns, description).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginManifest {
    /// Subprocess programs the plugin spawns via the `exec-command` host fn.
    pub exec_programs: Vec<String>,
    /// HTTP host patterns the plugin calls (e.g. `"api.github.com"`).
    /// Empty list means no HTTP.
    pub http_hosts: Vec<String>,
    /// Free-text description of the plugin and its access needs.
    pub description: String,
    /// Optional homepage / source URL.
    pub homepage: Option<String>,
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
    /// Stored auth token was rejected (401). User must sign in again for this
    /// account before it can be used. Carries the backend slug so the UI can
    /// route "Reconnect" clicks straight to the right signup flow.
    ReauthRequired { backend_slug: String },
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod pack_f_capability_gates {
    //! Pack-F regression: per-backend capability-gate helpers must match
    //! the declared `capabilities_for_slug` table. Table-driven to catch
    //! drift when a new backend is added or a declaration flips.

    use super::*;

    fn expected(
        slug: &str,
    ) -> (
        bool, // should_show_dms
        bool, // should_show_friends
        bool, // should_show_notifications
        bool, // should_show_voice
        bool, // composer_writable
    ) {
        match slug {
            "hackernews" => (false, false, false, false, false),
            "github" | "forgejo" => (false, false, true, false, false),
            "lemmy" | "demo_forum" => (false, false, true, false, true),
            "matrix" => (true, true, true, false, true),
            "stoat" => (true, true, true, false, true),
            "teams" => (true, true, true, true, true),
            "discord" | "demo" | "poly" => (true, true, true, true, true),
            _ => (false, false, false, false, false),
        }
    }

    fn check(slug: &str) {
        let caps = capabilities_for_slug(slug);
        let (dms, friends, notifs, voice, composer) = expected(slug);
        assert_eq!(caps.should_show_dms(), dms, "{slug}: should_show_dms");
        assert_eq!(
            caps.should_show_friends(),
            friends,
            "{slug}: should_show_friends"
        );
        assert_eq!(
            caps.should_show_notifications(),
            notifs,
            "{slug}: should_show_notifications"
        );
        assert_eq!(caps.should_show_voice(), voice, "{slug}: should_show_voice");
        assert_eq!(
            caps.composer_writable(),
            composer,
            "{slug}: composer_writable"
        );
    }

    #[test]
    fn hackernews_hides_everything_social() {
        check("hackernews");
    }

    #[test]
    fn github_shows_only_notifications() {
        check("github");
    }

    #[test]
    fn forgejo_shows_only_notifications() {
        check("forgejo");
    }

    #[test]
    fn lemmy_writeable_forum_no_social() {
        check("lemmy");
    }

    #[test]
    fn demo_forum_matches_lemmy() {
        check("demo_forum");
    }

    #[test]
    fn matrix_full_social_no_voice() {
        check("matrix");
    }

    #[test]
    fn stoat_full_social_no_voice() {
        check("stoat");
    }

    #[test]
    fn teams_full_social_with_voice() {
        check("teams");
    }

    #[test]
    fn discord_full_everything() {
        check("discord");
    }

    #[test]
    fn demo_full_everything() {
        check("demo");
    }

    #[test]
    fn poly_full_everything() {
        check("poly");
    }

    #[test]
    fn unknown_slug_is_read_only_feed() {
        check("definitely-not-a-real-plugin");
    }
}
