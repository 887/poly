//! # poly-lemmy
//!
//! Lemmy federated forum client for Poly.
//!
//! Implements [`poly_client::IsBackend`] using the Lemmy REST API v3.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.
//!
//! ## Module layout
//!
//! `LemmyClient` implements 11 `poly_client` traits. Each `impl Trait for
//! LemmyClient` lives in its own sibling module (split B.1, SOLID Single
//! Responsibility). The inherent impl + struct definition stay here;
//! the API layer is rooted at [`api`].

#![allow(clippy::if_same_then_else)]

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "lemmy";

#[cfg(feature = "native")]
pub(crate) mod api;

#[cfg(feature = "native")]
pub mod signup;

/// WIT bindings for the WASM plugin (WASI targets only).
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

// ── Trait impls split out for Single Responsibility (B.1) ─────────────────
#[cfg(feature = "native")]
mod context_action;
#[cfg(feature = "native")]
mod discover;
#[cfg(feature = "native")]
mod dms_groups;
#[cfg(feature = "native")]
mod forum;
#[cfg(feature = "native")]
mod is_backend;
#[cfg(feature = "native")]
mod messaging;
#[cfg(feature = "native")]
mod moderation;
#[cfg(feature = "native")]
mod server_admin;
#[cfg(feature = "native")]
mod settings;
#[cfg(feature = "native")]
mod social_graph;
#[cfg(feature = "native")]
mod view_descriptor;

#[cfg(feature = "native")]
use api::LemmyHttpClient;
#[cfg(feature = "native")]
use poly_client::{ClientError, ClientResult, SettingsScope, SettingsStorageCell};

// ── NotSupported string constants — avoids repeated heap allocations ──────────
// Each constant covers one logical "category" of unsupported capability.
// Use these wherever the same error message would otherwise be duplicated.
// `pub(crate)` because sibling trait-impl modules consume them.
#[cfg(feature = "native")]
pub(crate) const GROUP_DM_UNSUPPORTED: &str = "Lemmy has no group DMs";
#[cfg(feature = "native")]
pub(crate) const CONVO_MUTE_UNSUPPORTED: &str = "Lemmy has no conversation mute API";

/// Return the raw FTL translation source for the Lemmy client plugin.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Lemmy federated forum client.
#[cfg(feature = "native")]
pub struct LemmyClient {
    pub(crate) http: LemmyHttpClient,
    /// In-memory settings storage (per-account KV).
    pub(crate) settings_storage: SettingsStorageCell,
    /// Stored version override (None = use api::DEFAULT_CLIENT_VERSION).
    pub(crate) version_override: std::sync::Mutex<Option<String>>,
}

#[cfg(feature = "native")]
impl LemmyClient {
    /// Create a new Lemmy client pointed at `base_url` (e.g. `https://lemmy.ml`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: LemmyHttpClient::new(base_url),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// The configured instance base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }

    /// Stable instance identifier derived from the base URL host.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.http
            .base_url()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_string()
    }

    /// Return the currently stored session JWT, if any.
    #[must_use]
    pub fn session_jwt(&self) -> Option<String> {
        self.http.session().map(|s| s.jwt)
    }

    /// Read the `render-previews` mechanism state from in-memory storage.
    ///
    /// Defaults to `true` (previews on) when the user has never toggled it.
    pub(crate) fn render_previews_enabled(&self) -> bool {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "render-previews")
            .is_none_or(|v| v != "false")
    }

    /// Return the currently stored user_id, if authenticated.
    pub(crate) fn current_user_id(&self) -> Option<i64> {
        self.http.session().map(|s| s.user_id)
    }

    /// Return (account_id, account_display_name) or an AuthFailed error.
    ///
    /// The `account_id` MUST match `session.id` produced during `authenticate`
    /// (`"lemmy-session-{user_id}"`). Using a different prefix such as
    /// `"lemmy-user-{user_id}"` causes `Server.account_id` to diverge from the
    /// session key stored in `ClientManager`, making the account-server-bar
    /// filter find zero servers and routing the user to the empty Notifications
    /// page instead of the first community.
    pub(crate) fn current_account_metadata(&self) -> ClientResult<(String, String)> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy client is not authenticated".to_string())
        })?;
        let account_id = format!("lemmy-session-{}", session.user_id);
        let display = session.user_display_name;
        Ok((account_id, display))
    }

    /// Extract a community_id integer from a `lemmy-community-{id}` server ID string.
    pub(crate) fn parse_community_id(server_id: &str) -> ClientResult<i64> {
        server_id
            .strip_prefix("lemmy-community-")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| {
                ClientError::NotFound(format!("invalid Lemmy server id: {server_id}"))
            })
    }

    /// Extract a post_id integer from a `lemmy-feed-{community_id}` channel ID.
    pub(crate) fn parse_feed_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-feed-")
            .and_then(|s| s.parse::<i64>().ok())
    }

    /// Extract a community_id integer from a `lemmy-comments-{community_id}` channel ID.
    /// Phase D — synthetic channel for the community-level recent-comments feed.
    pub(crate) fn parse_comments_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-comments-")
            .and_then(|s| s.parse::<i64>().ok())
    }

    /// Extract a post_id integer from a `lemmy-post-{id}` channel/message ID.
    pub(crate) fn parse_post_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-post-")
            .and_then(|s| s.parse::<i64>().ok())
    }

    /// Parse a Lemmy person integer ID from either a bare integer string
    /// or a `lemmy-user-{id}` prefixed string.
    pub(crate) fn parse_person_id(member_id: &str) -> ClientResult<i64> {
        // Accept both "lemmy-user-42" and bare "42".
        let raw = member_id
            .strip_prefix("lemmy-user-")
            .unwrap_or(member_id);
        raw.parse::<i64>().map_err(|_err| {
            ClientError::NotFound(format!("invalid Lemmy member id: {member_id}"))
        })
    }
}
