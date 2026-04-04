//! Application state management for Poly.
//!
//! Uses Dioxus signals and global state for:
//! - Current navigation state (selected server, channel, view)
//! - Active accounts and their backends
//! - Chat data (servers, channels, messages) from backends
//! - Theme configuration
//! - i18n locale
//! - Setup wizard state

pub mod chat_data;

pub use chat_data::{ChatData, DragSource};

use poly_client::BackendType;
use poly_client::User;
use serde::{Deserialize, Serialize};

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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ByStatus => "by-status",
            Self::NoGrouping => "none",
        }
    }

    /// Parse from a stored slug.
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alphabetical => "alphabetical",
            Self::OnlineFirst => "online-first",
            Self::JoinOrder => "join-order",
        }
    }

    /// Parse from a stored slug.
    pub fn from_slug(value: &str) -> Self {
        match value {
            "alphabetical" => Self::Alphabetical,
            "online-first" => Self::OnlineFirst,
            "join-order" => Self::JoinOrder,
            _ => Self::default(),
        }
    }
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
    /// DMs and friends list.
    DmsFriends,
    /// Friends browser — tiled grid view with filtering (account, server, search)
    Friends,
    /// Notifications feed.
    Notifications,
    /// A server's channel view.
    Server,
    /// Settings page.
    Settings,
    /// Global search page.
    Search,
    /// Account signup flow — full-page, outside MainLayout.
    Signup,
}

/// Current navigation state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationState {
    /// Currently active view.
    pub view: View,
    /// The backend type of the account currently navigated to (e.g. Demo, Stoat).
    ///
    /// Set by the router's `on_update` callback and mirrors the `:backend` URL
    /// segment. `None` on app launch before any navigation occurs.
    pub active_backend: Option<BackendType>,
    /// The federated instance/homeserver for the active account.
    ///
    /// Mirrors the `:instance_id` URL segment. Examples: `"demo"`, `"matrix.org"`.
    /// `None` for app-level routes (`/notifications`, `/settings`).
    pub active_instance_id: Option<String>,
    /// The account ID currently navigated to.
    ///
    /// Set by the router's `on_update` callback and mirrors the `:account_id`
    /// URL segment. `None` for app-level routes (`/notifications`, `/settings`).
    pub active_account_id: Option<String>,
    /// Currently selected server ID (if in Server view).
    pub selected_server: Option<String>,
    /// Currently selected channel ID.
    pub selected_channel: Option<String>,
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
            view: View::DmsFriends,
            active_backend: None,
            active_instance_id: None,
            active_account_id: None,
            selected_server: None,
            selected_channel: None,
            right_sidebar_visible: true,
            dm_right_sidebar_visible: true,
            mobile_dm_contact_detail_visible: false,
            account_last_routes: std::collections::HashMap::new(),
            account_last_dm_routes: std::collections::HashMap::new(),
            profile_modal_user: None,
            pending_direct_call: None,
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
    /// AI provider configuration (API keys, model selection, feature toggles).
    Ai,
    /// Notification settings.
    Notifications,
    /// Voice & Video (audio device, noise suppression, etc.).
    VoiceVideo,
    /// Diagnostics — connection stats, storage usage, account health.
    Diagnostics,
    /// Plugin manager — view and manage loaded client plugins.
    ///
    /// Plugin-provided settings pages are registered dynamically at runtime
    /// via [`crate::client_manager::ClientManager::register_plugin_settings`].
    /// No plugin-specific variants exist in this enum.
    Plugins,
}

impl SettingsSection {
    /// Convert to a URL-friendly slug used in `/settings/:section` routes.
    pub fn to_slug(self) -> &'static str {
        match self {
            Self::Accounts | Self::Notifications => "accounts",
            Self::VoiceVideo => "voice-video",
            Self::Backup => "backup",
            Self::Identity => "identity",
            Self::Theme | Self::Appearance => "theme",
            Self::Media => "media",
            Self::Ai => "ai",
            Self::Language => "language",
            Self::Layout => "layout",
            Self::General => "general",
            Self::Plugins => "plugins",
            Self::Diagnostics => "diagnostics",
        }
    }

    /// Parse a URL slug back into a `SettingsSection`.
    /// Returns `Self::Accounts` as the default for unknown slugs.
    pub fn from_slug(slug: &str) -> Self {
        match slug {
            "accounts" => Self::Accounts,
            "voice-video" => Self::VoiceVideo,
            "backup" => Self::Backup,
            "identity" => Self::Identity,
            "theme" => Self::Theme,
            "media" => Self::Media,
            "ai" => Self::Ai,
            "language" => Self::Language,
            "layout" => Self::Layout,
            "general" => Self::General,
            "plugins" => Self::Plugins,
            "diagnostics" => Self::Diagnostics,
            _ => Self::Accounts,
        }
    }
}

/// State for the active right-click server context menu.
///
/// Stored in `AppState` so the context menu component can be rendered
/// at the `MainLayout` level (above sidebar overflow clipping).
#[derive(Debug, Clone)]
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

/// State for the active right-click channel context menu.
#[derive(Debug, Clone)]
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
    /// Active right-click context menu, if any.
    ///
    /// Set by `oncontextmenu` on server icons; cleared by a global
    /// click handler in `MainLayout`.
    pub context_menu: Option<ContextMenuState>,
    /// Active right-click channel context menu, if any.
    pub channel_context_menu: Option<ChannelContextMenuState>,
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
            context_menu: None,
            channel_context_menu: None,
        }
    }
}
