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
}

#[cfg(feature = "native")]
impl LemmyClient {
    /// Create a new Lemmy client pointed at `base_url` (e.g. `https://lemmy.ml`).
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: LemmyHttpClient::new(base_url),
            settings_storage: SettingsStorageCell::new(),
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
        raw.parse::<i64>().map_err(|_| {
            ClientError::NotFound(format!("invalid Lemmy member id: {member_id}"))
        })
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

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        self.http
            .put_community(community_id, banner_url)
            .await
            .map(|_| ())
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
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match target {
            MenuTargetKind::Server => {
                // Pack E.1 (P43): probe subscription state and pick between
                // Subscribe / Unsubscribe. Any lookup error falls back to
                // "Subscribe" — safer default (can't accidentally unsubscribe
                // someone with a stale menu).
                let subscribed = match Self::parse_community_id(target_id) {
                    Ok(cid) => match self.http.fetch_community(cid).await {
                        Ok(view) => view
                            .subscribed
                            .as_deref()
                            .map(|s| s == "Subscribed" || s == "Pending")
                            .unwrap_or(false),
                        // Lookup failed (network / auth): default to unsubscribed.
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
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        // Pack C P18: in-memory storage stub. TODO: migrate to
        // host-api.kv_get once exposed to plugins for true persistence.
        if let Some(value) = self.settings_storage.get(scope, scope_id, key) {
            return Ok(value);
        }
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
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()> {
        // Pack C P18: in-memory storage stub. TODO: migrate to
        // host-api.kv_set once exposed to plugins for true persistence.
        self.settings_storage.set(scope, scope_id, key, value)
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

    /// Account overview: a CardGrid of the user's subscribed communities.
    ///
    /// Uses the synthetic channel id `"lemmy-overview"`. `get_view_rows`
    /// recognises this id and fetches subscribed communities instead of posts.
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
        channel_id: &str,
        cursor: Option<Cursor>,
        sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        // The account overview uses a synthetic channel id that routes here
        // instead of to a community post feed.
        if channel_id.is_empty() || channel_id == "lemmy-overview" {
            let resp = self.http.fetch_subscribed_communities().await?;
            let rows: Vec<ViewRow> = resp
                .communities
                .iter()
                .map(|view| map_community_to_viewrow(view, 0))
                .collect();
            return Ok(ViewRowsPage { rows, next_cursor: None });
        }

        let community_id = Self::parse_feed_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "get_view_rows: channel must be a lemmy-feed-{{id}} or lemmy-overview: {channel_id}"
            ))
        })?;

        let page = cursor_to_page(cursor.as_ref());
        let sort = sort_id.unwrap_or("Hot");
        let page_size: u32 = 25;

        let resp = self
            .http
            .fetch_posts_paged(community_id, sort, page, page_size)
            .await?;

        let now = chrono::Utc::now();
        let rows: Vec<ViewRow> = resp.posts.iter().map(|v| map_post_to_viewrow(v, now)).collect();
        let next_cursor = next_page_cursor(page, page_size as usize, rows.len());

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        // row_id is either the post's integer id (from map_post_to_viewrow when
        // `ap_id` was absent) or the `ap_id` URL. Try integer first; if that
        // fails, extract the numeric suffix from a `.../post/{id}` URL.
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

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        // Lemmy is a read/vote platform — no freeform composer beyond post creation.
        Ok(Vec::new())
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
        match action_id {
            "upvote" | "downvote" | "report" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }

    // ── Moderation ────────────────────────────────────────────────────────────

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
            let moderator = e.moderator.as_ref().map(map_person).unwrap_or_else(|| User {
                id: "lemmy-user-unknown".to_string(),
                display_name: "Unknown".to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from("lemmy"),
            });
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
            let moderator = e.moderator.as_ref().map(map_person).unwrap_or_else(|| User {
                id: "lemmy-user-unknown".to_string(),
                display_name: "Unknown".to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from("lemmy"),
            });
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
            let moderator = e.moderator.as_ref().map(map_person).unwrap_or_else(|| User {
                id: "lemmy-user-unknown".to_string(),
                display_name: "Unknown".to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from("lemmy"),
            });
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

    // ── Backend info ──────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::from("lemmy")
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
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }
    }
}
