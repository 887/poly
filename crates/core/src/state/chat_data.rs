//! Reactive data stores for the chat UI.
//!
//! `ChatData` holds the currently loaded data for the active view —
//! servers, channels, messages, members, notifications, DMs, groups.
//! All data is populated from backends via the [`ClientManager`].
//!
//! Provided as `Signal<ChatData>` at the `App` level.
//!
//! ## Phase G.6 migration status
//!
//! Three sub-signal types have been extracted (plan-solid-refactor-survey.md):
//! - `ChatLists` — servers, channels, dm_channels, groups, friends, notifications + by-id shadows
//! - `ChatViewState` — messages, members, current_server/channel, typing_users, loading, etc.
//! - `AccountSessions` — account_sessions, favorited_server_ids, account_order, content_policy, blocked_users
//!
//! These are now provided as separate `BatchedSignal` contexts alongside `ChatData`
//! (see `ui.rs`). Full migration of the 262 call sites is in progress — `ChatData`
//! remains the authoritative data store until all consumers are migrated.
//!
// TODO(phase-2.5.2): Reactive Data Stores
// DECISION(V-4): VoiceMediaSettings is defined here and re-exported via VoiceState
// (phase-G.2 of plan-solid-refactor-survey.md). Persistence TBD.

use poly_client::*;
use std::collections::HashMap;

/// Runtime voice & audio settings (device selection, noise cancellation).
///
/// Held in ChatData so all voice UI components can read/write without
/// plumbing extra props. Reset on app restart (no persistence yet).
// DECISION(V-noise): noise_cancel_enabled defaults to true — AI noise reduction is on by default.
#[derive(Debug, Clone)]
pub struct VoiceMediaSettings {
    /// Whether RNNoise-based noise cancellation is enabled.
    ///
    /// When true, the mic audio pipeline routes through `nnnoiseless`
    /// before reaching the WebRTC send track (Phase 3 implementation).
    /// The toggle is functional in the UI; the actual audio worklet
    /// integration is TODO(phase-voice-3).
    /// Defaults to `true` — noise cancellation is on by default.
    pub noise_cancel_enabled: bool,
    /// Selected microphone input device ID (`None` = system default).
    pub mic_device_id: Option<String>,
    /// Selected speaker / output device ID (`None` = system default).
    pub speaker_device_id: Option<String>,
}

impl Default for VoiceMediaSettings {
    fn default() -> Self {
        Self {
            noise_cancel_enabled: true,
            mic_device_id: None,
            speaker_device_id: None,
        }
    }
}

/// Reactive data store for the chat UI.
///
/// Holds loaded data from all active backends. Updated by async tasks
/// that call into the `ClientManager`.
///
/// ## Phase G.6 note
///
/// The logical sub-structs `ChatLists`, `ChatViewState`, and `AccountSessions`
/// (in `crate::state`) are now provided as separate `BatchedSignal` contexts.
/// `ChatData` retains all fields until all call sites are migrated.
#[derive(Debug, Clone, Default)]
pub struct ChatData {
    /// All favorited servers from all backends.
    pub servers: Vec<Server>,
    /// Channels for the currently selected server.
    pub channels: Vec<Channel>,
    /// Messages for the currently selected channel.
    pub messages: Vec<Message>,
    /// Members of the currently selected channel.
    pub members: Vec<User>,
    /// Aggregated notifications from all backends.
    pub notifications: Vec<Notification>,
    /// DM channels from all backends.
    pub dm_channels: Vec<DmChannel>,
    /// Group chats from all backends.
    pub groups: Vec<Group>,
    /// Friends per account (account_id → friends list).
    pub friends: HashMap<String, Vec<User>>,
    /// Whether data is currently loading.
    pub loading: bool,
    /// Currently selected server info (for channel list header).
    pub current_server: Option<Server>,
    /// Currently selected channel info (for chat header).
    pub current_channel: Option<Channel>,
    /// Sessions keyed by account ID — one entry per active account.
    ///
    /// Used to look up `icon_emoji`, display name, and other per-account
    /// identity data in sidebar components without traversing all servers.
    pub account_sessions: HashMap<String, Session>,
    /// Server IDs that are pinned to the Favorites Bar (Bar 1).
    ///
    /// Drag a server from Bar 2 to Bar 1 to add it here. Empty means
    /// no servers are pinned (Bar 1 shows nothing in the server area).
    pub favorited_server_ids: Vec<String>,
    /// F-DC-1 — Permission-denied error for the currently selected channel.
    ///
    /// Set when a channel load fails with `ClientError::PermissionDenied`.
    /// Cleared when the channel changes. Drives a styled permission-denied
    /// empty state in `render_message_list_content` instead of the generic
    /// "no messages" wave.
    pub channel_load_error: Option<String>,
    /// Custom server ordering per account (account_id → Vec<server_id>).
    ///
    /// Populated on first drag within the Account Server Bar. Servers not
    /// listed here appear after the explicitly ordered ones.
    pub account_server_order: HashMap<String, Vec<String>>,
    /// User-preferred order of account icons in the Favorites Bar (Bar 1).
    ///
    /// Hydrated from `AppSettings.account_order` at startup. Accounts not
    /// listed here are appended alphabetically at render time so the icon
    /// layout is stable across `HashMap`-based `ClientManager.backends`
    /// iteration order.
    pub account_order: Vec<String>,
    /// Users currently typing in the selected channel.
    ///
    /// Each entry is a display name string. Updated by the event stream
    /// consumer when `TypingStarted` events arrive, cleared after a
    /// few-second timeout.
    pub typing_users: Vec<String>,
    /// Members of the currently open group DM.
    ///
    /// Populated from the `Group::members` list when a group conversation
    /// is opened. Empty for individual DMs and server channels.
    /// Used by `DmUserSidebar` to render the group member list.
    pub active_group_members: Vec<User>,
    /// Content and social policy for the currently active account.
    ///
    /// Loaded from `get_content_policy()` on account switch.
    /// Falls back to `ContentPolicy::default()` if the backend returns
    /// `NotSupported`. Written to when the user changes settings in the
    /// Content & Social settings page.
    pub content_policy: ContentPolicy,
    /// Users blocked per account (account_id → blocked list).
    pub blocked_users: HashMap<String, Vec<BlockedUser>>,
    /// Set when the most recent channel message load used `MessageQuery::around`
    /// (anchor restore). Tells `use_history_state_effect` to set `has_more_after = true`
    /// so the bottom sentinel and "Jump to Present" will chain-load newer messages.
    /// Reset to `false` after `use_history_state_effect` consumes it.
    pub messages_loaded_via_anchor: bool,
}

impl ChatData {
    /// Apply a typed [`super::chat_actions::ChatAction`] to this `ChatData`.
    ///
    /// This method is kept for backward compatibility during the G.6 migration.
    /// New code should use `ChatViewState::apply()` instead.
    ///
    /// Call inside a `.batch()` closure:
    /// ```ignore
    /// chat_data.batch(|cd| cd.apply(ChatAction::ClearChannelContext));
    /// ```
    pub fn apply(&mut self, action: super::chat_actions::ChatAction) {
        use super::chat_actions::ChatAction;
        match action {
            ChatAction::ClearChannelContext => {
                self.current_server = None;
                self.current_channel = None;
                self.channels.clear();
                self.messages.clear();
                self.members.clear();
            }
            ChatAction::ClearActiveChannel => {
                self.current_channel = None;
                self.messages.clear();
                self.members.clear();
            }
        }
    }
}

/// Format a file size in human-readable form.
#[must_use]
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // lint-allow-unused: u64→f64 lossy is acceptable for human-readable size display.
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    let kb = bytes as f64 / 1_024.0_f64;
    if kb < 1_024.0_f64 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1_024.0_f64;
    if mb < 1_024.0_f64 {
        return format!("{mb:.1} MB");
    }
    let gb = mb / 1_024.0_f64;
    format!("{gb:.2} GB")
}

/// Generic fallback badge shown as a backend source indicator.
///
/// Pre-WP-7 this function was a `match backend.as_str()` slug ladder. Per
/// D27 (plan `plan-client-ui-surface.md`), backend icons are plugin-declared
/// — the host no longer hard-codes them. Until every caller migrates to the
/// plugin's declared icon field, this returns a single generic placeholder
/// for all backends.
///
/// DECISION(D27): do not re-introduce slug→emoji mapping in this file —
/// it belongs in the plugin's declaration.
#[must_use]
pub fn backend_badge(_backend: &BackendType) -> &'static str {
    "⬜"
}

/// Get a deterministic color for a user ID (for avatar and username coloring).
///
/// Returns a CSS color string.
#[must_use]
pub fn user_color(user_id: &str) -> &'static str {
    let hash: u32 = user_id.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    let colors = [
        "#60a5fa", // blue
        "#f87171", // red
        "#4ade80", // green
        "#fbbf24", // amber
        "#a78bfa", // purple
        "#fb923c", // orange
        "#2dd4bf", // teal
        "#f472b6", // pink
    ];
    // lint-allow-unused: hash is u32, usize is at least 32 bits; modulo by a
    // non-zero const len is safe (colors has 8 entries, always nonzero).
    #[allow(clippy::as_conversions, clippy::arithmetic_side_effects)]
    let idx = (hash as usize) % colors.len();
    colors.get(idx).copied().unwrap_or("#60a5fa")
}
