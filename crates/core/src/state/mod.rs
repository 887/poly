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

pub use chat_data::ChatData;

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
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            view: View::DmsFriends,
            active_backend: None,
            active_account_id: None,
            selected_server: None,
            selected_channel: None,
            right_sidebar_visible: true,
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
    /// Notification settings.
    Notifications,
    /// Voice & Video (audio device, noise suppression, etc.).
    VoiceVideo,
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            is_setup_complete: false,
            nav: NavigationState::default(),
            settings_section: SettingsSection::Accounts,
        }
    }
}
