//! # poly-forgejo
//!
//! Forgejo / Gitea / Codeberg client for Poly.
//!
//! Implements [`poly_client::IsBackend`] using the Forgejo REST API v1
//! via direct HTTP through `poly_host_bridge::http::HttpClient`.
//!
//! Repos appear as Poly servers. Each repo exposes:
//! - an **issues** Forum channel
//! - a **pull-requests** Forum channel
//! - a **code** [`ChannelType::Code`] channel for the file/code explorer
//!
//! The backend is read-only — send_message returns NotSupported.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "forgejo";

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
mod channel_ids;
#[cfg(feature = "native")]
mod code_repo;
#[cfg(feature = "native")]
mod context_action;
#[cfg(feature = "native")]
mod dms_and_groups;
#[cfg(feature = "native")]
mod is_backend;
#[cfg(feature = "native")]
mod mapping;
#[cfg(feature = "native")]
mod moderation;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
mod settings;
#[cfg(feature = "native")]
mod social_graph;
#[cfg(feature = "native")]
mod types;
#[cfg(feature = "native")]
mod view_descriptor;

#[cfg(feature = "native")]
pub use api::ForgejoApi;
#[cfg(feature = "native")]
pub use mapping::{BACKEND_SLUG, issue_thread_channel_id, map_issue_to_viewrow};
#[cfg(feature = "native")]
pub use types::ForgejoIssue;

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use poly_common_forge::{decode_b64, kind_from_string, split_owner_repo};
#[cfg(feature = "native")]
use std::pin::Pin;

// ── Not-supported message constants ─────────────────────────────────────────
// One cfg gate for all constants; the 30+ NotSupported sites don't allocate
// unique string literals.
#[cfg(feature = "native")]
mod ns {
    pub(super) const DM: &str = "Forgejo has no DM concept";
    pub(super) const SAVED_MSG: &str = "Forgejo has no saved-messages concept";
    pub(super) const GROUP_DM: &str = "Forgejo has no group DMs";
    pub(super) const CONV_MUTE: &str = "Forgejo has no conversation mute";
    // Tier 2 (plan-trait-split-readable-vs-writable): FRIEND / USER_NOTE
    // / BLOCK / UNBLOCK / IGNORE / PRESENCE removed — forgejo no longer
    // implements those write methods; the read trait's shim returns
    // `NotSupported` automatically.
    // READ_ONLY_SEND removed (Phase D.11): same reasoning.
}

/// Return FTL translation source for the Forgejo client plugin.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Forgejo / Gitea / Codeberg client.
///
/// Construct with [`ForgejoClient::new`] for any instance or
/// [`ForgejoClient::codeberg`] as a shortcut for `https://codeberg.org`.
#[cfg(feature = "native")]
pub struct ForgejoClient {
    api: ForgejoApi,
    session: Option<Session>,
    /// Cached repo list — refreshed on `get_servers`.
    repos: tokio::sync::Mutex<Vec<types::ForgejoRepo>>,
    /// In-memory settings storage for this client instance.
    settings_storage: SettingsStorageCell,
    /// Stored version override (None = use api::DEFAULT_CLIENT_VERSION).
    version_override: std::sync::Mutex<Option<String>>,
}

#[cfg(feature = "native")]
impl ForgejoClient {
    /// Create a new client pointed at `instance_url`.
    #[must_use]
    pub fn new(instance_url: &str) -> Self {
        Self {
            api: ForgejoApi::new(instance_url),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Shortcut for `https://codeberg.org`.
    #[must_use]
    pub fn codeberg() -> Self {
        Self::new("https://codeberg.org")
    }

    fn session_id(&self) -> &str {
        self.session.as_ref().map_or("fj", |s| s.id.as_str())
    }

    fn session_login(&self) -> &str {
        self.session
            .as_ref()
            .map_or("anonymous", |s| s.user.id.as_str())
    }

}

#[cfg(feature = "native")]
impl Default for ForgejoClient {
    fn default() -> Self {
        Self::codeberg()
    }
}
