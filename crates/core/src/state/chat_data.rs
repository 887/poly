//! Reactive data stores for the chat UI.
//!
//! `ChatData` holds the currently loaded data for the active view —
//! servers, channels, messages, members, notifications, DMs, groups.
//! All data is populated from backends via the [`ClientManager`].
//!
//! Provided as `Signal<ChatData>` at the `App` level.
// TODO(phase-2.5.2): Reactive Data Stores

use poly_client::*;
use std::collections::HashMap;

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
    /// Friends from all backends.
    pub friends: Vec<User>,
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
    /// The authenticated session for the most recently activated account.
    ///
    /// Used to identify the local user (e.g. when joining a voice channel).
    /// In a multi-account world this is the "primary" or last-activated session.
    pub local_session: Option<Session>,
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
    /// Users currently typing in the selected channel.
    ///
    /// Each entry is a display name string. Updated by the event stream
    /// consumer when `TypingStarted` events arrive, cleared after a
    /// few-second timeout.
    pub typing_users: Vec<String>,
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
    match backend {
        BackendType::Stoat => "🟣",
        BackendType::Matrix => "🔵",
        BackendType::Discord => "🟢",
        BackendType::Teams => "🟡",
        BackendType::Demo => "🧪",
        BackendType::Poly => "🔶",
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
