//! Platform-transparent storage abstraction for Poly.
//!
//! Provides a unified key/value storage interface with typed convenience
//! methods for all persisted data (settings, identity, account tokens, etc.).
//!
//! ## Backend selection
//!
//! | Target | Backend | Notes |
//! |---|---|---|
//! | Native (Linux/macOS/Windows) | SurrealDB (SurrealKV) | Same as main app |
//! | WebAssembly | `localStorage` via `gloo-storage` | Persistent across page reloads |
//!
//! ## Usage
//!
//! ```rust,ignore
//! // At app startup (in App component):
//! let storage = Storage::init().await?;
//!
//! // Read typed settings:
//! let settings = storage.get_app_settings().await?;
//!
//! // Write typed settings:
//! storage.set_app_settings(&AppSettings {
//!     setup_complete: true,
//!     account_id: "abc123".into(),
//!     locale: "en".into(),
//! }).await?;
//! ```
//!
//! DECISION(DX-STORAGE-1): Unified trait pattern — same call sites work whether
//! compiled to native or WASM. No feature flags required at call sites.

use serde::{Deserialize, Serialize};

const fn default_server_member_list_open() -> bool {
    true
}

/// Demo is active by default so new users get demo data on first launch.
const fn default_demo_active() -> bool {
    true
}

const fn default_gif_provider() -> GifProviderKind {
    GifProviderKind::Klippy
}

// ── Platform backends ─────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
use native::StorageInner;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
use web::StorageInner;

// ── Error type ────────────────────────────────────────────────────────────────

/// Storage operation error.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Backend-specific error (SurrealDB or Web API error).
    #[error("storage backend error: {0}")]
    Backend(String),

    /// (De)serialization error.
    #[error("serialization error: {0}")]
    Serde(String),
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e.to_string())
    }
}

/// Supported external GIF search providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GifProviderKind {
    /// Klippy / Tenor-like provider configured by API key.
    #[default]
    Klippy,
    /// Giphy API integration.
    Giphy,
    /// Imgur gallery/search integration.
    Imgur,
}

impl GifProviderKind {
    /// Stable lowercase value for storage/UI selects.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Klippy => "klippy",
            Self::Giphy => "giphy",
            Self::Imgur => "imgur",
        }
    }

    /// Parse a stored/provider select value (slug form).
    pub fn from_slug(value: &str) -> Option<Self> {
        match value {
            "klippy" => Some(Self::Klippy),
            "giphy" => Some(Self::Giphy),
            "imgur" => Some(Self::Imgur),
            _ => None,
        }
    }
}

/// User-configurable state for one GIF provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GifProviderConfig {
    /// Whether the provider is enabled in the UI.
    pub enabled: bool,
    /// API key/token entered by the user.
    #[serde(default)]
    pub api_key: String,
}

/// App-level media integration settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaProviderSettings {
    /// Active provider selected in the GIF picker dropdown.
    #[serde(default = "default_gif_provider")]
    pub active_gif_provider: GifProviderKind,
    /// Klippy integration settings.
    #[serde(default)]
    pub klippy: GifProviderConfig,
    /// Giphy integration settings.
    #[serde(default)]
    pub giphy: GifProviderConfig,
    /// Imgur integration settings.
    #[serde(default)]
    pub imgur: GifProviderConfig,
}

// ── Typed data models ─────────────────────────────────────────────────────────

/// Top-level application settings.
///
/// Persisted under the key `"app_settings"`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    /// Whether the setup wizard has been completed.
    pub setup_complete: bool,
    /// BIP39 account ID (hex-encoded public key).
    pub account_id: String,
    /// Selected locale (e.g. `"en"`, `"de"`).
    pub locale: String,
    /// Selected theme preset (e.g. `"neutral-dark"`, `"purple"`, `"red"`).
    pub theme: String,
    /// Whether the demo client was active when the app was last closed.
    ///
    /// When `true`, the demo client is automatically restored on next launch
    /// so the user doesn't lose their demo session across restarts. Defaults
    /// to `true` so new users get demo data on first launch.
    #[serde(default = "default_demo_active")]
    pub demo_active: bool,
    /// Server IDs pinned to the Favorites Bar (Bar 1), in display order.
    ///
    /// Persisted so favorites survive page reloads, app restarts, and
    /// temporary offline periods. Defaults to empty for new installs.
    /// Restored into `ChatData.favorited_server_ids` at startup before
    /// the event stream populates server data.
    #[serde(default)]
    pub favorited_server_ids: Vec<String>,
    /// User-defined server icon URL overrides, keyed by server ID.
    ///
    /// Applied on top of the backend-reported `icon_url` after each
    /// `load_server_data` / `restore_server_channel` call.
    /// Supports backends that don't expose a programmatic icon API
    /// (e.g. Matrix, Teams) as well as user customisation for all backends.
    /// Uses `#[serde(default)]` for backwards compatibility with existing records.
    #[serde(default)]
    pub server_icon_overrides: std::collections::HashMap<String, String>,
    /// User-defined server banner URL overrides, keyed by server ID.
    ///
    /// Same semantics as `server_icon_overrides` but for the wide banner
    /// shown at the top of the channel list.
    #[serde(default)]
    pub server_banner_overrides: std::collections::HashMap<String, String>,
    /// Whether the integrated server member list is open.
    #[serde(default = "default_server_member_list_open")]
    pub server_member_list_open: bool,
    /// Whether the integrated group-DM member list is open.
    #[serde(default)]
    pub dm_member_list_open: bool,
    /// External media provider configuration (GIF search, etc.).
    #[serde(default)]
    pub media: MediaProviderSettings,
    /// List of native backend type slugs that have been disabled by the user.
    ///
    /// E.g. `["discord", "teams"]` means those compiled-in backends are toggled off.
    /// Absent from this list = enabled. Uses slugs from `BackendType::slug()`.
    #[serde(default)]
    pub disabled_native_backends: Vec<String>,
    /// WASM plugin entries added by the user (from URLs or local files).
    ///
    /// Each entry records the URL and an optional display name override.
    /// The app appends its WIT version to the URL before fetching so the
    /// remote can serve the correct plugin binary.
    #[serde(default)]
    pub wasm_plugins: Vec<WasmPluginEntry>,
    /// Whether the Poly Server backend should use WebSocket/JSON-RPC for
    /// real-time event delivery.
    ///
    /// When `true` (default), the backend opens a persistent WebSocket
    /// connection (`ws://host/ws?token=<JWT>`) to receive pushes immediately.
    /// When `false`, the backend falls back to periodic HTTP polling.
    /// Changing this setting requires reconnecting or restarting the app.
    #[serde(default = "default_true")]
    pub poly_use_websocket: bool,
}

/// Minimal server metadata cached for offline display.
///
/// Persisted under `"offline_server_cache"` as a JSON array. Updated every
/// time a poly-server account successfully connects so that on next startup
/// (with the server offline) we can still render server icons in Bar 1 and
/// show the account's servers as "offline" rather than making them disappear.
///
/// Stored separately from [`FavoriteItem`] because favorites track *order*
/// while this cache tracks *metadata* (name, icon, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineServerRecord {
    /// Backend-specific server ID.
    pub id: String,
    /// Server display name.
    pub name: String,
    /// URL to the server icon/avatar.
    #[serde(default)]
    pub icon_url: Option<String>,
    /// URL to the server banner image.
    #[serde(default)]
    pub banner_url: Option<String>,
    /// Backend slug, e.g. `"poly"`.
    pub backend: String,
    /// Account ID this server belongs to.
    pub account_id: String,
    /// Display name of the owning account (for account header display).
    pub account_display_name: String,
}

/// A user-added WASM plugin, loaded from a URL.
///
/// Persisted in [`AppSettings::wasm_plugins`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmPluginEntry {
    /// The base URL to fetch the plugin from.
    /// The app appends `?wit=<version>` before making the request.
    pub url: String,
    /// Optional user-defined name override. Falls back to the URL hostname.
    #[serde(default)]
    pub name: Option<String>,
    /// Whether this plugin is currently enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Global notification preferences — device-level settings only.
///
/// Persisted under the key `"notification_settings"` (for backwards compat).
/// Per-account preferences are stored separately in [`AccountNotificationSettings`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    /// Desktop notification permission granted & enabled.
    ///
    /// This is device-level — it requires OS/browser permission and applies
    /// regardless of which account is active. Per-account event preferences
    /// live in [`AccountNotificationSettings`].
    pub desktop_enabled: bool,
    /// Notify when people I know start streaming.
    pub notify_streams: bool,
    /// Notify when friends join voice channels.
    pub notify_friends_voice: bool,
    /// Notify when someone reacts to my messages.
    pub notify_reactions: bool,
    /// Play sound on new message.
    pub sound_new_message: bool,
    /// Play sound on DM.
    pub sound_dm: bool,
    /// Play sound on incoming ring.
    pub sound_ring: bool,
    /// Show unread badge.
    pub badge_unread: bool,
}

/// Per-account notification preferences.
///
/// Keyed by `account_id` — stored under `"notif:{account_id}"`.
/// Does NOT include `desktop_enabled` (device-level, kept in [`NotificationSettings`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountNotificationSettings {
    /// Notify when people I know start streaming.
    pub notify_streams: bool,
    /// Notify when friends join voice channels.
    pub notify_friends_voice: bool,
    /// Notify when someone reacts to my messages.
    pub notify_reactions: bool,
    /// Play sound on new messages.
    pub sound_new_message: bool,
    /// Play sound on DMs.
    pub sound_dm: bool,
    /// Play sound on incoming ring.
    pub sound_ring: bool,
    /// Show unread badge.
    pub badge_unread: bool,
}

impl Default for AccountNotificationSettings {
    fn default() -> Self {
        Self {
            notify_streams: true,
            notify_friends_voice: true,
            notify_reactions: true,
            sound_new_message: true,
            sound_dm: true,
            sound_ring: true,
            badge_unread: true,
        }
    }
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            desktop_enabled: false,
            notify_streams: true,
            notify_friends_voice: true,
            notify_reactions: true,
            sound_new_message: true,
            sound_dm: true,
            sound_ring: true,
            badge_unread: true,
        }
    }
}

/// Voice & video preferences.
///
/// Persisted under the key `"voice_settings"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSettings {
    /// Input volume (0–100).
    pub input_volume: u32,
    /// Output volume (0–100).
    pub output_volume: u32,
    /// Input mode: `"vad"` (voice activity) or `"ptt"` (push-to-talk).
    pub input_mode: String,
    /// Noise suppression: `"off"`, `"standard"`, or `"high"`.
    pub noise_suppression: String,
    /// Echo cancellation enabled.
    pub echo_cancellation: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            input_volume: 80,
            output_volume: 80,
            input_mode: "vad".to_string(),
            noise_suppression: "standard".to_string(),
            echo_cancellation: true,
        }
    }
}

/// A stored messenger account credential/token.
///
/// Persisted under the key `"account_tokens"` as a JSON array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountToken {
    /// Backend identifier (`"poly"`, `"stoat"`, `"matrix"`, `"discord"`, …).
    pub backend: String,
    /// Account ID within that backend.
    pub account_id: String,
    /// Auth token / session token.
    pub token: String,
    /// Display name.
    pub display_name: String,
    /// Full backend base URL (with protocol) for reconnection after restart.
    ///
    /// Required for backends where the URL is user-configurable (e.g. poly
    /// server: `"http://127.0.0.1:7080"`).  `None` for built-in services.
    #[serde(default)]
    pub instance_id: Option<String>,
}

/// Stored backup server configuration.
///
/// Persisted under the key `"backup_servers"` as a JSON array.
/// Identified by `url` — upsert replaces the entry with the same URL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupServerRecord {
    /// Base URL (e.g. `"http://backup.example.com:8080"`).
    pub url: String,
    /// User-chosen friendly name (e.g. `"My Home Server"`).
    pub label: String,
    /// Whether this server is active — disabled servers are skipped during sync.
    pub enabled: bool,
    /// Highest sequence number successfully synced.
    pub last_sequence: u64,
    /// Stored session token (raw, used as Bearer token).
    pub token: Option<String>,
    /// ISO-8601 UTC expiry timestamp from the auth response.
    pub token_expires_at: Option<String>,
    /// ISO-8601 UTC timestamp of the last successful sync.
    pub last_synced_at: Option<String>,
}

/// The kind of favorited item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FavoriteKind {
    /// A direct message / friend.
    DirectMessage,
    /// A group DM.
    Group,
    /// A server / guild.
    Server,
    /// A channel within a server.
    Channel,
}

/// A favorited item (pinned server, friend, group DM, or channel).
///
/// Persisted under the key `"favorites"`.
///
/// TODO(phase-2.4.3.8): Favorites storage implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteItem {
    /// Unique entity ID (backend-specific).
    pub id: String,
    /// Backend identifier (`"stoat"`, `"matrix"`, `"poly-server"`, …).
    pub backend: String,
    /// Account ID this favorite belongs to.
    pub account_id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Optional icon / avatar URL.
    pub icon_url: Option<String>,
    /// Kind of favorited item.
    pub kind: FavoriteKind,
}

// ── Storage handle ────────────────────────────────────────────────────────────

/// Platform-transparent storage handle.
///
/// Cheap to clone — backed by `Arc` internally on native,
/// by a zero-size-type on WASM (localStorage is a browser global).
#[derive(Clone)]
pub struct Storage(StorageInner);

impl Storage {
    /// Initialize the storage backend.
    ///
    /// * **Native**: opens (or creates) the SurrealKV database in the platform
    ///   data directory (`~/.local/share/poly` on Linux etc.).
    /// * **WASM**: no-op — `localStorage` is always available.
    pub async fn init() -> Result<Self, StorageError> {
        Ok(Self(StorageInner::init().await?))
    }

    // ── Raw KV access ─────────────────────────────────────────────────────────

    /// Get a value by key. Returns `None` if the key is not present.
    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        self.0.get(key).await
    }

    /// Set a value by key (upsert semantics).
    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        self.0.set(key, value).await
    }

    /// Delete a key. No-op if the key does not exist.
    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.0.delete(key).await
    }

    /// Reset user-facing app data while preserving device identity.
    ///
    /// This clears setup completion, account tokens, backup server configs,
    /// favorites, and persisted theme settings.
    pub async fn reset_user_data(&self) -> Result<(), StorageError> {
        for key in [
            "app_settings",
            "account_tokens",
            "backup_servers",
            "favorites",
            "theme_config",
        ] {
            self.delete(key).await?;
        }
        Ok(())
    }

    /// Irreversibly clear all app state from persistent storage.
    ///
    /// On native this wipes the whole SurrealKV table. On web this clears all
    /// browser localStorage keys used by the app.
    pub async fn nuke_all_data(&self) -> Result<(), StorageError> {
        self.0.clear_all().await
    }

    // ── Typed access — AppSettings ────────────────────────────────────────────

    /// Read application settings, returning [`AppSettings::default`] if not yet set.
    pub async fn get_app_settings(&self) -> Result<AppSettings, StorageError> {
        Ok(self
            .get("app_settings")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist application settings.
    pub async fn set_app_settings(&self, settings: &AppSettings) -> Result<(), StorageError> {
        self.set("app_settings", serde_json::to_value(settings)?)
            .await
    }

    // ── Typed access — NotificationSettings ───────────────────────────────────

    /// Read the global (device-level) notification settings, returning defaults if not yet set.
    pub async fn get_notification_settings(&self) -> Result<NotificationSettings, StorageError> {
        Ok(self
            .get("notification_settings")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist the global (device-level) notification settings.
    pub async fn set_notification_settings(
        &self,
        settings: &NotificationSettings,
    ) -> Result<(), StorageError> {
        self.set("notification_settings", serde_json::to_value(settings)?)
            .await
    }

    // ── Typed access — AccountNotificationSettings ────────────────────────────

    /// Read per-account notification settings for `account_id`.
    ///
    /// Storage key: `"notif:{account_id}"`. Returns defaults if not yet saved.
    pub async fn get_account_notification_settings(
        &self,
        account_id: &str,
    ) -> Result<AccountNotificationSettings, StorageError> {
        let key = format!("notif:{account_id}");
        Ok(self
            .get(&key)
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist per-account notification settings for `account_id`.
    ///
    /// Storage key: `"notif:{account_id}"`.
    pub async fn set_account_notification_settings(
        &self,
        account_id: &str,
        settings: &AccountNotificationSettings,
    ) -> Result<(), StorageError> {
        let key = format!("notif:{account_id}");
        self.set(&key, serde_json::to_value(settings)?).await
    }

    // ── Typed access — VoiceSettings ──────────────────────────────────────────

    /// Read voice/video settings, returning defaults if not yet set.
    pub async fn get_voice_settings(&self) -> Result<VoiceSettings, StorageError> {
        Ok(self
            .get("voice_settings")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist voice/video settings.
    pub async fn set_voice_settings(&self, settings: &VoiceSettings) -> Result<(), StorageError> {
        self.set("voice_settings", serde_json::to_value(settings)?)
            .await
    }

    // ── Typed access — AccountToken ───────────────────────────────────────────

    /// List all stored account tokens.
    pub async fn get_account_tokens(&self) -> Result<Vec<AccountToken>, StorageError> {
        Ok(self
            .get("account_tokens")
            .await?
            .and_then(|v| serde_json::from_value::<Vec<AccountToken>>(v).ok())
            .unwrap_or_default())
    }

    /// Persist (upsert) an account token. Identified by `backend + account_id`.
    pub async fn upsert_account_token(&self, token: &AccountToken) -> Result<(), StorageError> {
        let mut tokens = self.get_account_tokens().await?;
        tokens.retain(|t| !(t.backend == token.backend && t.account_id == token.account_id));
        tokens.push(token.clone());
        self.set("account_tokens", serde_json::to_value(&tokens)?)
            .await
    }

    /// Remove an account token.
    pub async fn remove_account_token(
        &self,
        backend: &str,
        account_id: &str,
    ) -> Result<(), StorageError> {
        let mut tokens = self.get_account_tokens().await?;
        tokens.retain(|t| !(t.backend == backend && t.account_id == account_id));
        self.set("account_tokens", serde_json::to_value(&tokens)?)
            .await
    }

    // ── Typed access — FavoriteItem ───────────────────────────────────────────

    /// List all stored favorites.
    ///
    /// TODO(phase-2.4.3.8): Favorites storage.
    pub async fn get_favorites(&self) -> Result<Vec<FavoriteItem>, StorageError> {
        Ok(self
            .get("favorites")
            .await?
            .and_then(|v| serde_json::from_value::<Vec<FavoriteItem>>(v).ok())
            .unwrap_or_default())
    }

    /// Add or update a favorite (identified by `backend + id`).
    pub async fn upsert_favorite(&self, item: &FavoriteItem) -> Result<(), StorageError> {
        let mut favorites = self.get_favorites().await?;
        favorites.retain(|f| !(f.backend == item.backend && f.id == item.id));
        favorites.push(item.clone());
        self.set("favorites", serde_json::to_value(&favorites)?)
            .await
    }

    /// Remove a favorite by backend + entity id.
    pub async fn remove_favorite(&self, backend: &str, id: &str) -> Result<(), StorageError> {
        let mut favorites = self.get_favorites().await?;
        favorites.retain(|f| !(f.backend == backend && f.id == id));
        self.set("favorites", serde_json::to_value(&favorites)?)
            .await
    }

    // ── Typed access — OfflineServerRecord ───────────────────────────────────

    /// Read the entire offline server metadata cache.
    ///
    /// Returns an empty list if the key does not exist yet.
    pub async fn get_offline_server_cache(&self) -> Result<Vec<OfflineServerRecord>, StorageError> {
        Ok(self
            .get("offline_server_cache")
            .await?
            .and_then(|v| serde_json::from_value::<Vec<OfflineServerRecord>>(v).ok())
            .unwrap_or_default())
    }

    /// Upsert `new_records` into the offline server cache.
    ///
    /// Existing records with the same `id` are replaced; new ones are appended.
    /// Other accounts' records are preserved.
    pub async fn upsert_offline_server_cache(
        &self,
        new_records: &[OfflineServerRecord],
    ) -> Result<(), StorageError> {
        let mut existing = self.get_offline_server_cache().await?;
        for record in new_records {
            existing.retain(|r| r.id != record.id);
            existing.push(record.clone());
        }
        self.set("offline_server_cache", serde_json::to_value(&existing)?)
            .await
    }

    /// Remove all cached server records for a given account.
    ///
    /// Called when the user disables / removes a backend account so stale
    /// records do not linger in the cache.
    pub async fn remove_offline_server_cache_for_account(
        &self,
        account_id: &str,
    ) -> Result<(), StorageError> {
        let mut existing = self.get_offline_server_cache().await?;
        existing.retain(|r| r.account_id != account_id);
        self.set("offline_server_cache", serde_json::to_value(&existing)?)
            .await
    }

    // ── Typed access — ThemeConfig ────────────────────────────────────────────

    /// Read the stored theme configuration.
    ///
    /// Returns [`crate::theme::ThemeConfig::default`] (neutral-dark) if not yet set.
    ///
    /// TODO(phase-2.4.3.9): Theme preferences storage.
    pub async fn get_theme_config(&self) -> Result<crate::theme::ThemeConfig, StorageError> {
        Ok(self
            .get("theme_config")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist the theme configuration.
    pub async fn set_theme_config(
        &self,
        config: &crate::theme::ThemeConfig,
    ) -> Result<(), StorageError> {
        self.set("theme_config", serde_json::to_value(config)?)
            .await
    }

    // ── Typed access — BackupServerRecord ─────────────────────────────────────

    /// List all stored backup server records.
    pub async fn get_backup_servers(&self) -> Result<Vec<BackupServerRecord>, StorageError> {
        Ok(self
            .get("backup_servers")
            .await?
            .and_then(|v| serde_json::from_value::<Vec<BackupServerRecord>>(v).ok())
            .unwrap_or_default())
    }

    /// Add or update a backup server record (keyed by `url`).
    pub async fn upsert_backup_server(
        &self,
        record: &BackupServerRecord,
    ) -> Result<(), StorageError> {
        let mut servers = self.get_backup_servers().await?;
        servers.retain(|s| s.url != record.url);
        servers.push(record.clone());
        self.set("backup_servers", serde_json::to_value(&servers)?)
            .await
    }

    /// Remove a backup server by URL. No-op if not found.
    pub async fn remove_backup_server(&self, url: &str) -> Result<(), StorageError> {
        let mut servers = self.get_backup_servers().await?;
        servers.retain(|s| s.url != url);
        self.set("backup_servers", serde_json::to_value(&servers)?)
            .await
    }

    // ── Typed access — AccountLastRoutes ─────────────────────────────────────

    /// Read the persisted per-account last-visited URL map.
    ///
    /// Storage key: `"account_last_routes"`. Returns empty map if not yet saved.
    pub async fn get_account_last_routes(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, StorageError> {
        Ok(self
            .get("account_last_routes")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist the per-account last-visited URL map.
    ///
    /// Storage key: `"account_last_routes"`.
    pub async fn set_account_last_routes(
        &self,
        routes: &std::collections::HashMap<String, String>,
    ) -> Result<(), StorageError> {
        self.set("account_last_routes", serde_json::to_value(routes)?)
            .await
    }

    // ── Typed access — Identity ───────────────────────────────────────────────

    /// Retrieve the raw Ed25519 private key bytes (32 bytes) from storage.
    ///
    /// Returns `None` if the identity has not been generated yet (pre-wizard).
    pub async fn get_identity_key(&self) -> Result<Option<[u8; 32]>, StorageError> {
        let raw = self.get("identity_key").await?;
        match raw {
            None => Ok(None),
            Some(v) => {
                let bytes: Vec<u8> = serde_json::from_value(v)?;
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    Ok(Some(arr))
                } else {
                    Err(StorageError::Serde(
                        "identity_key length mismatch — expected 32 bytes".into(),
                    ))
                }
            }
        }
    }

    /// Persist the raw Ed25519 private key bytes.
    pub async fn set_identity_key(&self, key_bytes: &[u8; 32]) -> Result<(), StorageError> {
        self.set("identity_key", serde_json::to_value(key_bytes.as_slice())?)
            .await
    }

    /// Delete the identity key from storage. This is irreversible unless you have
    /// the mnemonic backed up.
    pub async fn delete_identity_key(&self) -> Result<(), StorageError> {
        self.delete("identity_key").await
    }

    // ── Typed access — LastChannelPerServer ──────────────────────────────────

    /// Read the last-visited channel ID for a given server.
    ///
    /// Returns `None` if no channel has been visited for that server yet.
    ///
    /// Storage key: `"last_channel_per_server"` (a JSON `{ server_id → channel_id }` map).
    pub async fn get_last_channel_for_server(
        &self,
        server_id: &str,
    ) -> Result<Option<String>, StorageError> {
        Ok(self
            .get("last_channel_per_server")
            .await?
            .and_then(|v| {
                serde_json::from_value::<std::collections::HashMap<String, String>>(v).ok()
            })
            .and_then(|m| m.get(server_id).cloned()))
    }

    /// Persist the last-visited channel ID for a given server.
    ///
    /// Other server entries in the map are preserved.
    ///
    /// Storage key: `"last_channel_per_server"`.
    pub async fn set_last_channel_for_server(
        &self,
        server_id: &str,
        channel_id: &str,
    ) -> Result<(), StorageError> {
        let mut map: std::collections::HashMap<String, String> = self
            .get("last_channel_per_server")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        map.insert(server_id.to_string(), channel_id.to_string());
        self.set("last_channel_per_server", serde_json::to_value(&map)?)
            .await
    }

    // ── Migrations ────────────────────────────────────────────────────────────

    /// Current storage schema version.
    const CURRENT_VERSION: u64 = 1;

    /// Run any pending storage schema migrations.
    ///
    /// Call once at startup, after [`Storage::init`], before reading any data.
    ///
    /// Each migration step is idempotent — safe to re-run after a crash.
    ///
    /// TODO(phase-2.4.3.10): Migration system.
    pub async fn run_migrations(&self) -> Result<(), StorageError> {
        let version = self
            .get("storage_version")
            .await?
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        tracing::info!(
            "Storage schema: current v{version}, target v{}",
            Self::CURRENT_VERSION
        );

        // v0 → v1: initial schema. Nothing to migrate; just stamp the version.
        if version < 1 {
            tracing::info!("Applying storage migration: v0 → v1 (initial stamp)");
            self.set("storage_version", serde_json::json!(Self::CURRENT_VERSION))
                .await?;
        }

        Ok(())
    }
}
