//! Authentication, session, presence, and account types.

use serde::{Deserialize, Serialize};

use super::backend::BackendType;
use super::user::User;

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
    #[must_use]
    pub const fn css_class(&self) -> &'static str {
        match self {
            Self::Connected => "connected",
            Self::Connecting => "connecting",
            Self::Disconnected => "disconnected",
            Self::Unauthenticated(_) => "unauthenticated",
            Self::Error(_) => "error",
        }
    }

    /// Small indicator emoji shown on the account icon top-left badge.
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
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
    #[must_use]
    pub const fn needs_reauth(&self) -> bool {
        matches!(self, Self::Unauthenticated(_))
    }
}

/// The user-chosen availability / presence status for an account.
///
/// Stored per-account in `ClientManager` and persisted to local storage
/// so the preference survives restarts. This is a *user-chosen* setting
/// (what the user wants to appear as), distinct from [`super::user::PresenceStatus`]
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
    #[must_use]
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Away => "away",
            Self::DoNotDisturb => "dnd",
            Self::AppearOffline => "appear-offline",
            Self::Offline => "offline",
        }
    }

    /// Small indicator emoji shown on the account icon bottom-left badge.
    #[must_use]
    pub const fn emoji(self) -> &'static str {
        match self {
            Self::Online => "●",
            Self::Away => "◑",
            Self::DoNotDisturb => "⊗",
            Self::AppearOffline | Self::Offline => "○",
        }
    }

    /// Display name for UI labels.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
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
