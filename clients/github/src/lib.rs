//! # poly-github
//!
//! GitHub / GitHub Enterprise client for Poly.
//!
//! Uses the user's `gh` CLI as transport — no token extraction, no
//! direct HTTP. The CLI handles auth, rate limiting, and pagination.
//!
//! GitHub repos appear as Poly servers (filtered to owner + collaborator
//! repos with activity in the last two years). Each repo exposes:
//!
//! - an **issues** Forum channel
//! - a **pull-requests** Forum channel
//! - a **code** [`ChannelType::Code`] channel for the file/code explorer
//!
//! Code search is intentionally external — clients should open
//! `https://{instance}/{owner}/{repo}/search?type=code&q=…` for that.
//!
//! ## Native vs WASM
//!
//! On native targets the [`api`] module spawns the user's `gh` CLI directly
//! via [`tokio::process::Command`]. On wasm32 (the dioxus web build that runs
//! inside the Wry / Electron shells) the same module instead POSTs to a
//! localhost subprocess bridge exposed by the native shell at
//! `http://127.0.0.1:9223/gh`. The shell forwards each call to its own
//! `gh` binary and pipes stdout/stderr/exit_code back to the WASM frontend,
//! so the rest of the crate is target-agnostic.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "github";

mod api;
mod forum;
mod impl_code_repo;
mod impl_context_action;
mod impl_dms_and_groups;
mod impl_is_backend;
mod impl_moderation;
mod impl_settings;
mod impl_social_graph;
mod impl_view_descriptor;
mod mapping;
pub mod signup;
mod types;

use poly_client::*;

pub use api::{GhCli, GhError, RepoPermissions};
pub use mapping::{BACKEND_SLUG, issue_thread_channel_id};

/// Number of years of `pushed_at` activity required for a repo to surface
/// in the server list. Two years matches the user's stated requirement.
const ACTIVITY_WINDOW_YEARS: i64 = 2;

// ── NotSupported message constants ───────────────────────────────────────────
// Centralised here so each unique message is written once.
const NS_NO_FRIEND_SYSTEM: &str = "GitHub has no friend system";
const NS_NO_GROUP_DMS: &str = "GitHub has no group DMs";
const NS_NO_DM_CONCEPT: &str = "GitHub has no DM concept";
const NS_NO_CONVERSATION_MUTE: &str = "GitHub has no conversation mute";
const NS_NO_BAN_CONCEPT: &str = "GitHub: no per-repo ban concept";
const NS_NO_TIMEOUT_CONCEPT: &str = "GitHub: no timeout concept";
const NS_NO_IGNORE_CONCEPT: &str = "GitHub has no ignore concept";

/// Return FTL translation source for the GitHub client plugin.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// GitHub / GitHub Enterprise client.
///
/// Each instance wraps one `gh` CLI configuration. Construct with
/// [`GitHubClient::dotcom`] for github.com or [`GitHubClient::enterprise`]
/// for a GHE hostname.
pub struct GitHubClient {
    pub(crate) cli: GhCli,
    pub(crate) session: Option<Session>,
    /// Cached repo list — refreshed on `get_servers`.
    pub(crate) repos: tokio::sync::Mutex<Vec<types::GhRepo>>,
    /// In-memory settings storage (persists for process lifetime).
    pub(crate) settings_storage: SettingsStorageCell,
    /// Stored version override (None = use DEFAULT_CLIENT_VERSION).
    ///
    /// Note: the gh CLI controls the wire-level User-Agent for all HTTP
    /// requests. This field records the override for `client_version()` to
    /// return; it does NOT propagate to the wire because `GhCli` owns the
    /// transport and does not expose a User-Agent override surface.
    pub(crate) version_override: std::sync::Mutex<Option<String>>,
}

impl GitHubClient {
    /// Wrap the user's gh CLI for github.com.
    #[must_use]
    pub fn dotcom() -> Self {
        Self {
            cli: GhCli::dotcom(),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Wrap the user's gh CLI for a GitHub Enterprise hostname.
    pub fn enterprise(hostname: impl Into<String>) -> Self {
        Self {
            cli: GhCli::enterprise(hostname),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Create a client using direct HTTP transport (for testing).
    pub fn with_http(base_url: impl Into<String>) -> Self {
        Self {
            cli: GhCli::with_http(base_url),
            session: None,
            repos: tokio::sync::Mutex::new(Vec::new()),
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Build an authenticated [`Session`] from a `gh` login.
    fn build_session(&self, login: &str) -> Session {
        let instance = self.cli.instance_id().to_string();
        Session {
            id: format!("gh-{}-{}", instance, login),
            user: User {
                id: login.to_string(),
                display_name: login.to_string(),
                avatar_url: Some(format!("https://github.com/{login}.png")),
                presence: PresenceStatus::Offline,
                backend: BackendType::from(BACKEND_SLUG),
            },
            token: String::new(), // gh CLI owns the token
            backend: BackendType::from(BACKEND_SLUG),
            icon_emoji: Some("🐙".to_string()),
            instance_id: instance.clone(),
            backend_url: Some(if instance == "github.com" {
                "https://github.com".to_string()
            } else {
                format!("https://{instance}")
            }),
        }
    }

    /// Look up the cached repo for a server ID and return `(owner, repo)`.
    /// Returns `None` if not found in cache.
    pub(crate) async fn resolve_owner_repo_from_server_id(
        &self,
        server_id: &str,
    ) -> Option<(String, String)> {
        let cache = self.repos.lock().await;
        cache.iter().find_map(|r| {
            if mapping::server_id_for_repo(r) == server_id {
                let (owner, repo) = mapping::split_full_name(&r.full_name);
                Some((owner, repo))
            } else {
                None
            }
        })
    }

    pub(crate) fn session_login(&self) -> &str {
        self.session
            .as_ref()
            .map_or("anonymous", |s| s.user.id.as_str())
    }

    pub(crate) fn session_id(&self) -> &str {
        self.session.as_ref().map_or("gh", |s| s.id.as_str())
    }

    pub(crate) fn convert_err(e: GhError) -> ClientError {
        match e {
            GhError::Spawn(msg) => ClientError::Internal(format!(
                "gh CLI not available: {msg} — install from https://cli.github.com"
            )),
            GhError::Exit { code: _, stderr } if stderr.contains("not authenticated") => {
                ClientError::AuthFailed(stderr)
            }
            GhError::Exit { code, stderr } => {
                ClientError::Network(format!("gh exited {code}: {stderr}"))
            }
            GhError::Parse(msg) => ClientError::Internal(format!("gh JSON parse: {msg}")),
        }
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::dotcom()
    }
}
