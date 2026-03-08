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
use serde::{Deserialize, Serialize};

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
    /// Last-visited URL per account ID.
    ///
    /// Populated by `sync_route_to_app_state` on every account-scoped navigation.
    /// Used by `FavoritesBar` to restore the account's previous page when switching.
    /// Persisted to storage so it survives page reloads.
    pub account_last_routes: std::collections::HashMap<String, String>,
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
            account_last_routes: std::collections::HashMap::new(),
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
    /// Demo data settings — toggle demo client, regenerate data.
    Demo,
    /// Plugin manager — view and manage loaded client plugins.
    Plugins,
    /// Plugin-provided settings pages — each backend can expose its own settings.
    PluginSettings,
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
            Self::Language => "language",
            Self::General => "general",
            Self::Plugins => "plugins",
            Self::PluginSettings => "plugin-settings",
            Self::Diagnostics => "diagnostics",
            Self::Demo => "demo",
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
            "language" => Self::Language,
            "general" => Self::General,
            "plugins" => Self::Plugins,
            "plugin-settings" => Self::PluginSettings,
            "diagnostics" => Self::Diagnostics,
            "demo" => Self::Demo,
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

/// Global app state provided at the root level.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Whether the app has been set up (keys generated).
    pub is_setup_complete: bool,
    /// Navigation state.
    pub nav: NavigationState,
    /// Active settings section.
    pub settings_section: SettingsSection,
    /// Active right-click context menu, if any.
    ///
    /// Set by `oncontextmenu` on server icons; cleared by a global
    /// click handler in `MainLayout`.
    pub context_menu: Option<ContextMenuState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            is_setup_complete: false,
            nav: NavigationState::default(),
            settings_section: SettingsSection::Accounts,
            context_menu: None,
        }
    }
}
