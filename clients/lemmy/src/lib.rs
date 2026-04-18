//! # poly-lemmy
//!
//! Lemmy federated forum client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using the Lemmy REST API v3.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.

#![allow(clippy::if_same_then_else)]

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
    LemmyHttpClient, LemmySession, community_to_channel, map_comment_to_message,
    map_community_to_server, map_person, map_pm_to_dm_channel, map_post_to_message,
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
}

#[cfg(feature = "native")]
impl LemmyClient {
    /// Create a new Lemmy client pointed at `base_url` (e.g. `https://lemmy.ml`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: LemmyHttpClient::new(base_url),
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

    /// Return the currently stored user_id, if authenticated.
    fn current_user_id(&self) -> Option<i64> {
        self.http.session().map(|s| s.user_id)
    }

    /// Return (account_id, account_display_name) or an AuthFailed error.
    fn current_account_metadata(&self) -> ClientResult<(String, String)> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy client is not authenticated".to_string())
        })?;
        let account_id = format!("lemmy-user-{}", session.user_id);
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

    /// Extract a post_id integer from a `lemmy-post-{id}` channel/message ID.
    fn parse_post_channel(channel_id: &str) -> Option<i64> {
        channel_id
            .strip_prefix("lemmy-post-")
            .and_then(|s| s.parse::<i64>().ok())
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for LemmyClient {
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
                    backend: BackendType::from("lemmy"),
                    icon_emoji: None,
                    instance_id,
                    backend_url: Some(self.base_url().to_string()),
                });
            }
            other => {
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
            backend: BackendType::from("lemmy"),
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

    // ── Users ────────────────────────────────────────────────────────────────

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
                    backend: BackendType::from("lemmy"),
                });
            }
        }
        Err(ClientError::NotFound(format!("user not found: {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // Lemmy has no friends concept
        Ok(vec![])
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        // Lemmy communities don't expose a member list via the standard API
        Ok(vec![])
    }

    // ── Groups ────────────────────────────────────────────────────────────────

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        // Lemmy has no group DMs
        Ok(vec![])
    }

    // ── Direct Messages ───────────────────────────────────────────────────────

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let my_user_id = self.current_user_id().ok_or_else(|| {
            ClientError::AuthFailed("Lemmy client is not authenticated".to_string())
        })?;
        let (account_id, _) = self.current_account_metadata()?;

        let resp = self.http.fetch_private_messages().await?;

        // Group by conversation partner: keep only the most recent PM per partner.
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

    // ── Notifications ─────────────────────────────────────────────────────────

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    // ── Presence ─────────────────────────────────────────────────────────────

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Lemmy has no presence system".to_string(),
        ))
    }

    // ── Voice ─────────────────────────────────────────────────────────────────

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(vec![])
    }

    // ── Real-time events ──────────────────────────────────────────────────────

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // Lemmy v0.19+ removed WebSocket. Real-time requires polling.
        // For now return an empty stream; polling will be added in a later phase.
        Box::pin(stream::empty())
    }

    // ── Client UI surface (WP 1.D) ────────────────────────────────────────────

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match target {
            MenuTargetKind::Server => Ok(vec![
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
                MenuItem {
                    id: "subscribe-community".to_string(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-lemmy-menu-subscribe-community-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
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
            ]),
            _ => Ok(Vec::new()),
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

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

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

    async fn get_setting_value(
        &self,
        _scope: SettingsScope,
        _scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        // TODO(WP 3): wire to host-api.kv_get once exposed to this plugin.
        for section in self.get_settings_sections().await? {
            for field in section.fields {
                if field.key == key {
                    return Ok(field.default_value);
                }
            }
        }
        Err(ClientError::NotFound(format!("setting: {key}")))
    }

    async fn set_setting_value(
        &self,
        _scope: SettingsScope,
        _scope_id: &str,
        _key: &str,
        _value: &str,
    ) -> ClientResult<()> {
        // TODO(WP 3): wire to host-api.kv_set once exposed to this plugin.
        Err(ClientError::NotSupported("settings storage not yet wired".into()))
    }

    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Communities,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::Tree,
            header: Some(ViewHeader {
                title_key: Some("plugin-lemmy-view-posts-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![
                    ToolbarOption { id: "hot".to_string(), label_key: "plugin-lemmy-sort-hot".to_string(), icon: None, default_selected: true },
                    ToolbarOption { id: "new".to_string(), label_key: "plugin-lemmy-sort-new".to_string(), icon: None, default_selected: false },
                    ToolbarOption { id: "top".to_string(), label_key: "plugin-lemmy-sort-top".to_string(), icon: None, default_selected: false },
                ],
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
        _channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        // WP 5 initial: return empty page. Real Lemmy API integration is follow-up.
        Ok(ViewRowsPage { rows: Vec::new(), next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("view-detail not yet implemented".into()))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(Vec::new())
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(Vec::new())
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown composer action: {action_id}")))
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown message action: {action_id}")))
    }

    // ── Backend info ──────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::from("lemmy")
    }

    fn backend_name(&self) -> &str {
        "Lemmy"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            reactions: true,
            landing: poly_client::LandingPage::FirstServer,
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }
    }
}
