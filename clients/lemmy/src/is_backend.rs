//! `impl IsBackend for LemmyClient` — authentication, plugin manifest,
//! server/channel/message basics, capability declaration, event stream.
//!
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::{
    AuthCredentials, BackendCapabilities, BackendType, Channel, ClientError, ClientEvent,
    ClientResult, CommunitySearchSupport, IsBackend, Mechanism, Message, MessageQuery,
    Notification, PluginManifest, Server, Session, SettingsScope, SignupMethod, User,
};
use std::pin::Pin;

use crate::LemmyClient;
use crate::api::{
    self, LemmyPerson, LemmySession, community_to_channel, map_comment_to_message,
    map_community_to_server, map_person, map_post_to_message,
};

impl LemmyClient {
    /// Resolve `(LemmySession, Session)` for the just-authenticated user.
    ///
    /// Side-effect: stores the session on `self.http` so subsequent calls use
    /// the new JWT. Both `AuthCredentials::EmailPassword` and `Token(...)`
    /// arms converge here once a `LemmyPerson` has been retrieved — collapses
    /// the three-arm duplication that previously lived in `authenticate`.
    ///
    /// DIP: callers no longer know how `Person → Session` projection works.
    fn finalize_session(&self, person: &LemmyPerson, jwt: String) -> Session {
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
        Session {
            id: format!("lemmy-session-{}", person.id),
            user: map_person(person),
            token: jwt,
            backend: BackendType::from(crate::SLUG),
            icon_emoji: None,
            instance_id,
            backend_url: Some(self.base_url().to_string()),
        }
    }

    /// Stash a placeholder session so `fetch_site` has a JWT to send.
    ///
    /// The user_id / display_name / avatar fields are zeroed and immediately
    /// overwritten by `finalize_session` once the site response lands.
    fn prime_placeholder_session(&self, jwt: &str) {
        self.http.set_session(LemmySession {
            jwt: jwt.to_string(),
            user_id: 0,
            user_display_name: String::new(),
            user_avatar_url: None,
        });
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for LemmyClient {
    // ── Authentication ──────────────────────────────────────────────────────

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        // Resolve `(jwt, missing_user_err)` from the credential variant.
        let (jwt, missing_user_err) = match credentials {
            AuthCredentials::EmailPassword { email, password } => {
                let login_resp = self.http.login(&email, &password).await?;
                let jwt = login_resp.jwt.ok_or_else(|| {
                    ClientError::AuthFailed(
                        "Lemmy login succeeded but no JWT was returned \
                         (may require email verification)"
                            .to_string(),
                    )
                })?;
                (jwt, "Login OK but site returned no user info")
            }
            AuthCredentials::Token(jwt) => (
                jwt,
                "JWT is invalid or expired (no my_user in site response)",
            ),
            other @ (AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. }) => {
                return Err(ClientError::AuthFailed(format!(
                    "Lemmy does not support {:?} credentials",
                    std::mem::discriminant(&other)
                )));
            }
        };

        // Common tail: prime placeholder → fetch_site → finalize.
        self.prime_placeholder_session(&jwt);
        let site = self.http.fetch_site().await?;
        let person = site
            .my_user
            .ok_or_else(|| ClientError::AuthFailed(missing_user_err.to_string()))?
            .local_user_view
            .person;

        Ok(self.finalize_session(&person, jwt))
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

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    // ── Writable messaging (plan-trait-split-readable-vs-writable) ──────────

    fn as_writable_messaging(&self) -> Option<&dyn poly_client::WritableMessagingBackend> {
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

    fn backend_name(&self) -> &'static str {
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
