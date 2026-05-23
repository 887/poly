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

#![allow(clippy::if_same_then_else)]

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "lemmy";

#[cfg(feature = "native")]
mod api;

#[cfg(feature = "native")]
pub mod signup;

/// WIT bindings for the WASM plugin (WASI targets only).
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

#[cfg(feature = "native")]
use api::{
    BanFromCommunityRequest, LemmyHttpClient, LemmySession, community_to_channel, cursor_to_page,
    map_comment_to_message, map_community_to_server, map_community_to_viewrow, map_person,
    map_pm_to_dm_channel, map_post_to_message, map_post_to_viewrow, next_page_cursor,
};
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::collections::HashMap;
#[cfg(feature = "native")]
use std::pin::Pin;

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
    http: LemmyHttpClient,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
    /// Stored version override (None = use api::DEFAULT_CLIENT_VERSION).
    version_override: std::sync::Mutex<Option<String>>,
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
    fn render_previews_enabled(&self) -> bool {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "render-previews")
            .is_none_or(|v| v != "false")
    }

    /// Return the currently stored user_id, if authenticated.
    fn current_user_id(&self) -> Option<i64> {
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
    fn current_account_metadata(&self) -> ClientResult<(String, String)> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy client is not authenticated".to_string())
        })?;
        let account_id = format!("lemmy-session-{}", session.user_id);
        let display = session.user_display_name;
        Ok((account_id, display))
    }

    /// Extract a community_id integer from a `lemmy-community-{id}` server ID string.
    fn parse_community_id(server_id: &str) -> ClientResult<i64> {
        server_id
            .strip_prefix("lemmy-community-")
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| {
                ClientError::NotFound(format!("invalid Lemmy server id: {server_id}"))
            })
    }

    /// Extract a post_id integer from a `lemmy-feed-{community_id}` channel ID.
    fn parse_feed_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-feed-")
            .and_then(|s| s.parse::<i64>().ok())
    }

    /// Extract a community_id integer from a `lemmy-comments-{community_id}` channel ID.
    /// Phase D — synthetic channel for the community-level recent-comments feed.
    fn parse_comments_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-comments-")
            .and_then(|s| s.parse::<i64>().ok())
    }

    /// Extract a post_id integer from a `lemmy-post-{id}` channel/message ID.
    fn parse_post_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-post-")
            .and_then(|s| s.parse::<i64>().ok())
    }

    /// Parse a Lemmy person integer ID from either a bare integer string
    /// or a `lemmy-user-{id}` prefixed string.
    fn parse_person_id(member_id: &str) -> ClientResult<i64> {
        // Accept both "lemmy-user-42" and bare "42".
        let raw = member_id
            .strip_prefix("lemmy-user-")
            .unwrap_or(member_id);
        raw.parse::<i64>().map_err(|_err| {
            ClientError::NotFound(format!("invalid Lemmy member id: {member_id}"))
        })
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for LemmyClient {
    // ── Authentication ──────────────────────────────────────────────────────

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let (username, password) = match credentials {
            AuthCredentials::EmailPassword { email, password } => (email, password),
            AuthCredentials::Token(jwt) => {
                // Restore from persisted JWT: store it and fetch user from site
                let placeholder = LemmySession {
                    jwt: jwt.clone(),
                    user_id: 0,
                    user_display_name: String::new(),
                    user_avatar_url: None,
                };
                self.http.set_session(placeholder);
                let site = self.http.fetch_site().await?;
                let person = site
                    .my_user
                    .ok_or_else(|| {
                        ClientError::AuthFailed(
                            "JWT is invalid or expired (no my_user in site response)".to_string(),
                        )
                    })?
                    .local_user_view
                    .person;

                let session = LemmySession {
                    jwt,
                    user_id: person.id,
                    user_display_name: person
                        .display_name
                        .clone()
                        .unwrap_or_else(|| person.name.clone()),
                    user_avatar_url: person.avatar.clone(),
                };
                self.http.set_session(session.clone());

                let instance_id = self.instance_id();
                return Ok(Session {
                    id: format!("lemmy-session-{}", person.id),
                    user: map_person(&person),
                    token: session.jwt,
                    backend: BackendType::from(crate::SLUG),
                    icon_emoji: None,
                    instance_id,
                    backend_url: Some(self.base_url().to_string()),
                });
            }
            other @ (AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. }) => {
                return Err(ClientError::AuthFailed(format!(
                    "Lemmy does not support {:?} credentials",
                    std::mem::discriminant(&other)
                )));
            }
        };

        let login_resp = self.http.login(&username, &password).await?;
        let jwt = login_resp.jwt.ok_or_else(|| {
            ClientError::AuthFailed(
                "Lemmy login succeeded but no JWT was returned (may require email verification)"
                    .to_string(),
            )
        })?;

        // Store a temporary session so fetch_site can use it
        let placeholder = LemmySession {
            jwt: jwt.clone(),
            user_id: 0,
            user_display_name: String::new(),
            user_avatar_url: None,
        };
        self.http.set_session(placeholder);

        let site = self.http.fetch_site().await?;
        let person = site
            .my_user
            .ok_or_else(|| {
                ClientError::AuthFailed(
                    "Login OK but site returned no user info".to_string(),
                )
            })?
            .local_user_view
            .person;

        let session = LemmySession {
            jwt: jwt.clone(),
            user_id: person.id,
            user_display_name: person
                .display_name
                .clone()
                .unwrap_or_else(|| person.name.clone()),
            user_avatar_url: person.avatar.clone(),
        };
        self.http.set_session(session);

        let instance_id = self.instance_id();
        Ok(Session {
            id: format!("lemmy-session-{}", person.id),
            user: map_person(&person),
            token: jwt,
            backend: BackendType::from(crate::SLUG),
            icon_emoji: None,
            instance_id,
            backend_url: Some(self.base_url().to_string()),
        })
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.clear_session();
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["<lemmy instance from account>".to_string()],
            description: "Lemmy backend. Federated link aggregator — connects \
                          to any Lemmy / Kbin instance (lemmy.world, lemmy.ml, \
                          beehaw.org, your own). Browse communities, comment, \
                          and submit posts when signed in."
                .to_string(),
            homepage: Some("https://join-lemmy.org".to_string()),
        }
    }

    // ── Servers / Communities ───────────────────────────────────────────────

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let (account_id, account_display_name) = self.current_account_metadata()?;
        let resp = self.http.fetch_subscribed_communities().await?;
        Ok(resp
            .communities
            .iter()
            .map(|view| map_community_to_server(view, &account_id, &account_display_name))
            .collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let community_id = Self::parse_community_id(id)?;
        let (account_id, account_display_name) = self.current_account_metadata()?;
        let view = self.http.fetch_community(community_id).await?;
        Ok(map_community_to_server(&view, &account_id, &account_display_name))
    }

    fn as_server_admin(&self) -> Option<&dyn poly_client::ServerAdminBackend> {
        Some(self)
    }

    fn as_discover(&self) -> Option<&dyn poly_client::DiscoverBackend> {
        Some(self)
    }

    // ── Channels ────────────────────────────────────────────────────────────

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let community_id = Self::parse_community_id(server_id)?;
        let view = self.http.fetch_community(community_id).await?;
        Ok(vec![community_to_channel(&view.community)])
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        // channel ID is `lemmy-feed-{community_id}` or `lemmy-post-{post_id}`
        if let Some(community_id) = Self::parse_feed_channel(id) {
            let view = self.http.fetch_community(community_id).await?;
            return Ok(community_to_channel(&view.community));
        }
        Err(ClientError::NotFound(format!("channel not found: {id}")))
    }

    // ── Messages ────────────────────────────────────────────────────────────

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };

        if let Some(post_id) = Self::parse_post_channel(channel_id) {
            let view = self.http.create_comment(post_id, &text, None).await?;
            return Ok(map_comment_to_message(&view));
        }

        Err(ClientError::NotSupported(
            "send_message: channel must be a lemmy-post-{id} thread channel".to_string(),
        ))
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        // `lemmy-feed-{community_id}` → return posts as messages
        if let Some(community_id) = Self::parse_feed_channel(channel_id) {
            let resp = self.http.fetch_posts(community_id).await?;
            let mut messages: Vec<Message> =
                resp.posts.iter().map(map_post_to_message).collect();
            messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            return Ok(messages);
        }

        // `lemmy-post-{post_id}` → return comments as messages
        if let Some(post_id) = Self::parse_post_channel(channel_id) {
            let resp = self.http.fetch_comments(post_id).await?;
            let mut messages: Vec<Message> =
                resp.comments.iter().map(map_comment_to_message).collect();
            messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            return Ok(messages);
        }

        Err(ClientError::NotFound(format!(
            "unknown Lemmy channel: {channel_id}"
        )))
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        // Lemmy communities don't expose a member list via the standard API
        Ok(vec![])
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    // ── Notifications ─────────────────────────────────────────────────────────

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    // ── Voice / Settings / Views / Context: moved to C.1 sub-traits below ────

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    fn as_context_action(&self) -> Option<&dyn poly_client::ContextActionBackend> {
        Some(self)
    }

    // ── Real-time events ──────────────────────────────────────────────────────

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // Lemmy v0.19+ removed WebSocket. Real-time requires polling.
        // For now return an empty stream; polling will be added in a later phase.
        Box::pin(stream::empty())
    }



    /// Return the mechanism inventory for this backend.
    ///
    /// Declares the `render-previews` mechanism, which controls whether
    /// forum post thumbnails (`thumbnail_url`) are fetched from the pict-rs
    /// CDN and displayed next to post titles. Default ON.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        let enabled = self.render_previews_enabled();
        Ok(vec![Mechanism {
            id: "render-previews".to_string(),
            name_key: "plugin-lemmy-mechanism-render-previews-label".to_string(),
            enabled,
            requires_host_cap: None,
            description_key: Some("plugin-lemmy-mechanism-render-previews-desc".to_string()),
        }])
    }

    /// Toggle the `render-previews` mechanism on or off.
    async fn set_client_mechanism(&self, id: &str, enabled: bool) -> ClientResult<()> {
        match id {
            "render-previews" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "render-previews",
                if enabled { "true" } else { "false" },
            ),
            _ => Err(ClientError::NotFound(format!("unknown mechanism: {id}"))),
        }
    }

    // --- Forum channels (H.2.b — moved to ForumBackend) ---

    fn as_forum(&self) -> Option<&dyn poly_client::ForumBackend> {
        Some(self)
    }

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
    }

    // ── Backend info ──────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Lemmy"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            has_roles: false,
            has_kick: false,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: false,
            has_moderation_log: true,
            community_search: CommunitySearchSupport::SubscribedLocalAll,
            // Phase D — Posts | Comments toggle.
            supports_comment_feed: true,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }
    }

    // search_communities moved to DiscoverBackend below (H.4.c)

    fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
        let base = server_url.unwrap_or("https://lemmy.ml");
        SignupMethod::External(format!("{}/signup", base.trim_end_matches('/')))
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| api::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| api::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}

// ── H.2.b — ForumBackend ─────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ForumBackend for LemmyClient {
    async fn get_forum_posts(
        &self,
        _forum_channel_id: &str,
        _sort: ForumSortOrder,
        _limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>> {
        Err(ClientError::NotSupported("get_forum_posts".to_string()))
    }

    /// C.7 — wire `create_forum_post` for Lemmy via `POST /api/v3/post`.
    ///
    /// `forum_channel_id` must be `lemmy-feed-{community_id}`.  Tags are
    /// ignored (Lemmy's tag system requires community-specific tag IDs that
    /// the UI doesn't yet expose).
    async fn create_forum_post(
        &self,
        forum_channel_id: &str,
        title: &str,
        body: &str,
        _tags: Vec<String>,
    ) -> ClientResult<ForumPost> {
        let community_id = Self::parse_feed_channel(forum_channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "create_forum_post: expected lemmy-feed-<id>, got: {forum_channel_id}"
            ))
        })?;

        let post_view = self
            .http
            .create_post(community_id, title, Some(body), None)
            .await?;

        Ok(ForumPost {
            thread: poly_client::ThreadInfo {
                thread_id: format!("lemmy-post-{}", post_view.post.id),
                parent_channel_id: forum_channel_id.to_string(),
                message_count: 0,
                member_count: 0,
            },
            applied_tags: vec![],
            starter_message_id: None,
        })
    }

    /// Return recent comments across a Lemmy community (Phase D).
    ///
    /// `channel_id` must be a `lemmy-feed-{community_id}` channel. Returns up
    /// to `query.limit` (default 50) comments sorted by newest first, each
    /// mapped to a `Message` via `map_comment_to_message`.
    async fn get_recent_comments(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let community_id = Self::parse_feed_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "get_recent_comments: expected lemmy-feed-<id>, got: {channel_id}"
            ))
        })?;

        let limit = query.limit.unwrap_or(50).min(200);
        let resp = self.http.fetch_community_comments(community_id, limit).await?;

        let messages: Vec<Message> = resp
            .comments
            .iter()
            .map(|view| map_comment_to_message(view))
            .collect();

        Ok(messages)
    }
}

// ── H.3.a — ModerationBackend ────────────────────────────────────────────────
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for LemmyClient {
    async fn get_my_permissions(
        &self,
        _server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        Err(ClientError::NotSupported("Lemmy: permission model not exposed".to_string()))
    }

    /// Lemmy has no kick concept — community membership is implicit.
    async fn kick_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy has no kick concept; community membership is implicit".to_string(),
        ))
    }

    /// Ban a member from a community (permanent — no `expires`).
    ///
    /// `server_id` is `lemmy-community-{id}`.
    /// `member_id` is a Lemmy person id as a string or `lemmy-user-{id}`.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        let person_id = Self::parse_person_id(member_id)?;
        self.http
            .ban_from_community(BanFromCommunityRequest {
                community_id,
                person_id,
                ban: true,
                reason: reason.map(str::to_string),
                expires: None,
                remove_data: false,
            })
            .await
            .map(|_| ())
    }

    /// Unban a member from a community.
    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        let person_id = Self::parse_person_id(member_id)?;
        self.http
            .ban_from_community(BanFromCommunityRequest {
                community_id,
                person_id,
                ban: false,
                reason: None,
                expires: None,
                remove_data: false,
            })
            .await
            .map(|_| ())
    }

    /// Timeout a member by banning with a native `expires` timestamp.
    ///
    /// Lemmy's `ban_user` endpoint accepts a Unix timestamp `expires` field,
    /// making a short ban functionally equivalent to a timeout. This method is
    /// a thin wrapper that calls `ban_from_community` with `ban: true` and the
    /// computed expiry.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        let person_id = Self::parse_person_id(member_id)?;
        self.http
            .ban_from_community(BanFromCommunityRequest {
                community_id,
                person_id,
                ban: true,
                reason: reason.map(str::to_string),
                expires: Some(until.timestamp()),
                remove_data: false,
            })
            .await
            .map(|_| ())
    }

    /// Remove a timeout from a member by unbanning them.
    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.unban_member(server_id, member_id).await
    }

    /// List banned members by querying the modlog for `ModBanFromCommunity` events.
    ///
    /// Lemmy has no `/community/bans` endpoint; `GET /api/v3/modlog` with
    /// `type_=ModBanFromCommunity` is the only way to retrieve the ban list.
    /// The response includes all ban/unban history; we deduplicate per person
    /// keeping only the most recent ban entry (ignoring unban records).
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let community_id = Self::parse_community_id(server_id)?;
        let modlog = self.http.get_modlog_bans(community_id).await?;

        // Deduplicate: for each person keep only their most recent entry.
        // If the most recent entry has `banned==true`, they are still banned.
        // If it has `banned==false` (unban), they are not currently banned.
        let mut most_recent: HashMap<i64, api::ModBanFromCommunityView> = HashMap::new();
        for entry in modlog {
            most_recent
                .entry(entry.banned_person.id)
                .and_modify(|existing| {
                    if entry.mod_ban_from_community.when_
                        > existing.mod_ban_from_community.when_
                    {
                        *existing = entry.clone();
                    }
                })
                .or_insert(entry);
        }

        // Only include entries where the most recent action was a ban.
        let by_person: HashMap<i64, api::ModBanFromCommunityView> = most_recent
            .into_iter()
            .filter(|(_, e)| e.mod_ban_from_community.banned)
            .collect();

        Ok(by_person
            .values()
            .map(|e| BannedMember {
                user_id: format!("lemmy-user-{}", e.banned_person.id),
                display_name: e
                    .banned_person
                    .display_name
                    .clone()
                    .unwrap_or_else(|| e.banned_person.name.clone()),
                avatar_url: e.banned_person.avatar.clone(),
                reason: e.mod_ban_from_community.reason.clone(),
                expires_at: e
                    .mod_ban_from_community
                    .expires
                    .map(|dt| dt.to_rfc3339()),
                banned_at: Some(e.mod_ban_from_community.when_.to_rfc3339()),
            })
            .collect())
    }

    /// Delete (remove) a message by ID.
    ///
    /// Message IDs are prefixed:
    /// - `lemmy-post-{id}` → `POST /api/v3/post/remove`
    /// - `lemmy-comment-{id}` → `POST /api/v3/comment/remove`
    ///
    /// The `channel_id` parameter is ignored — Lemmy's remove endpoints use
    /// only the post/comment id.
    async fn delete_message(
        &self,
        _channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        if let Some(post_id) = message_id
            .strip_prefix("lemmy-post-")
            .and_then(|s| s.parse::<i64>().ok())
        {
            return self.http.remove_post(post_id, None).await;
        }

        if let Some(comment_id) = message_id
            .strip_prefix("lemmy-comment-")
            .and_then(|s| s.parse::<i64>().ok())
        {
            return self.http.remove_comment(comment_id, None).await;
        }

        Err(ClientError::NotFound(format!(
            "delete_message: unrecognised message id '{message_id}'; \
             expected 'lemmy-post-{{n}}' or 'lemmy-comment-{{n}}'"
        )))
    }

    /// Lemmy community update is admin-only and out of scope for v1.
    async fn update_channel(
        &self,
        _channel_id: &str,
        _update: UpdateChannelParams,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy: 'channel' = community; community update is admin-only and out-of-scope for v1"
                .to_string(),
        ))
    }

    /// Lemmy has no channel reordering concept.
    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy: channel reordering is not supported".to_string(),
        ))
    }

    /// Fetch the moderation log for a community.
    ///
    /// Aggregates `removed_posts`, `removed_comments`, and
    /// `banned_from_community` from `GET /api/v3/modlog` and returns them
    /// sorted by timestamp (most recent first), capped at `limit`.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        let community_id = Self::parse_community_id(server_id)?;
        let modlog = self.http.get_modlog(community_id).await?;

        let mut entries: Vec<ModerationLogEntry> = Vec::new();

        for e in &modlog.banned_from_community {
            let action = if e.mod_ban_from_community.banned {
                if e.mod_ban_from_community.expires.is_some() {
                    ModerationAction::MemberTimedOut
                } else {
                    ModerationAction::MemberBanned
                }
            } else {
                ModerationAction::MemberUnbanned
            };
            let moderator = e.moderator.as_ref().map_or_else(
                || User {
                    id: "lemmy-user-unknown".to_string(),
                    display_name: "Unknown".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                map_person,
            );
            entries.push(ModerationLogEntry {
                id: format!("lemmy-modlog-ban-{}", e.mod_ban_from_community.id),
                action,
                moderator,
                target_user_id: Some(format!("lemmy-user-{}", e.banned_person.id)),
                target_display_name: Some(
                    e.banned_person
                        .display_name
                        .clone()
                        .unwrap_or_else(|| e.banned_person.name.clone()),
                ),
                channel_id: None,
                message_id: None,
                reason: e.mod_ban_from_community.reason.clone(),
                timestamp: e.mod_ban_from_community.when_.to_rfc3339(),
            });
        }

        for e in &modlog.removed_posts {
            let moderator = e.moderator.as_ref().map_or_else(
                || User {
                    id: "lemmy-user-unknown".to_string(),
                    display_name: "Unknown".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                map_person,
            );
            entries.push(ModerationLogEntry {
                id: format!("lemmy-modlog-rmpost-{}", e.mod_remove_post.id),
                action: ModerationAction::MessageDeleted,
                moderator,
                target_user_id: None,
                target_display_name: None,
                channel_id: Some(format!(
                    "lemmy-feed-{}",
                    e.community.id
                )),
                message_id: Some(format!("lemmy-post-{}", e.post.id)),
                reason: e.mod_remove_post.reason.clone(),
                timestamp: e.mod_remove_post.when_.to_rfc3339(),
            });
        }

        for e in &modlog.removed_comments {
            let moderator = e.moderator.as_ref().map_or_else(
                || User {
                    id: "lemmy-user-unknown".to_string(),
                    display_name: "Unknown".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from(crate::SLUG),
                },
                map_person,
            );
            entries.push(ModerationLogEntry {
                id: format!("lemmy-modlog-rmcomment-{}", e.mod_remove_comment.id),
                action: ModerationAction::MessageDeleted,
                moderator,
                target_user_id: Some(format!("lemmy-user-{}", e.commenter.id)),
                target_display_name: Some(
                    e.commenter
                        .display_name
                        .clone()
                        .unwrap_or_else(|| e.commenter.name.clone()),
                ),
                channel_id: Some(format!("lemmy-feed-{}", e.community.id)),
                message_id: Some(format!("lemmy-comment-{}", e.comment.id)),
                reason: e.mod_remove_comment.reason.clone(),
                timestamp: e.mod_remove_comment.when_.to_rfc3339(),
            });
        }

        // Sort most-recent first.
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit);
        Ok(entries)
    }

    async fn get_server_roles(&self, _server_id: &str) -> ClientResult<Vec<Role>> {
        Err(ClientError::NotSupported("Lemmy: no role concept".to_string()))
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for LemmyClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        // id is `lemmy-user-{n}` — we return a minimal user from session if it matches,
        // otherwise return an error (full user fetch is not needed for the current scope).
        if let Some(session) = self.http.session() {
            let own_id = format!("lemmy-user-{}", session.user_id);
            if id == own_id {
                return Ok(User {
                    id: own_id,
                    display_name: session.user_display_name,
                    avatar_url: session.user_avatar_url,
                    presence: PresenceStatus::Online,
                    backend: BackendType::from(crate::SLUG),
                });
            }
        }
        Err(ClientError::NotFound(format!("user not found: {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // Lemmy has no friends concept
        Ok(vec![])
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no friend system".to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no friend system".to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no friend system".to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no friend system".to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no user note system".to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy: block not supported via this interface".to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy: unblock not supported via this interface".to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no ignore concept".to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no ignore concept".to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no presence system".to_string()))
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// Lemmy supports private messages (1:1 DMs). No group DMs.

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for LemmyClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let my_user_id = self.current_user_id().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy client is not authenticated".to_string())
        })?;
        let (account_id, _) = self.current_account_metadata()?;

        let resp = self.http.fetch_private_messages().await?;

        let mut by_partner: HashMap<i64, _> = HashMap::new();
        for view in &resp.private_messages {
            let partner_id = if view.creator.id == my_user_id {
                view.recipient.id
            } else {
                view.creator.id
            };
            by_partner
                .entry(partner_id)
                .and_modify(|existing: &mut &api::PrivateMessageView| {
                    if view.private_message.published > existing.private_message.published {
                        *existing = view;
                    }
                })
                .or_insert(view);
        }

        Ok(by_partner
            .values()
            .map(|view| map_pm_to_dm_channel(view, my_user_id, &account_id))
            .collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: not yet implemented for Lemmy".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Lemmy has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no group DMs".to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no group DMs".to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no group DMs".to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "close_dm_channel: not yet implemented for Lemmy".to_string(),
        ))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no conversation mute API".to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no conversation mute API".to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no group DMs".to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no group DMs".to_string()))
    }
}

// ── H.4.a — MessagingBackend ─────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::MessagingBackend for LemmyClient {
    async fn send_typing(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no typing indicators".to_string()))
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };

        if let Some(post_id) = Self::parse_post_channel(channel_id) {
            // reply_to_message_id is `lemmy-comment-{id}`
            let parent_id = reply_to_message_id
                .strip_prefix("lemmy-comment-")
                .and_then(|s| s.parse::<i64>().ok());
            let view = self.http.create_comment(post_id, &text, parent_id).await?;
            return Ok(map_comment_to_message(&view));
        }

        Err(ClientError::NotSupported(
            "send_reply_message: channel must be a lemmy-post-{id} thread channel".to_string(),
        ))
    }

    async fn search_messages(
        &self,
        _query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Err(ClientError::NotSupported("search_messages: Lemmy search not yet implemented".to_string()))
    }

    async fn get_pinned_messages(&self, _channel_id: &str) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported("get_pinned_messages: not supported by Lemmy".to_string()))
    }

    async fn set_message_pinned(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _pinned: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("set_message_pinned: not supported by Lemmy".to_string()))
    }

    async fn get_channel_commands(&self, _channel_id: &str) -> ClientResult<Vec<ChatCommand>> {
        Ok(Vec::new())
    }

    async fn get_available_emojis(&self, _channel_id: &str) -> ClientResult<Vec<CustomEmoji>> {
        Ok(Vec::new())
    }

    async fn get_available_stickers(&self, _channel_id: &str) -> ClientResult<Vec<StickerItem>> {
        Ok(Vec::new())
    }
}

// ── H.4.b — ServerAdminBackend ───────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ServerAdminBackend for LemmyClient {
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported("lemmy: create_server not implemented".to_string()))
    }

    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported("lemmy: create_channel not implemented".to_string()))
    }

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        let community_id = LemmyClient::parse_community_id(server_id)?;
        self.http
            .put_community(community_id, banner_url)
            .await
            .map(|_| ())
    }

    async fn mark_channel_read(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("lemmy: mark_channel_read not implemented".to_string()))
    }

    async fn respond_to_server_invite(&self, _server_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("lemmy: respond_to_server_invite not implemented".to_string()))
    }

    async fn invite_user_to_server(&self, _server_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("lemmy: invite_user_to_server not implemented".to_string()))
    }
}

// ── H.4.c — DiscoverBackend ──────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DiscoverBackend for LemmyClient {
    async fn search_communities(
        &self,
        query: &str,
        scope: CommunityScope,
        cursor: Option<String>,
    ) -> ClientResult<CommunityPage> {
        let listing_type = match scope {
            CommunityScope::Subscribed => "Subscribed",
            CommunityScope::Local => "Local",
            CommunityScope::All => "All",
        };
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy: not authenticated".to_string())
        })?;
        let account_id = session.user_id.to_string();
        let account_display_name = session.user_display_name.clone();
        let resp = self.http.search_communities(
            query,
            listing_type,
            cursor.as_deref(),
        ).await?;

        // Lemmy returns exactly `limit` items (50) when a full page exists.
        // Next page cursor is the 1-based page number incremented as a string.
        let current_page: u32 = cursor
            .as_deref()
            .and_then(|c| c.parse().ok())
            .unwrap_or(1u32);
        let next_cursor = if resp.communities.len() == 50 {
            Some((current_page + 1).to_string())
        } else {
            None
        };

        let items = resp
            .communities
            .iter()
            .map(|view| map_community_to_server(view, &account_id, &account_display_name))
            .collect();

        Ok(CommunityPage { items, next_cursor })
    }
}

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for LemmyClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::PerServer,
            section_key: "community".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "mute-community".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
                    extra: String::new(),
                },
                SettingDescriptor {
                    key: "show-nsfw".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
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
impl poly_client::ViewDescriptorBackend for LemmyClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Communities,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        let sort_value = match action_id {
            "sort-hot" => "Hot",
            "sort-active" => "Active",
            "sort-scaled" => "Scaled",
            "sort-controversial" => "Controversial",
            "sort-new" => "New",
            "sort-old" => "Old",
            "sort-most-comments" => "MostComments",
            "sort-new-comments" => "NewComments",
            "sort-top" | "sort-top-day" => "TopDay",
            "sort-top-hour" => "TopHour",
            "sort-top-six-hours" => "TopSixHour",
            "sort-top-twelve-hours" => "TopTwelveHour",
            "sort-top-week" => "TopWeek",
            "sort-top-month" => "TopMonth",
            "sort-top-year" => "TopYear",
            "sort-top-all" => "TopAll",
            _ => {
                return Err(ClientError::NotFound(format!(
                    "unknown sidebar action: {action_id}"
                )));
            }
        };
        self.settings_storage.set(
            SettingsScope::AccountGlobal,
            "",
            "current-sort",
            sort_value,
        )?;
        Ok(ActionOutcome::RefreshTarget)
    }

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-lemmy-overview-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        if Self::parse_comments_channel(channel_id).is_some() {
            return Ok(ViewDescriptor {
                kind: ViewKind::FlatList,
                header: Some(ViewHeader {
                    title_key: Some("plugin-lemmy-view-comments-title".to_string()),
                    subtitle_key: None,
                    info_block: None,
                }),
                toolbar: None,
                body: ViewBody::ListBody(ListSpec {
                    row_template: RowTemplate {
                        primary_field: "text".to_string(),
                        secondary_field: Some("author".to_string()),
                        meta_field: None,
                        icon_field: None,
                    },
                    page_size: 50,
                }),
            });
        }
        Ok(ViewDescriptor {
            kind: ViewKind::Tree,
            header: Some(ViewHeader {
                title_key: Some("plugin-lemmy-view-posts-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![],
                filter_options: vec![],
                tabs: vec![],
                action_items: vec![],
            }),
            body: ViewBody::TreeBody(TreeSpec {
                root_page_size: 25,
                max_depth: 8,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if channel_id.is_empty() || channel_id == "lemmy-overview" {
            let resp = self.http.fetch_subscribed_communities().await?;
            let rows: Vec<ViewRow> = resp
                .communities
                .iter()
                .map(|view| map_community_to_viewrow(view, 0))
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        if let Some(community_id) = Self::parse_comments_channel(channel_id) {
            let limit: u32 = 50;
            let resp = self.http.fetch_community_comments(community_id, limit).await?;
            let rows: Vec<ViewRow> = resp.comments.iter().map(|view| {
                let msg = map_comment_to_message(view);
                let content_text = match &msg.content {
                    MessageContent::Text(s) => s.clone(),
                    MessageContent::WithAttachments { text, .. } => text.clone(),
                };
                ViewRow {
                    id: msg.id.clone(),
                    primary_text: content_text,
                    secondary_text: Some(msg.author.display_name.clone()),
                    meta_text: None,
                    icon: msg.author.avatar_url.clone(),
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Message,
                    preview_image_url: None,
                    is_video: false,
                }
            }).collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        let community_id = Self::parse_feed_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "get_view_rows: channel must be a lemmy-feed-{{id}} or lemmy-overview: {channel_id}"
            ))
        })?;

        let page = cursor_to_page(cursor.as_ref());
        let stored_sort = self.settings_storage.get(
            SettingsScope::AccountGlobal,
            "",
            "current-sort",
        );
        let sort: &str = sort_id
            .or(stored_sort.as_deref())
            .unwrap_or("Hot");
        let page_size: u32 = 25;

        let resp = self
            .http
            .fetch_posts_paged(community_id, sort, page, page_size)
            .await?;

        let now = chrono::Utc::now();
        let render_previews = self.render_previews_enabled();
        let rows: Vec<ViewRow> = resp.posts.iter().map(|v| map_post_to_viewrow(v, now, render_previews)).collect();
        let next_cursor = next_page_cursor(page, page_size.try_into().unwrap_or(usize::MAX), rows.len());

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        let post_id = row_id
            .parse::<i64>()
            .ok()
            .or_else(|| {
                row_id
                    .rsplit('/')
                    .next()
                    .and_then(|last| last.parse::<i64>().ok())
            })
            .ok_or_else(|| {
                ClientError::NotFound(format!("get_view_detail: cannot parse row id: {row_id}"))
            })?;

        fn html_escape(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
        }

        let post_view = self.http.fetch_post(post_id).await?;
        let body = post_view.post.body.clone().unwrap_or_default();
        let url_line = post_view
            .post
            .url
            .as_deref()
            .map(|u| format!("<p><a href=\"{}\">{}</a></p>", html_escape(u), html_escape(u)))
            .unwrap_or_default();
        let sanitized_html = format!(
            "<h3>{}</h3>{}<p>{}</p>",
            html_escape(&post_view.post.name),
            url_line,
            html_escape(&body),
        );

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html,
                stylesheet: None,
                max_height_px: None,
            },
            comments_section: Some(TreeSpec {
                root_page_size: 25,
                max_depth: 8,
            }),
        })
    }
}

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ContextActionBackend for LemmyClient {
    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match target {
            MenuTargetKind::Server => {
                let subscribed = match Self::parse_community_id(target_id) {
                    Ok(cid) => match self.http.fetch_community(cid).await {
                        Ok(view) => view
                            .subscribed
                            .as_deref()
                            .is_some_and(|s| s == "Subscribed" || s == "Pending"),
                        Err(_) => false,
                    },
                    Err(_) => false,
                };

                let sub_item = if subscribed {
                    MenuItem {
                        id: "unsubscribe-community".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-lemmy-menu-unsubscribe-community-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "subscribe-community".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-lemmy-menu-subscribe-community-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };

                Ok(vec![
                    MenuItem {
                        id: "view-community".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-lemmy-menu-view-community-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    sub_item,
                    MenuItem {
                        id: "view-modlog".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-lemmy-menu-view-modlog-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "block-community".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-lemmy-menu-block-community-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Category
            | MenuTargetKind::Channel
            | MenuTargetKind::Dm
            | MenuTargetKind::Message
            | MenuTargetKind::User => Ok(Vec::new()),
        }
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "view-community" | "subscribe-community" | "view-modlog" | "block-community" => {
                Ok(ActionOutcome::Noop)
            }
            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![
            MenuItem {
                id: "upvote".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-lemmy-message-action-upvote-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "downvote".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-lemmy-message-action-downvote-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "report".to_string(),
                parent_id: None,
                slot: MenuSlot::BeforeLeave,
                label_key: "plugin-lemmy-message-action-report-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
        ])
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "upvote" | "downvote" | "report" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}
