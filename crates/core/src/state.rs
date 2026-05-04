//! Application state management for Poly.
//!
//! Uses Dioxus signals and global state for:
//! - Current navigation state (selected server, channel, view)
//! - Active accounts and their backends
//! - Chat data (servers, channels, messages) from backends
//! - Theme configuration
//! - i18n locale
//! - Setup wizard state

pub mod batched_signal;
pub mod bisect_log;
pub mod chat_data;
pub mod route_synced;
pub mod use_reactive_effect;
pub mod use_spawn_once;

pub use bisect_log::bisect_log;
pub use batched_signal::{BatchedSignal, PendingUpdate, use_batched_context};
pub use chat_data::{ChatData, DragSource};
pub use route_synced::RouteSynced;
pub use self::use_reactive_effect::use_reactive_effect;
pub use self::use_spawn_once::use_spawn_once;

use poly_client::{BackendType, MemberPermissions};
use poly_client::User;
use serde::{Deserialize, Serialize};

/// Which moderation dialog (if any) is currently open.
#[derive(Debug, Clone, PartialEq)]
pub enum ModerationDialog {
    Kick { server_id: String, member_id: String, member_name: String, account_id: String },
    Ban  { server_id: String, member_id: String, member_name: String, account_id: String },
    Timeout { server_id: String, member_id: String, member_name: String, account_id: String },
    EditChannel { channel_id: String, account_id: String },
}

/// How the member list groups its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MemberListGrouping {
    /// Online / Idle / Do-Not-Disturb / Offline groups (default behaviour).
    #[default]
    ByStatus,
    /// Flat list — no group headings.
    NoGrouping,
}

impl MemberListGrouping {
    /// Stable slug used in storage and HTML select values.
    #[must_use] 
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ByStatus => "by-status",
            Self::NoGrouping => "none",
        }
    }

    /// Parse from a stored slug.
    #[must_use] 
    pub fn from_slug(value: &str) -> Self {
        match value {
            "by-status" => Self::ByStatus,
            "none" => Self::NoGrouping,
            _ => Self::default(),
        }
    }
}

/// How members are sorted within each group (or globally when ungrouped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MemberListSortOrder {
    /// A-Z by display name (default).
    #[default]
    Alphabetical,
    /// Online-tier members first, then offline; A-Z within each tier.
    OnlineFirst,
    /// Preserve the order returned by the backend (join / position order).
    JoinOrder,
}

impl MemberListSortOrder {
    /// Stable slug.
    #[must_use] 
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alphabetical => "alphabetical",
            Self::OnlineFirst => "online-first",
            Self::JoinOrder => "join-order",
        }
    }

    /// Parse from a stored slug.
    #[must_use] 
    pub fn from_slug(value: &str) -> Self {
        match value {
            "alphabetical" => Self::Alphabetical,
            "online-first" => Self::OnlineFirst,
            "join-order" => Self::JoinOrder,
            _ => Self::default(),
        }
    }
}

/// Which feed the forum view shows — posts or recent comments.
///
/// Stored in `AppState.view_filter`; toggled by the Posts | Comments pill
/// in `ForumView`. Only meaningful for backends where
/// `BackendCapabilities::supports_comment_feed` is `true` (currently Lemmy).
/// All other backends ignore this field and always render posts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PostsOrComments {
    /// Show the post feed (default).
    #[default]
    Posts,
    /// Show the recent-comments feed.
    Comments,
}

/// Global shell layout mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LayoutMode {
    /// Use the mobile shell when the viewport width is <= 640px.
    #[default]
    AutoWidth,
    /// Use the mobile shell when the viewport is portrait (`height > width`).
    AutoPortrait,
    /// Always use the desktop shell regardless of viewport size.
    ForceDesktop,
    /// Always use the mobile shell regardless of viewport size.
    ForceMobile,
}

/// The main navigation views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    /// First-launch setup wizard.
    Setup,
    /// Per-account overview — default landing for every backend.
    Overview,
    /// DMs and friends list.
    DmsFriends,
    /// Friends browser — tiled grid view with filtering (account, server, search)
    Friends,
    /// Notifications feed.
    Notifications,
    /// Discover Communities page (Lemmy, Reddit).
    DiscoverCommunities,
    /// A server's channel view.
    Server,
    /// Settings page.
    Settings,
    /// Agent page — AI integrations and agent profile.
    Agent,
    /// Global search page.
    Search,
    /// Account signup flow — full-page, outside MainLayout.
    Signup,
}

/// Current navigation state.
///
/// **Route-synced fields** — `view`, `active_backend`, `active_instance_id`,
/// `active_account_id`, `selected_server`, `selected_channel` — are wrapped in
/// `RouteSynced<T>`. Reads still work via `Deref` (`nav.selected_channel.is_some()`,
/// `nav.selected_channel.as_deref()`, …). Writes are compile-locked to
/// `crate::ui::routes::sync_route_to_app_state`. To change one of these from a
/// click handler, call `nav.push(Route::…)` and let `on_update` write it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationState {
    /// Currently active view.
    pub view: RouteSynced<View>,
    /// The backend type of the account currently navigated to (e.g. Demo, Stoat).
    ///
    /// Set by the router's `on_update` callback and mirrors the `:backend` URL
    /// segment. `None` on app launch before any navigation occurs.
    pub active_backend: RouteSynced<Option<BackendType>>,
    /// The federated instance/homeserver for the active account.
    ///
    /// Mirrors the `:instance_id` URL segment. Examples: `"demo"`, `"matrix.org"`.
    /// `None` for app-level routes (`/notifications`, `/settings`).
    pub active_instance_id: RouteSynced<Option<String>>,
    /// The account ID currently navigated to.
    ///
    /// Set by the router's `on_update` callback and mirrors the `:account_id`
    /// URL segment. `None` for app-level routes (`/notifications`, `/settings`).
    pub active_account_id: RouteSynced<Option<String>>,
    /// Currently selected server ID (if in Server view).
    pub selected_server: RouteSynced<Option<String>>,
    /// Currently selected channel ID.
    pub selected_channel: RouteSynced<Option<String>>,
    /// Whether right sidebar (user list) is visible.
    pub right_sidebar_visible: bool,
    /// Whether the DM/group right member sidebar is visible.
    ///
    /// Toggled by the "Members" button in the group chat header.
    /// Independent of `right_sidebar_visible` (which controls the server member list).
    pub dm_right_sidebar_visible: bool,
    /// Mobile-only detail mode for 1:1 DM right wing.
    ///
    /// On mobile, the DM wing opens to a simple member/contact list first. Selecting
    /// the contact opens the full contact-info detail panel. Closing that detail panel
    /// should return to the list rather than collapsing the entire wing.
    pub mobile_dm_contact_detail_visible: bool,
    /// Last-visited URL per account ID.
    ///
    /// Populated by `sync_route_to_app_state` on every account-scoped navigation.
    /// Used by `FavoritesBar` to restore the account's previous page when switching.
    /// Persisted to storage so it survives page reloads.
    pub account_last_routes: std::collections::HashMap<String, String>,
    /// Last selected DM/group route per account.
    ///
    /// Unlike `account_last_routes`, this only tracks DM conversation routes so
    /// `/dms` can reopen the most recent conversation instead of the empty DM home.
    /// Persisted to storage so it survives restarts.
    pub account_last_dm_routes: std::collections::HashMap<String, String>,
    /// Currently open user profile modal target.
    ///
    /// When `Some(user)`, `UserProfileModal` renders a full-screen overlay showing
    /// the given user's profile. Set by `open_user_profile()`; cleared on close.
    /// Not serialised — cleared on every cold start.
    #[serde(skip)]
    pub profile_modal_user: Option<User>,
    /// Pending direct call intent awaiting route-backed confirmation/connection.
    ///
    /// Used by the temporary outgoing-call route: the route holds the lightweight
    /// "calling…" UI, and once it dismisses back to the DM route, the DM route
    /// consumes this request and starts the actual temporary call connection.
    #[serde(skip)]
    pub pending_direct_call: Option<PendingDirectCallRequest>,
    /// Thread panel — the thread ID currently open in the side panel on desktop.
    ///
    /// When `Some(thread_id)`, `ThreadPanel` renders alongside the parent channel
    /// chat. `None` means the panel is closed. Not serialised — always starts
    /// closed on a cold start so stale thread context never leaks across sessions.
    #[serde(skip)]
    pub thread_panel_open: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDirectCallRequest {
    pub account_id: String,
    pub dm_id: String,
    pub target_user: User,
    pub start_video: bool,
    pub allow_add_to_active_temporary: bool,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            view: RouteSynced::new(View::DmsFriends),
            active_backend: RouteSynced::new(None),
            active_instance_id: RouteSynced::new(None),
            active_account_id: RouteSynced::new(None),
            selected_server: RouteSynced::new(None),
            selected_channel: RouteSynced::new(None),
            right_sidebar_visible: true,
            dm_right_sidebar_visible: true,
            mobile_dm_contact_detail_visible: false,
            account_last_routes: std::collections::HashMap::new(),
            account_last_dm_routes: std::collections::HashMap::new(),
            profile_modal_user: None,
            pending_direct_call: None,
            thread_panel_open: None,
        }
    }
}

/// Settings for the current settings page section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsSection {
    /// Account management.
    Accounts,
    /// Backup server configuration.
    Backup,
    /// Identity and recovery.
    Identity,
    /// Theme customization.
    Theme,
    /// Language selection.
    Language,
    /// Layout behavior and mirroring.
    Layout,
    /// Appearance (dark/light mode).
    Appearance,
    /// General preferences.
    General,
    /// External media integrations (GIF providers, future rich media sources).
    Media,
    /// Notification settings.
    Notifications,
    /// Voice & Video (audio device, noise suppression, etc.).
    VoiceVideo,
    /// Diagnostics — connection stats, storage usage, account health.
    Diagnostics,
    /// On-device translation (Bergamot / browser built-in).
    Translation,
    /// Plugin manager — view and manage loaded client plugins.
    ///
    /// Plugin-provided settings pages are registered dynamically at runtime
    /// via [`crate::client_manager::ClientManager::register_plugin_settings`].
    /// No plugin-specific variants exist in this enum.
    Plugins,
}

impl SettingsSection {
    /// Convert to a URL-friendly slug used in `/settings/:section` routes.
    #[must_use] 
    pub fn to_slug(self) -> &'static str {
        match self {
            Self::Accounts | Self::Notifications => "accounts",
            Self::VoiceVideo => "voice-video",
            Self::Backup => "backup",
            Self::Identity => "identity",
            Self::Theme | Self::Appearance => "theme",
            Self::Media => "media",
            Self::Translation => "translation",
            Self::Language => "language",
            Self::Layout => "layout",
            Self::General => "general",
            Self::Plugins => "plugins",
            Self::Diagnostics => "diagnostics",
        }
    }

    /// Parse a URL slug back into a `SettingsSection`.
    /// Returns `Self::Accounts` as the default for unknown slugs.
    #[must_use] 
    pub fn from_slug(slug: &str) -> Self {
        match slug {
            "voice-video" => Self::VoiceVideo,
            "backup" => Self::Backup,
            "identity" => Self::Identity,
            "theme" => Self::Theme,
            "media" => Self::Media,
            "translation" => Self::Translation,
            "language" => Self::Language,
            "layout" => Self::Layout,
            "general" => Self::General,
            "plugins" => Self::Plugins,
            "diagnostics" => Self::Diagnostics,
            // "accounts" + unknown both fall through to Accounts (the default section).
            _ => Self::Accounts,
        }
    }
}

/// State for the active right-click server context menu.
///
/// Stored in `AppState` so the context menu component can be rendered
/// at the `MainLayout` level (above sidebar overflow clipping).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMenuState {
    /// Fixed X position (from `page_coordinates()`).
    pub x: f64,
    /// Fixed Y position (from `page_coordinates()`).
    pub y: f64,
    /// Server ID this menu was opened for.
    pub server_id: String,
    /// Human-readable server name.
    pub server_name: String,
    /// Account ID that owns this server.
    pub account_id: String,
    /// Federated instance ID for this account (mirrors `:instance_id` URL segment).
    pub instance_id: String,
    /// Backend slug ("demo", "matrix", etc.)
    pub backend_slug: String,
}

/// Where a stacked context menu is anchored on screen.
///
/// Part of plan-context-menu-quality-control.md §4.1 — the runtime stack
/// uses this to distinguish cursor-anchored desktop menus from the
/// mobile center-overlay variant and anchored-below submenus.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuAnchor {
    /// `position: fixed; left: {x}px; top: {y}px` — desktop cursor anchor.
    Cursor { x: f64, y: f64 },
    /// Mobile: centered overlay with scrim. The runtime coerces Cursor to
    /// Center when `runtime_mobile_ui_active()` returns true (§4.3.1).
    Center,
    /// Submenu anchored below a parent menu item — used by
    /// `has_arrow: true` items on desktop (§4.2.2).
    AnchoredBelow { x: f64, y: f64, width: f64 },
}

/// One entry on the active context-menu stack.
///
/// The stack-shaped replacement for the older `context_menu` /
/// `channel_context_menu` scalar fields (plan §2.3.2 / §4.1).
/// During Phase A both shapes coexist: existing `ServerContextMenu` /
/// `ChannelContextMenu` / `MsgContextMenuOverlay` still read the scalar
/// fields; new menus (`ForumPostContextMenu`, `UserRowContextMenu`) push
/// onto this stack. The scalar fields will be removed once every menu
/// migrates.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveContextMenu {
    /// Monotonic id. Used as the stack key so submenu pushes / pops stay
    /// stable under re-renders.
    pub id: u64,
    /// Where to anchor this menu.
    pub anchor: MenuAnchor,
    /// Opaque payload — usually a JSON snapshot of the trigger's props
    /// the menu will decode on render. Kept as `serde_json::Value` (not a
    /// typed `ContextMenuNode`) so the stack can hold heterogeneous
    /// menus without a central `enum ContextMenuKind`.
    pub ctx_json: serde_json::Value,
    /// Stable menu-type tag — the `type_name::<Self>()` of the
    /// `ContextMenuFor` impl. The host uses this to dispatch to the
    /// right render function.
    pub menu_type: &'static str,
    /// Should an outside click pop this entry? True for top-level menus,
    /// false for keyboard-driven submenus.
    pub dismiss_on_outside: bool,
}

/// State for the active right-click avatar context menu.
///
/// Opened by right-clicking a user avatar `<img>` in message rows.
/// Cleared by a global click on the `MainLayout` root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvatarContextMenuState {
    /// Fixed X position (from `page_coordinates()`).
    pub x: f64,
    /// Fixed Y position (from `page_coordinates()`).
    pub y: f64,
    /// The user ID of the avatar owner.
    pub user_id: String,
    /// Display name of the user.
    pub user_display_name: String,
    /// The account ID currently active (used for routing).
    pub account_id: String,
    /// Backend slug ("demo", "matrix", etc.).
    pub backend_slug: String,
    /// Federated instance ID (mirrors `:instance_id` URL segment).
    pub instance_id: String,
}

/// State for the active right-click reaction chip context menu.
///
/// Opened by right-clicking an emoji reaction pill on a message.
/// Cleared by a global click on the `MainLayout` root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReactionContextMenuState {
    /// Fixed X position (from `page_coordinates()`).
    pub x: f64,
    /// Fixed Y position (from `page_coordinates()`).
    pub y: f64,
    /// The message this reaction belongs to.
    pub message_id: String,
    /// The emoji of the reaction (e.g. "👍").
    pub emoji: String,
}

/// State for the active right-click attachment (image) context menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentContextMenuState {
    /// Fixed X position (from `page_coordinates()`).
    pub x: f64,
    /// Fixed Y position (from `page_coordinates()`).
    pub y: f64,
    /// URL of the attachment.
    pub url: String,
    /// Filename of the attachment (used for Save Image).
    pub filename: String,
}

/// State for the active right-click account-icon context menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountContextMenuState {
    pub x: f64,
    pub y: f64,
    pub account_id: String,
    pub display_name: String,
    pub backend_slug: String,
    pub instance_id: String,
}

/// State for the active right-click DM (1-on-1) context menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DmContextMenuState {
    pub x: f64,
    pub y: f64,
    /// DM channel ID (the `dm-...` id).
    pub channel_id: String,
    /// Other party's user ID.
    pub user_id: String,
    /// Other party's display name.
    pub display_name: String,
    /// Account ID that owns this DM.
    pub account_id: String,
    pub instance_id: String,
    pub backend_slug: String,
    /// Snapshot of `unread_count` at menu open — used to grey out
    /// "Mark as Read" when there's nothing to mark.
    pub unread_count: u32,
}

/// State for the active right-click group-DM context menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupDmContextMenuState {
    pub x: f64,
    pub y: f64,
    /// Group channel ID.
    pub channel_id: String,
    /// Display name of the group (or comma-joined member names).
    pub display_name: String,
    /// Account ID that owns this group.
    pub account_id: String,
    pub instance_id: String,
    pub backend_slug: String,
    /// Snapshot of `unread_count` at menu open.
    pub unread_count: u32,
}

// (DmContextMenuState / GroupDmContextMenuState above also serve the
// new menu types added in this commit.)

/// State for the active right-click channel context menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelContextMenuState {
    /// Fixed X position (from `page_coordinates()`).
    pub x: f64,
    /// Fixed Y position (from `page_coordinates()`).
    pub y: f64,
    /// Channel ID this menu was opened for.
    pub channel_id: String,
    /// Human-readable channel name.
    pub channel_name: String,
    /// Account ID that owns this channel.
    pub account_id: String,
    /// Server ID this channel belongs to.
    pub server_id: String,
    /// Federated instance ID for this account.
    pub instance_id: String,
    /// Backend slug.
    pub backend_slug: String,
}

/// Global app state provided at the root level.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Whether the app has been set up (keys generated).
    pub is_setup_complete: bool,
    /// Navigation state.
    pub nav: NavigationState,
    /// Active settings section.
    pub settings_section: SettingsSection,
    /// Global shell layout mode.
    pub layout_mode: LayoutMode,
    /// Whether the menu / wing order is mirrored.
    pub mirror_menu_layout: bool,
    /// Whether chat message rows are mirrored.
    pub mirror_chat_messages: bool,
    /// How members are grouped in the member list sidebar.
    pub member_list_grouping: MemberListGrouping,
    /// How members are sorted within groups (or globally if ungrouped).
    pub member_list_sort_order: MemberListSortOrder,
    /// Whether offline/invisible members are shown in the sidebar.
    pub member_list_show_offline: bool,
    /// One-shot seed for the next visit to the global search page.
    ///
    /// Used by account-scoped views to open the shared search route with a
    /// narrowed initial type filter (for example DMs + Groups only).
    pub search_type_seed: Option<Vec<String>>,
    /// Stack of context menus (plan-context-menu-quality-control.md Phase A).
    ///
    /// Empty = no overlay. Pushing opens a submenu; popping closes it.
    /// Consumed by `ui::context_menu::host::ContextMenuStack`. All legacy
    /// scalar `*_context_menu` fields have been migrated to this stack
    /// (Phase G.1 of plan-solid-refactor-survey.md).
    pub context_menu_stack: Vec<ActiveContextMenu>,
    /// Pack B P28 — monotonic counter incremented on receipt of a
    /// [`poly_client::ClientEvent::SidebarInvalidated`] event from any
    /// active backend. `ClientSidebar` reads this into its `use_resource`
    /// dependency list so an increment forces a re-fetch of
    /// `get_sidebar_declaration`.
    pub sidebar_invalidated_tick: u32,
    /// Last-known permissions for the currently active account in the
    /// currently active server.
    ///
    /// Populated by Wave 2/3 backend agents as users navigate into server
    /// channels. Used by `MessageContextMenu` and `UserRowContextMenu` to
    /// gate moderation affordances without a blocking async lookup on every
    /// right-click. `None` until the first successful `get_my_permissions`
    /// call for the active server.
    pub last_known_perms: Option<MemberPermissions>,
    /// Currently open moderation dialog (kick / ban / timeout / edit-channel).
    ///
    /// `None` = no dialog open. Set by context menu items; cleared by the
    /// dialog's own `on_close` handler (which also resets to `None`).
    pub active_moderation_dialog: Option<ModerationDialog>,
    /// Active feed-scope for the forum (Lemmy-style) view.
    ///
    /// One of `"subscribed"`, `"local"`, or `"all"`. Updated by the
    /// Subscribed / Local / All scope buttons in the forum sidebar
    /// (`channel_list.rs`). Read by `ForumView` to key the `ClientView`
    /// mount and pre-select the matching toolbar tab on re-mount.
    pub forum_scope: String,
    /// Active scope for the per-account overview sidebar toggles.
    ///
    /// One of `"servers"` (default), `"dms"`, `"friends"`, `"notifications"`.
    /// Updated by the toggle buttons in `OverviewSidebar`. Read by the
    /// account overview body to filter which categories of cards render.
    pub overview_scope: String,
    /// Whether the forum view shows posts or recent comments (Phase D).
    ///
    /// Toggled by the Posts | Comments pill in `ForumView`. Only meaningful
    /// when the active backend's `BackendCapabilities::supports_comment_feed`
    /// is `true`; all other backends always show posts.
    pub view_filter: PostsOrComments,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            is_setup_complete: false,
            nav: NavigationState::default(),
            settings_section: SettingsSection::Accounts,
            layout_mode: LayoutMode::AutoWidth,
            mirror_menu_layout: false,
            mirror_chat_messages: false,
            member_list_grouping: MemberListGrouping::ByStatus,
            member_list_sort_order: MemberListSortOrder::Alphabetical,
            member_list_show_offline: true,
            search_type_seed: None,
            context_menu_stack: Vec::new(),
            sidebar_invalidated_tick: 0,
            last_known_perms: None,
            active_moderation_dialog: None,
            forum_scope: "subscribed".to_string(),
            overview_scope: "servers".to_string(),
            view_filter: PostsOrComments::Posts,
        }
    }
}
