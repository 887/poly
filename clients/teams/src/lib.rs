//! # poly-teams
//!
//! Microsoft Teams messenger client for Poly.
//!
//! Implements [`poly_client::IsBackend`] using Microsoft Graph API.
//! Uses Bearer token auth against `/v1.0/` endpoints.
//!
//! ## Build Modes
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "teams";

#[cfg(feature = "native")]
pub mod auth;
#[cfg(feature = "native")]
mod http;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
pub mod types;
/// Teams voice stub — see [`voice::TeamsVoiceClient`] and Phase I of
/// `docs/plans/plan-voice-video-calls.md`.
pub mod voice;

/// WIT bindings for the WASM plugin (WASI targets only).
#[cfg(target_os = "wasi")]
mod wit_bindings;
/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

// ── Trait impl modules (D.2 — split along capability-trait lines) ────────────
#[cfg(feature = "native")]
mod is_backend;
#[cfg(feature = "native")]
mod moderation;
#[cfg(feature = "native")]
mod social_graph;
#[cfg(feature = "native")]
mod dms_groups;
#[cfg(feature = "native")]
mod settings;
#[cfg(feature = "native")]
mod view_descriptor;
#[cfg(feature = "native")]
mod context_action;

/// Return Fluent translations for the given locale.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

#[cfg(feature = "native")]
use http::TeamsHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::collections::HashSet;
#[cfg(feature = "native")]
use std::sync::Mutex;

/// Microsoft Teams client.
///
/// Uses Microsoft Graph API v1.0. Teams (guilds) map to poly `Server`s;
/// Graph channels map to poly `Channel`s. Token auth via Bearer header.
///
/// ## Channel ID format
///
/// Graph requires both team_id and channel_id to address messages.
/// We encode these as `"<team_id>/<channel_id>"` in `Channel.server_id` and
/// `Channel.id` respectively, and decode on use.
///
/// ## Menu state (F10)
///
/// State-aware menus branch on these in-memory sets (F9 covers KV persistence).
/// `Mutex` gives interior mutability behind `&self` — the `ClientBackend` trait
/// does not take `&mut self`.
#[cfg(feature = "native")]
pub struct TeamsClient {
    pub(crate) http: TeamsHttpClient,
    pub(crate) account_id: Option<String>,
    pub(crate) account_display_name: Option<String>,
    /// Pack C P18 — in-memory settings storage stub.
    pub(crate) settings_storage: SettingsStorageCell,
    // ── F10 menu state ──────────────────────────────────────────────────────
    pub(crate) hidden_channels: Mutex<HashSet<String>>,
    pub(crate) pinned_channels: Mutex<HashSet<String>>,
    pub(crate) muted_channels: Mutex<HashSet<String>>,
    pub(crate) muted_teams: Mutex<HashSet<String>>,
    pub(crate) saved_messages: Mutex<HashSet<String>>,
    pub(crate) hidden_dms: Mutex<HashSet<String>>,
    pub(crate) muted_dms: Mutex<HashSet<String>>,
    /// Stored version override (None = use http::DEFAULT_CLIENT_VERSION).
    pub(crate) version_override: Mutex<Option<String>>,
}

#[cfg(feature = "native")]
impl TeamsClient {
    #[must_use]
    pub fn new() -> Self {
        Self::with_base_url("https://graph.microsoft.com".to_string())
    }

    #[must_use]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: TeamsHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            hidden_channels: Mutex::new(HashSet::new()),
            pinned_channels: Mutex::new(HashSet::new()),
            muted_channels: Mutex::new(HashSet::new()),
            muted_teams: Mutex::new(HashSet::new()),
            saved_messages: Mutex::new(HashSet::new()),
            hidden_dms: Mutex::new(HashSet::new()),
            muted_dms: Mutex::new(HashSet::new()),
            version_override: Mutex::new(None),
        }
    }

    pub(crate) fn account_id(&self) -> String {
        self.account_id.clone().unwrap_or_default()
    }

    pub(crate) fn account_display_name(&self) -> String {
        self.account_display_name.clone().unwrap_or_default()
    }

    pub(crate) fn graph_message_to_poly(&self, m: types::GraphMessage) -> Message {
        let author = if let Some(from) = m.from {
            if let Some(u) = from.user {
                User {
                    id: u.id,
                    display_name: u.display_name.unwrap_or_default(),
                    avatar_url: None,
                    presence: PresenceStatus::Online,
                    backend: BackendType::from(crate::SLUG),
                }
            } else {
                self.unknown_user()
            }
        } else {
            self.unknown_user()
        };
        let timestamp = chrono::DateTime::parse_from_rfc3339(&m.created_date_time).map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc));
        Message {
            id: m.id,
            author,
            content: MessageContent::Text(m.body.content),
            timestamp,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            edited: false,
            thread: None,
            preview_image_url: None,
        }
    }

    /// Edit a channel message. Not yet on the `ClientBackend` trait — expose
    /// so test harnesses and future trait work can drive it.
    pub async fn edit_message(&self, channel_id: &str, message_id: &str, content: &str) -> ClientResult<Message> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams edit_message requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        let m = self.http.edit_channel_message(team_id, ch_id, message_id, content).await?;
        Ok(self.graph_message_to_poly(m))
    }

    /// Soft-delete a channel message.
    pub async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams delete_message requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http.delete_channel_message(team_id, ch_id, message_id).await
    }

    /// Add a reaction to a channel message.
    pub async fn react(&self, channel_id: &str, message_id: &str, reaction_type: &str) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams react requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http.set_channel_reaction(team_id, ch_id, message_id, reaction_type).await
    }

    /// Remove a reaction from a channel message.
    pub async fn unreact(&self, channel_id: &str, message_id: &str, reaction_type: &str) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams unreact requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http.unset_channel_reaction(team_id, ch_id, message_id, reaction_type).await
    }

    pub(crate) fn unknown_user(&self) -> User {
        User {
            id: String::new(),
            display_name: "Unknown".to_string(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: BackendType::from(crate::SLUG),
        }
    }
}

#[cfg(feature = "native")]
impl Default for TeamsClient {
    fn default() -> Self { Self::new() }
}
