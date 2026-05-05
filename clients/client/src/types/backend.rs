//! Backend identity, capability declarations, and plugin manifest types.

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
    #[must_use]
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
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.0.as_str()
    }

    /// URL path segment used to identify this backend in routes.
    ///
    /// These slugs appear in every account-scoped URL:
    /// `/:backend/:account_id/dms`, `/:backend/:account_id/channels/…`, etc.
    #[must_use]
    pub fn slug(&self) -> &str {
        self.as_str()
    }

    /// Parse a backend slug from a URL path segment.
    ///
    /// All strings are valid — returns `Self` directly (no `Option`).
    #[must_use]
    pub fn from_slug(s: &str) -> Self {
        Self(s.to_string())
    }

}

/// Static capability lookup for a backend slug.
///
/// **Internal use only** — used to seed `ClientManager::backend_capabilities`
/// at startup and as the compile-time fallback when no live backend instance
/// is available. UI code MUST use `ClientManager::capabilities_for_slug`
/// which consults the runtime registry first (populated from each backend's
/// own `backend_capabilities()` trait impl at connect/restore time).
///
/// This function stays `pub` for processes that genuinely cannot reach the
/// runtime registry (notably the `chat-mcp` MCP server, which runs out of
/// process and only sees backend slugs from RPC payloads). UI consumers
/// inside `crates/core/` MUST go through `client_manager.peek().capabilities_for_slug(slug)`
/// instead so the per-account live values win.
#[must_use]
pub fn capabilities_for_slug_static(slug: &str) -> BackendCapabilities {
    // lint-allow-unused: explicit "hackernews" arm documents intent vs the wildcard fallback
    #[allow(clippy::match_same_arms)]
    match slug {
        "hackernews" => BackendCapabilities::READ_ONLY_FEED,
        "github" | "forgejo" => BackendCapabilities {
            notifications: NotificationSupport::Activity,
            // landing inherits LandingPage::Overview from READ_ONLY_FEED
            ..BackendCapabilities::READ_ONLY_FEED
        },
        "lemmy" | "demo_forum" => BackendCapabilities {
            has_ban: true,
            has_timed_ban: true,
            has_moderation_log: true,
            community_search: crate::ui_surface::CommunitySearchSupport::SubscribedLocalAll,
            supports_comment_feed: true,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        },
        "reddit" => BackendCapabilities {
            // Reddit has no API for moderation actions on the user-client
            // surface (mod actions live behind the modtools UI which we
            // don't scrape). Leave moderation flags off; treat reddit as
            // a forum + DMs backend.
            community_search: crate::ui_surface::CommunitySearchSupport::Single,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        },
        "matrix" => BackendCapabilities {
            voice: VoiceSupport::None,
            // B-MX moderation flags
            has_kick: true,
            has_ban: true,
            has_channel_mgmt: true,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "stoat" => BackendCapabilities {
            voice: VoiceSupport::None,
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,   // native timeout via DataMemberEdit.timeout (B-ST)
            has_channel_mgmt: true,
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "teams" => BackendCapabilities {
            supports_typing_indicators: false,
            has_roles: false,       // owner/member binary; no role concept
            has_kick: true,
            has_ban: false,         // Teams has no ban concept — hide entirely
            has_timed_ban: false,
            has_channel_mgmt: true, // name + description only
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
        "discord" | "demo" | "poly" => BackendCapabilities {
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: true,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        },
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
    /// Per-account overview — the plugin's `get_account_overview_view`
    /// rendered at `/{backend}/{instance}/{account}/overview`. Default
    /// for every backend; override only when the client genuinely wants
    /// to drop the user somewhere else first.
    Overview,
    /// Direct messages list (preferred default for chat clients where
    /// DMs are the primary surface).
    DirectMessages,
    /// First server's channel list — for clients where the server-list
    /// is the only navigation worth seeing.
    FirstServer,
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
    /// Whether the backend supports sending typing indicators.
    ///
    /// When `true`, the `send_typing` MCP tool is advertised and callable.
    /// Read-only feeds and backends without real-time presence always set this to `false`.
    pub supports_typing_indicators: bool,

    // ── Moderation / permissions capability flags ──────────────────────────
    // All default to `false`. Backend Wave-2/3 agents set them to `true`.
    // Gated by `#[serde(default)]` so older serialised capability structs
    // from WASM plugins remain backwards-compatible.

    /// Whether the backend exposes a role/permission system.
    /// Gates the Roles tab in server settings.
    #[serde(default)]
    pub has_roles: bool,

    /// Whether `kick_member` is supported.
    /// Gates the Kick button in the member context menu.
    #[serde(default)]
    pub has_kick: bool,

    /// Whether `ban_member` / `get_bans` / `unban_member` are supported.
    /// Gates the Bans tab and Ban button.
    #[serde(default)]
    pub has_ban: bool,

    /// Whether the backend supports timed bans / timeouts natively.
    /// Gates the Timeout button.
    #[serde(default)]
    pub has_timed_ban: bool,

    /// Whether `update_channel` and optionally `reorder_channels` are supported.
    /// Gates the Edit Channel dialog and drag-handle in the channel list.
    #[serde(default)]
    pub has_channel_mgmt: bool,

    /// Whether `get_moderation_log` is supported.
    /// Gates the Mod Log tab in server settings.
    #[serde(default)]
    pub has_moderation_log: bool,

    // ── Discover Communities (Phase E) ─────────────────────────────────────

    /// Whether and how the backend supports community search.
    ///
    /// `None` (default) hides the "Discover" button in Bar 1.
    /// `Single` shows a search-only view (Reddit: one scope).
    /// `SubscribedLocalAll` shows tabbed Subscribed/Local/All + search (Lemmy).
    ///
    /// `#[serde(default)]` keeps older WASM plugin capability blobs valid —
    /// they get `CommunitySearchSupport::None` on deserialisation.
    #[serde(default)]
    pub community_search: crate::ui_surface::CommunitySearchSupport,

    // ── Phase D — Posts / Comments toggle ─────────────────────────────────

    /// Whether the backend supports a community-level recent-comments feed
    /// (Phase D — Posts | Comments toggle in the forum view).
    ///
    /// When `true`, `ForumView` renders the Posts | Comments pill; clicking
    /// "Comments" calls `ForumBackend::get_recent_comments` instead of the
    /// normal post feed. Currently only Lemmy sets this to `true`.
    ///
    /// `#[serde(default)]` keeps older WASM plugin capability blobs valid —
    /// they get `false` on deserialisation.
    #[serde(default)]
    pub supports_comment_feed: bool,
}

impl BackendCapabilities {
    /// Read-only feed (Hacker News). No writes, no social graph, no voice.
    pub const READ_ONLY_FEED: Self = Self {
        messaging: MessagingModel::ReadOnly,
        dms: DmSupport::None,
        friends: FriendModel::None,
        notifications: NotificationSupport::None,
        voice: VoiceSupport::None,
        landing: LandingPage::Overview,
        supports_typing_indicators: false,
        has_roles: false,
        has_kick: false,
        has_ban: false,
        has_timed_ban: false,
        has_channel_mgmt: false,
        has_moderation_log: false,
        community_search: crate::ui_surface::CommunitySearchSupport::None,
        supports_comment_feed: false,
    };

    /// Forum-style messaging with an inbox but no friends / DMs / voice (Lemmy).
    pub const MESSAGING_NO_SOCIAL: Self = Self {
        messaging: MessagingModel::Full,
        dms: DmSupport::None,
        friends: FriendModel::None,
        notifications: NotificationSupport::Inbox,
        voice: VoiceSupport::None,
        landing: LandingPage::Overview,
        supports_typing_indicators: false,
        has_roles: false,
        has_kick: false,
        has_ban: false,
        has_timed_ban: false,
        has_channel_mgmt: false,
        has_moderation_log: false,
        community_search: crate::ui_surface::CommunitySearchSupport::None,
        supports_comment_feed: false,
    };

    /// `true` if this backend should render with the forum-style UI layout:
    /// no DMs, no voice, no friend graph. Drives badge hiding and channel-list
    /// rendering in the host. Does not depend on the messaging model — both
    /// read-only feeds (HN) and writeable forums (Lemmy) return `true`.
    #[must_use]
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
    #[must_use]
    pub const fn should_show_dms(&self) -> bool {
        !matches!(self.dms, DmSupport::None)
    }

    /// `true` if the Friends tab / management page should be rendered.
    #[must_use]
    pub const fn should_show_friends(&self) -> bool {
        !matches!(self.friends, FriendModel::None)
    }

    /// `true` if the Notifications tab / inbox should be rendered.
    #[must_use]
    pub const fn should_show_notifications(&self) -> bool {
        !matches!(self.notifications, NotificationSupport::None)
    }

    /// `true` if voice affordances (mic, deafen, voice bar) should render.
    #[must_use]
    pub const fn should_show_voice(&self) -> bool {
        !matches!(self.voice, VoiceSupport::None)
    }

    /// `true` if the message composer (textarea + send button) should be
    /// writable. `false` for read-only feeds (HN, GitHub) — the composer
    /// is replaced by a static "this channel is read-only" notice.
    #[must_use]
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
        landing: LandingPage::Overview,
        supports_typing_indicators: true,
        has_roles: false,
        has_kick: false,
        has_ban: false,
        has_timed_ban: false,
        has_channel_mgmt: false,
        has_moderation_log: false,
        community_search: crate::ui_surface::CommunitySearchSupport::None,
        supports_comment_feed: false,
    };

    /// `true` if the Discover Communities button should be shown.
    #[must_use]
    pub const fn should_show_discover(&self) -> bool {
        !matches!(self.community_search, crate::ui_surface::CommunitySearchSupport::None)
    }

    // ── Moderation capability predicates ──────────────────────────────────

    /// `true` if the Roles tab in server settings should be rendered.
    #[must_use]
    pub const fn should_show_roles(&self) -> bool {
        self.has_roles
    }

    /// `true` if the Bans tab and Ban button should be rendered.
    #[must_use]
    pub const fn should_show_bans(&self) -> bool {
        self.has_ban
    }

    /// `true` if the Mod Log tab in server settings should be rendered.
    #[must_use]
    pub const fn should_show_modlog(&self) -> bool {
        self.has_moderation_log
    }

    /// `true` if the Kick button in the member context menu should be rendered.
    #[must_use]
    pub const fn should_show_kick(&self) -> bool {
        self.has_kick
    }

    /// `true` if the Timeout button in the member context menu should be rendered.
    #[must_use]
    pub const fn should_show_timeout(&self) -> bool {
        self.has_timed_ban
    }

    /// `true` if the Edit Channel dialog and reorder drag-handle should be rendered.
    #[must_use]
    pub const fn should_show_channel_mgmt(&self) -> bool {
        self.has_channel_mgmt
    }

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

// ── Signup-link surface (plan-client-signup-link-surface Phase A) ────────────

/// How a backend exposes account signup to users.
///
/// Mirrors the WIT `signup-method` variant.
///
/// `External(url)` — open the given URL in the system browser.
/// `InApp(route)` — navigate to a plugin-declared in-app route.
/// `NotSupported` — no signup affordance (demo, read-only feeds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignupMethod {
    /// Open this URL in the host's external browser.
    External(String),
    /// Navigate to this plugin-declared in-app route.
    InApp(String),
    /// No signup link supported by this backend.
    NotSupported,
}

// ── Client-config surface (plan-client-version-override-and-sandbox Phase A) ─

/// Optional host capability a mechanism may require to function.
///
/// Mirrors the WIT `host-cap` variant in `interface client-config`.
///
/// When a mechanism declares `requires_host_cap = Some(HostCap::SandboxBrowser)`
/// and the host doesn't advertise that cap, the UI MUST disable the toggle and
/// the plugin MUST treat the mechanism as off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCap {
    /// Open a sub-browser the user can interact with for challenges
    /// (Discord captcha, OAuth flows). Stub in v1.
    SandboxBrowser,
    /// Native system tray icon (future).
    SystemTray,
    /// OS-level notifications (future).
    OsNotifications,
}

/// One toggleable "mechanism" a backend supports — a named code path the
/// user can opt into or out of.
///
/// Mirrors the WIT `mechanism` record in `interface client-config`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mechanism {
    /// Stable ID used as the storage key suffix.
    /// Example: `"captcha-sandbox"`, `"sliding-sync"`, `"browser-shim"`.
    pub id: String,
    /// FTL key for the human-readable label.
    /// Example: `"plugin-discord-mechanism-captcha-sandbox-label"`.
    pub name_key: String,
    /// Current on/off state — merged with the host-stored override.
    pub enabled: bool,
    /// If `Some`, the mechanism only functions when the host advertises
    /// the matching capability.
    pub requires_host_cap: Option<HostCap>,
    /// Optional FTL key for a longer description shown on hover.
    pub description_key: Option<String>,
}

