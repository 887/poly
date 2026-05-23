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

/// Return Fluent translations for the given locale.
#[must_use] 
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::Stream;
#[cfg(feature = "native")]
use http::TeamsHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::collections::HashSet;
#[cfg(feature = "native")]
use std::pin::Pin;
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
    http: TeamsHttpClient,
    account_id: Option<String>,
    account_display_name: Option<String>,
    /// Pack C P18 — in-memory settings storage stub.
    settings_storage: SettingsStorageCell,
    // ── F10 menu state ──────────────────────────────────────────────────────
    hidden_channels: Mutex<HashSet<String>>,
    pinned_channels: Mutex<HashSet<String>>,
    muted_channels: Mutex<HashSet<String>>,
    muted_teams: Mutex<HashSet<String>>,
    saved_messages: Mutex<HashSet<String>>,
    hidden_dms: Mutex<HashSet<String>>,
    muted_dms: Mutex<HashSet<String>>,
    /// Stored version override (None = use http::DEFAULT_CLIENT_VERSION).
    version_override: Mutex<Option<String>>,
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

    fn account_id(&self) -> String {
        self.account_id.clone().unwrap_or_default()
    }

    fn account_display_name(&self) -> String {
        self.account_display_name.clone().unwrap_or_default()
    }

    fn graph_message_to_poly(&self, m: types::GraphMessage) -> Message {
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

    fn unknown_user(&self) -> User {
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

/// Convert a `TeamsEvent` JSON payload (from `/test/events/poll`) to a
/// `ClientEvent`. Returns None for events we don't yet surface.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn teams_event_to_client(ev: &serde_json::Value) -> Option<ClientEvent> {
    let ty = ev.get("type")?.as_str()?;
    match ty {
        "MessageCreated" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let m = ev.get("message")?;
            let msg = poly_event_message_from_json(m)?;
            Some(ClientEvent::MessageReceived { channel_id: resource_id, message: msg })
        }
        "MessageUpdated" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let m = ev.get("message")?;
            let msg = poly_event_message_from_json(m)?;
            Some(ClientEvent::MessageEdited { channel_id: resource_id, message: msg })
        }
        "MessageDeleted" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let message_id = ev.get("messageId")?.as_str()?.to_string();
            Some(ClientEvent::MessageDeleted { channel_id: resource_id, message_id })
        }
        _ => None,
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn poly_event_message_from_json(m: &serde_json::Value) -> Option<Message> {
    let id = m.get("id")?.as_str()?.to_string();
    let content = m.get("body")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let timestamp = m.get("createdDateTime")
        .and_then(|t| t.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map_or_else(chrono::Utc::now, |dt| dt.with_timezone(&chrono::Utc));
    let author_id = m.get("from")
        .and_then(|f| f.get("user"))
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let author_name = m.get("from")
        .and_then(|f| f.get("user"))
        .and_then(|u| u.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let edited = m.get("lastModifiedDateTime").is_some_and(|v| !v.is_null());
    Some(Message {
        id,
        author: User {
            id: author_id,
            display_name: author_name,
            avatar_url: None,
            presence: PresenceStatus::Online,
            backend: BackendType::from(crate::SLUG),
        },
        content: MessageContent::Text(content),
        timestamp,
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited,
        thread: None,
        preview_image_url: None,
    })
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for TeamsClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            AuthCredentials::OAuth { token } => token,
            AuthCredentials::EmailPassword { email, password } => {
                self.http.login(&email, &password).await?
            }
            AuthCredentials::DeviceCode { .. } | AuthCredentials::PolyServer { .. } => {
                return Err(ClientError::AuthFailed(
                    "Teams requires a Bearer token".into(),
                ));
            }
        };
        self.http.set_token(token.clone());
        let user = self.http.get_me().await?;
        self.account_id = Some(user.id.clone());
        self.account_display_name = Some(user.display_name.clone());
        Ok(Session {
            id: user.id.clone(),
            user: User {
                id: user.id.clone(),
                display_name: user.display_name.clone(),
                avatar_url: None,
                presence: PresenceStatus::Online,
                backend: BackendType::from(crate::SLUG),
            },
            token,
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("💼".to_string()),
            // Strip the URL scheme so `instance_id` is a bare "host:port"
            // (e.g. "localhost:9103") instead of "http://localhost:9103".
            // Route path segments cannot contain "://" — a scheme-inclusive
            // instance_id causes the Dioxus router to parse every Teams route
            // as PageNotFound, triggering an on_update redirect cascade that
            // produces a burst of unconditional app_state.write() calls and
            // hangs the WASM main thread (reproduced 5/5 times in visual audit).
            instance_id: self.http.base_url()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_string(),
            backend_url: Some(self.http.base_url().to_string()),
        })
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.account_id = None;
        self.account_display_name = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.account_id.is_some()
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![
                "graph.microsoft.com".to_string(),
                "login.microsoftonline.com".to_string(),
            ],
            description: "Microsoft Teams backend. Connects to Microsoft Graph with a \
                          Bearer token. Dev-only: not shipped in release builds because \
                          Teams' enterprise licensing blocks third-party app-store distribution."
                .to_string(),
            homepage: Some("https://teams.microsoft.com".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        Ok(self.http.get_joined_teams().await?.into_iter().map(|t| Server {
            id: t.id,
            name: t.display_name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id: account_id.clone(),
            account_display_name: account_name.clone(),
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        }).collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let t = self.http.get_team(id).await?;
        Ok(Server {
            id: t.id,
            name: t.display_name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id,
            account_display_name: account_name,
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(self.http.get_team_channels(server_id).await?.into_iter().map(|ch| Channel {
            // Channel IDs are encoded as "team_id/channel_id" per the plugin's
            // own contract (see module docs at top of file); every op splits on
            // '/' to dispatch to the teams/channels endpoint vs the chats one.
            // Without the team prefix `get_messages` falls back to /chats/{id}/
            // messages (DM endpoint) and 404s on every team channel.
            id: format!("{server_id}/{}", ch.id),
            name: ch.display_name,
            channel_type: ChannelType::Text,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        }).collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        // id is expected as "team_id/channel_id"
        let (team_id, channel_id) = id.split_once('/').ok_or_else(|| {
            ClientError::Internal(format!("Teams channel id must be 'team_id/channel_id', got '{id}'"))
        })?;
        let channels = self.http.get_team_channels(team_id).await?;
        channels
            .into_iter()
            .find(|c| c.id == channel_id)
            .map(|ch| Channel {
                // Keep the composite id for round-trip consistency.
                id: format!("{team_id}/{}", ch.id),
                name: ch.display_name,
                channel_type: ChannelType::Text,
                server_id: team_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
            })
            .ok_or_else(|| ClientError::NotFound(format!("Channel {id}")))
    }

    async fn send_message(&self, channel_id: &str, content: MessageContent) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };
        // Channel IDs are "team_id/channel_id"; chat IDs have no slash.
        let m = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.send_channel_message(team_id, ch_id, &text).await?
        } else {
            self.http.send_chat_message(channel_id, &text).await?
        };
        Ok(self.graph_message_to_poly(m))
    }

    async fn get_messages(&self, channel_id: &str, query: MessageQuery) -> ClientResult<Vec<Message>> {
        let msgs = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.get_channel_messages(team_id, ch_id, query.limit).await?
        } else {
            self.http.get_chat_messages(channel_id, query.limit).await?
        };
        Ok(msgs.into_iter().map(|m| self.graph_message_to_poly(m)).collect())
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    fn as_context_action(&self) -> Option<&dyn poly_client::ContextActionBackend> {
        Some(self)
    }

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let http = self.http.clone();
            let (tx, rx) = tokio::sync::mpsc::channel::<ClientEvent>(128);
            tokio::spawn(async move {
                loop {
                    match http.poll_events().await {
                        Ok(events) => {
                            for ev in events {
                                if let Some(ce) = teams_event_to_client(&ev)
                                    && tx.send(ce).await.is_err()
                                {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Teams poll_events error: {e:?}");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            });
            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(futures::stream::empty())
        }
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Teams"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            supports_typing_indicators: false,
            has_roles: false,       // owner/member binary; no role concept
            has_kick: true,
            has_ban: false,         // Teams has no ban concept — hide entirely
            has_timed_ban: false,
            has_channel_mgmt: true, // name + description only (no slow-mode / nsfw / position)
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    /// Teams declares one mechanism:
    ///
    /// - `oauth-sandbox` — route the Microsoft Entra ID (AAD) OAuth popup
    ///   through a sandboxed host-managed browser window. The AAD device-code
    ///   flow works without a sandbox; the interactive OAuth popup flow does
    ///   not, because the WASM UI cannot open a full-featured browser window
    ///   itself. Requires `HostCap::SandboxBrowser`.
    ///
    /// Teams does NOT use hCaptcha at the login wall (unlike Discord), so there
    /// is no separate `captcha-sandbox` mechanism — `oauth-sandbox` covers the
    /// only browser-popup scenario in the Teams login flow.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        let oauth_sandbox_enabled = self
            .settings_storage
            .get(SettingsScope::AccountGlobal, "", "oauth-sandbox")
            .map(|v| v == "true")
            .unwrap_or(false);
        Ok(vec![Mechanism {
            id: "oauth-sandbox".to_string(),
            name_key: "plugin-teams-mechanism-oauth-sandbox-label".to_string(),
            enabled: oauth_sandbox_enabled,
            requires_host_cap: Some(HostCap::SandboxBrowser),
            description_key: Some("plugin-teams-mechanism-oauth-sandbox-desc".to_string()),
        }])
    }

    async fn set_client_mechanism(&self, id: &str, enabled: bool) -> ClientResult<()> {
        match id {
            "oauth-sandbox" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "oauth-sandbox",
                if enabled { "true" } else { "false" },
            ),
            _ => Err(ClientError::NotFound(format!("unknown mechanism: {id}"))),
        }
    }


    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        // Last verified 2026-04-30
        SignupMethod::External("https://signup.live.com/signup?lic=1".into())
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}

// ── H.3.a — ModerationBackend ────────────────────────────────────────────────
#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for TeamsClient {
    /// Get the caller's permissions in a team.
    ///
    /// Fetches the team member list and checks whether the caller has the
    /// "owner" role. Teams is a binary owner/member model — no per-channel
    /// permissions exist in Graph.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let caller_id = self.account_id();
        let members = self.http.get_team_members(server_id).await?;
        let is_owner = members.iter().any(|m| {
            m.user_id.as_deref() == Some(caller_id.as_str())
                && m.roles.iter().any(|r| r == "owner")
        });
        Ok(MemberPermissions {
            manage_server: is_owner,
            manage_channels: is_owner,
            manage_roles: false, // no role concept in Teams
            kick_members: is_owner,
            ban_members: false,  // Teams has no ban concept
            manage_messages: is_owner,
            timeout_members: false, // no timeout concept in Teams
            display_role: if is_owner { "Owner".into() } else { "Member".into() },
            power_level: None,
        })
    }

    /// Kick a member by resolving their membership ID via the members list.
    ///
    /// `member_id` may be the user's OID; we look up the membership ID
    /// (base64-encoded composite) before issuing the DELETE.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        let members = self.http.get_team_members(server_id).await?;
        let membership_id = members
            .iter()
            .find(|m| m.user_id.as_deref() == Some(member_id) || m.id == member_id)
            .map(|m| m.id.clone())
            .ok_or_else(|| ClientError::NotFound(format!("member {member_id} not in team {server_id}")))?;
        self.http.delete_team_member(server_id, &membership_id).await
    }

    // ban_member / unban_member / timeout_member / untimeout_member / get_bans —
    // Teams has no ban or timeout concept. Return NotSupported so the UI gates
    // these behind has_ban=false / has_timed_ban=false and hides them entirely.

    async fn ban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "ban_member: Teams has no ban concept".into(),
        ))
    }

    async fn unban_member(
        &self,
        _server_id: &str,
        _member_id: &str,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "unban_member: Teams has no ban concept".into(),
        ))
    }

    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "timeout_member: Teams has no timeout concept".into(),
        ))
    }

    async fn untimeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "untimeout_member: Teams has no timeout concept".into(),
        ))
    }

    async fn get_bans(&self, _server_id: &str) -> ClientResult<Vec<BannedMember>> {
        Err(ClientError::NotSupported(
            "get_bans: Teams has no ban concept".into(),
        ))
    }

    /// Soft-delete a channel message.
    ///
    /// Uses the Graph softDelete action which preserves the compliance copy.
    /// `channel_id` must be in `"team_id/channel_id"` format per plugin contract.
    async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams delete_message requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http
            .soft_delete_channel_message(team_id, ch_id, message_id)
            .await
    }

    /// Update a channel.
    ///
    /// Only `name` (→ `displayName`) and `topic` (→ `description`) are
    /// forwarded to Graph. `slow_mode_secs`, `nsfw`, and `position` are silently
    /// ignored — Teams/Graph has no equivalent fields.
    ///
    /// `channel_id` must be in `"team_id/channel_id"` format.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams update_channel requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        // These three fields have no Graph equivalent — log at debug so we
        // don't warn-spam every time the UI sends a full update payload.
        // SOLID-audit-teams (Phase B.2).
        if update.slow_mode_secs.is_some() {
            tracing::debug!("Teams update_channel: slow_mode_secs has no Graph equivalent — ignored");
        }
        if update.nsfw.is_some() {
            tracing::debug!("Teams update_channel: nsfw has no Graph equivalent — ignored");
        }
        if update.position.is_some() {
            tracing::debug!("Teams update_channel: position has no Graph equivalent — ignored");
        }
        self.http
            .patch_channel(
                team_id,
                ch_id,
                update.name.as_deref(),
                update.topic.as_deref(),
            )
            .await
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Teams: Microsoft Graph has no channel position endpoint".into(),
        ))
    }

    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        Err(ClientError::NotSupported(
            "get_moderation_log: Teams has no moderation log".into(),
        ))
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported(
            "get_server_roles: Teams has no role concept".into(),
        ))
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for TeamsClient {
    async fn get_user(&self, _id: &str) -> ClientResult<User> {
        // The trait contract is "Ok(User) | Err(NotFound | Network | Auth)".
        // Returning NotFound for "this backend has no user-lookup endpoint"
        // would lie to callers — they'd give up looking elsewhere when in
        // fact the user might exist, just not on Teams. Use NotSupported.
        Err(ClientError::NotSupported("Teams user lookup not supported".into()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // LSP: Teams has no friend concept (`add_friend` etc. below return
        // `NotSupported`). Returning `Ok(vec![])` lies to callers ("you have
        // no friends in Teams") instead of disclosing "no such API".
        // SOLID-audit-teams (Phase B.1).
        Err(ClientError::NotSupported(
            "get_friends: Teams has no friend system".into(),
        ))
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no friend system".into()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no user note system".into()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams: block not supported via this interface".into()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams: unblock not supported via this interface".into()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no ignore concept".into()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Teams has no ignore concept".into()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()> {
        let availability = match status {
            PresenceStatus::Online => "Available",
            PresenceStatus::Idle => "Away",
            PresenceStatus::DoNotDisturb => "DoNotDisturb",
            PresenceStatus::Offline
            | PresenceStatus::Invisible
            | PresenceStatus::Unknown => "Offline",
        };
        self.http.set_presence(availability).await
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// Teams supports chat channels as DMs. No group-DM management API exposed.

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for TeamsClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let account_id = self.account_id();
        Ok(self.http.get_chats().await?.into_iter().map(|chat| {
            let contact = chat.members.iter()
                .find(|m| m.user_id.as_deref() != Some(account_id.as_str()))
                .and_then(|m| {
                    m.display_name.as_ref().map(|name| User {
                        id: m.user_id.clone().unwrap_or_default(),
                        display_name: name.clone(),
                        avatar_url: None,
                        presence: PresenceStatus::Offline,
                        backend: BackendType::from(crate::SLUG),
                    })
                })
                .unwrap_or_else(|| self.unknown_user());
            DmChannel {
                id: chat.id,
                user: contact,
                last_message: None,
                unread_count: 0,
                backend: BackendType::from(crate::SLUG),
                account_id: account_id.clone(),
            }
        }).collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: not yet implemented for Teams".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Teams has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_group_member: not yet implemented for Teams".to_string(),
        ))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "remove_group_member: not yet implemented for Teams".to_string(),
        ))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_users_to_group_dm: not yet implemented for Teams".to_string(),
        ))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "close_dm_channel: not yet implemented for Teams".to_string(),
        ))
    }

    async fn mute_conversation(
        &self,
        channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        // SOLID-audit-teams C.4: Graph `PATCH /chats/{id}/members/{membershipId}` with
        // `notificationSettings` requires knowing the caller's membership ID for each chat,
        // which requires an extra round-trip per call.  The in-memory `muted_dms` store that
        // already backs the context-menu "mute-dm" action is the same source of truth the
        // sidebar uses — wire `mute_conversation` to it directly so both call sites agree.
        // The `_until` timed-mute field is noted but Graph notifications don't support
        // expiry; we store the mute unconditionally (best-effort parity).
        tracing::debug!(channel_id, "teams: mute_conversation (in-memory store)");
        self.muted_dms
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(channel_id.to_string());
        Ok(())
    }

    async fn unmute_conversation(&self, channel_id: &str) -> ClientResult<()> {
        // SOLID-audit-teams C.4: symmetric with mute_conversation above.
        tracing::debug!(channel_id, "teams: unmute_conversation (in-memory store)");
        self.muted_dms
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(channel_id);
        Ok(())
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "leave_group_dm: not yet implemented for Teams".to_string(),
        ))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "edit_group_dm: not yet implemented for Teams".to_string(),
        ))
    }
}

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for TeamsClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::PerServer,
            section_key: "team-profile".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "display-name".to_string(),
                    kind: SettingKind::TextInput,
                    default_value: "\"\"".to_string(),
                    extra: String::new(),
                },
                SettingDescriptor {
                    key: "description".to_string(),
                    kind: SettingKind::TextInput,
                    default_value: "\"\"".to_string(),
                    extra: String::new(),
                },
            ],
            info_block: None,
        }])
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ViewDescriptorBackend for TeamsClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-teams-overview-title".to_string()),
                subtitle_key: Some("plugin-teams-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        // Empty channel_id signals the account overview — return one card per team.
        if !channel_id.is_empty() {
            return Err(ClientError::NotSupported("view-rows not yet implemented for team channels".into()));
        }

        let servers = self.get_servers().await?;

        // Fetch channel counts concurrently for each team.
        let mut rows = Vec::with_capacity(servers.len());
        for server in &servers {
            let channel_count = self
                .get_channels(&server.id)
                .await
                .map(|chs| chs.len())
                .unwrap_or(0);

            let meta = format!(
                "{} channel{} · {} unread · @{} mentions",
                channel_count,
                if channel_count == 1 { "" } else { "s" },
                server.unread_count,
                server.mention_count,
            );

            rows.push(ViewRow {
                id: server.id.clone(),
                primary_text: server.name.clone(),
                secondary_text: server.description.clone(),
                meta_text: Some(meta),
                icon: None,
                badge: if server.mention_count > 0 {
                    Some(format!("@{}", server.mention_count))
                } else if server.unread_count > 0 {
                    Some(server.unread_count.to_string())
                } else {
                    None
                },
                context_menu_target_kind: MenuTargetKind::Server,
                preview_image_url: None,
                is_video: false,
            });
        }

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("view-detail not yet implemented".into()))
    }
}

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for TeamsClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // Inline helper: build a MenuItem with common defaults.
        let item = |id: &str, label_key: &str, slot: MenuSlot, variant: MenuItemVariant| MenuItem {
            id: id.to_string(),
            parent_id: None,
            slot,
            label_key: label_key.to_string(),
            icon: None,
            item_variant: variant,
            shortcut: None,
            block: None,
        };

        match target {
            MenuTargetKind::Channel => {
                let hidden = self
                    .hidden_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                let pinned = self
                    .pinned_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                let muted = self
                    .muted_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    item("mark-read", "plugin-teams-menu-mark-read-label", MenuSlot::Top, MenuItemVariant::Normal),
                    item("mark-unread", "plugin-teams-menu-mark-unread-label", MenuSlot::Top, MenuItemVariant::Normal),
                    if pinned {
                        item("unpin-channel", "plugin-teams-menu-unpin-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("pin-channel", "plugin-teams-menu-pin-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if hidden {
                        item("show-channel", "plugin-teams-menu-show-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("hide-channel", "plugin-teams-menu-hide-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if muted {
                        item("unmute-channel", "plugin-teams-menu-unmute-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("mute-channel", "plugin-teams-menu-mute-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                ])
            }

            MenuTargetKind::Server => {
                let muted = self
                    .muted_teams
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    if muted {
                        item("unmute-team", "plugin-teams-menu-unmute-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("mute-team", "plugin-teams-menu-mute-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    item("get-team-code", "plugin-teams-menu-get-team-code-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("manage-team", "plugin-teams-menu-manage-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("team-settings", "plugin-teams-menu-team-settings-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("edit-per-team-profile", "plugin-teams-menu-edit-per-team-profile-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("leave-team", "plugin-teams-menu-leave-team-label", MenuSlot::BeforeLeave, MenuItemVariant::Destructive),
                ])
            }

            MenuTargetKind::User => Ok(vec![
                item("open-chat", "plugin-teams-menu-open-chat-label", MenuSlot::Top, MenuItemVariant::Normal),
                item("view-profile", "plugin-teams-menu-view-profile-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                item("schedule-meeting", "plugin-teams-menu-schedule-meeting-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
            ]),

            MenuTargetKind::Message => {
                let saved = self
                    .saved_messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    item("react", "plugin-teams-menu-react-label", MenuSlot::Top, MenuItemVariant::Normal),
                    item("reply-in-thread", "plugin-teams-menu-reply-in-thread-label", MenuSlot::Top, MenuItemVariant::Normal),
                    if saved {
                        item("unsave-message", "plugin-teams-menu-unsave-message-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("save-message", "plugin-teams-menu-save-message-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    item("mark-important", "plugin-teams-menu-mark-important-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    item("delete-message", "plugin-teams-menu-delete-message-label", MenuSlot::BeforeLeave, MenuItemVariant::Destructive),
                ])
            }

            MenuTargetKind::Dm => {
                let muted = self
                    .muted_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                let hidden = self
                    .hidden_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(target_id);
                Ok(vec![
                    if muted {
                        item("unmute-dm", "plugin-teams-menu-unmute-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("mute-dm", "plugin-teams-menu-mute-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if hidden {
                        item("show-dm", "plugin-teams-menu-show-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        item("hide-dm", "plugin-teams-menu-hide-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                ])
            }

            MenuTargetKind::Category => Ok(Vec::new()),
        }
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            // ── Channel toggles ──────────────────────────────────────────────
            "pin-channel" => {
                self.pinned_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unpin-channel" => {
                self.pinned_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "hide-channel" => {
                self.hidden_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "show-channel" => {
                self.hidden_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "mute-channel" => {
                self.muted_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-channel" => {
                self.muted_channels
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "mark-read"
            | "mark-unread"
            | "leave-team"
            | "get-team-code"
            | "manage-team"
            | "team-settings"
            | "edit-per-team-profile"
            | "open-chat"
            | "view-profile"
            | "schedule-meeting" => Ok(ActionOutcome::Noop),

            // ── Team toggles ─────────────────────────────────────────────────
            "mute-team" => {
                self.muted_teams
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-team" => {
                self.muted_teams
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }

            // ── User actions ─────────────────────────────────────────────────

            // ── Message toggles ──────────────────────────────────────────────
            "save-message" => {
                self.saved_messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unsave-message" => {
                self.saved_messages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "react" | "reply-in-thread" | "mark-important" | "delete-message" => {
                Ok(ActionOutcome::Noop)
            }

            // ── DM toggles ───────────────────────────────────────────────────
            "mute-dm" => {
                self.muted_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-dm" => {
                self.muted_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }
            "hide-dm" => {
                self.hidden_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(target_id.to_string());
                Ok(ActionOutcome::RefreshTarget)
            }
            "show-dm" => {
                self.hidden_dms
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(target_id);
                Ok(ActionOutcome::RefreshTarget)
            }

            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(vec![ComposerButton {
            id: "mention".to_string(),
            label_key: "plugin-teams-composer-mention-label".to_string(),
            icon: "@".to_string(),
            position: ComposerSlot::RightOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // Teams has no backend-specific per-message actions beyond host universals.
        Ok(Vec::new())
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "mention" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown composer action: {other}"))),
        }
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown message action: {action_id}")))
    }
}
