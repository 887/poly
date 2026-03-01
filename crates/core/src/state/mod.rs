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
    /// Navigation history stack for back/forward.
    pub nav_history: Vec<NavigationState>,
    /// Current index in the navigation history stack.
    pub nav_history_index: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            is_setup_complete: false,
            nav: NavigationState::default(),
            settings_section: SettingsSection::Accounts,
            nav_history: Vec::new(),
            nav_history_index: 0,
        }
    }
}

impl AppState {
    /// Push a navigation state to the history stack.
    ///
    /// Truncates forward history if we navigated back and then go somewhere new.
    pub fn push_nav_history(&mut self) {
        let entry = self.nav.clone();
        // Don't push duplicates
        if self.nav_history.last() == Some(&entry) {
            return;
        }
        // If we're in the middle of history, truncate forward entries
        if self.nav_history_index < self.nav_history.len() {
            self.nav_history.truncate(self.nav_history_index);
        }
        self.nav_history.push(entry);
        self.nav_history_index = self.nav_history.len();
    }

    /// Navigate back in history. Returns true if navigation occurred.
    pub fn nav_back(&mut self) -> bool {
        if self.nav_history_index > 0 {
            // Save current state if at the end of history
            if self.nav_history_index == self.nav_history.len() {
                let current = self.nav.clone();
                self.nav_history.push(current);
            }
            self.nav_history_index -= 1;
            if let Some(entry) = self.nav_history.get(self.nav_history_index).cloned() {
                self.nav = entry;
                return true;
            }
        }
        false
    }

    /// Navigate forward in history. Returns true if navigation occurred.
    pub fn nav_forward(&mut self) -> bool {
        if self.nav_history_index + 1 < self.nav_history.len() {
            self.nav_history_index += 1;
            if let Some(entry) = self.nav_history.get(self.nav_history_index).cloned() {
                self.nav = entry;
                return true;
            }
        }
        false
    }

    /// Whether back navigation is possible.
    pub fn can_go_back(&self) -> bool {
        self.nav_history_index > 0
    }

    /// Whether forward navigation is possible.
    pub fn can_go_forward(&self) -> bool {
        self.nav_history_index + 1 < self.nav_history.len()
    }
}
