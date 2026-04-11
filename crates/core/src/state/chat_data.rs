//! Reactive data stores for the chat UI.
//!
//! `ChatData` holds the currently loaded data for the active view —
//! servers, channels, messages, members, notifications, DMs, groups.
//! All data is populated from backends via the [`ClientManager`].
//!
//! Provided as `Signal<ChatData>` at the `App` level.
// TODO(phase-2.5.2): Reactive Data Stores
// DECISION(V-4): VoiceMediaSettings lives in ChatData (runtime state);
// persistence across sessions can be added later via storage.

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

/// Source of the current HTML5 drag operation.
///
/// Distinguishes what kind of element started the drag so drop handlers
/// can apply the correct reorder or add-to-favorites logic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DragSource {
    /// No drag in progress.
    #[default]
    None,
    /// Dragging a favorite server icon in Bar 1 (reorder within favorites).
    FavoriteServer,
    /// Dragging an account icon in Bar 1 (reorder within accounts).
    AccountIcon,
    /// Dragging a server icon in Bar 2 AccountServerBar
    /// (reorder within Bar 2, or drop onto Bar 1 to favorite).
    AccountServer,
}

/// Reactive data store for the chat UI.
///
/// Holds loaded data from all active backends. Updated by async tasks
/// that call into the `ClientManager`.
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
    /// Participants in each voice channel, keyed by channel ID.
    pub voice_channel_participants: HashMap<String, Vec<VoiceParticipant>>,
    /// The local user's current voice connection (None if not in a call).
    pub voice_connection: Option<VoiceConnection>,
    /// Voice calls currently on hold.
    ///
    /// Poly only renders one active call at a time, but temporary direct calls can
    /// suspend the previously active call (similar to Teams/Discord) so the user can
    /// swap back later.
    pub held_voice_connections: Vec<VoiceConnection>,
    /// Voice and audio device settings (noise cancel, mic/speaker selection).
    pub voice_media_settings: VoiceMediaSettings,
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
    /// Server ID currently being dragged (set on dragstart, cleared on drop/dragend).
    ///
    /// Used to pass drag state from Bar 2 (Account Server Bar) to Bar 1 (Favorites Bar)
    /// without needing browser DataTransfer API access.
    pub dragging_server_id: Option<String>,
    /// Source of the current drag operation.
    pub drag_source: DragSource,
    /// ID of the element currently being hovered over as a drop target.
    ///
    /// Set on `ondragover` of individual items so the parent can determine
    /// where to insert the dragged item on `ondrop`.
    pub drag_over_id: Option<String>,
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

/// Format a file size in human-readable form.
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let kb = bytes as f64 / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.1} MB");
    }
    let gb = mb / 1024.0;
    format!("{gb:.2} GB")
}

/// Get an emoji badge for a backend type (used as source indicator).
pub fn backend_badge(backend: &BackendType) -> &'static str {
    match backend.as_str() {
        "stoat" => "🟣",
        "matrix" => "🔵",
        "discord" => "🟢",
        "teams" => "🟡",
        "demo" => "🧪",
        "demo_forum" => "📋",
        "poly" => "🔶",
        "hackernews" => "🟠",
        "github" => "🐙",
        _ => "⬜",
    }
}

/// Get a deterministic color for a user ID (for avatar and username coloring).
///
/// Returns a CSS color string.
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
    colors
        .get((hash as usize) % colors.len())
        .copied()
        .unwrap_or("#60a5fa")
}
