//! [`ClientBackend`] impl for [`RedditClient`].
//!
//! Wires the HTML-scraping Reddit client into the Poly trait surface so the
//! UI can render Reddit accounts alongside Lemmy, Discord, etc.
//!
//! # ID mapping
//!
//! | Reddit concept | Poly ID |
//! |---|---|
//! | Subreddit `<sub>` | Server `r_<sub>`, Channel `c_posts_<sub>` |
//! | Post `<id>` (t3) | Message `t3_<id>` |
//! | Comment `<id>` (t1) | Message `t1_<id>` |
//! | DM `<id>` (t4) | DmChannel `dm_<id>`, Message `t4_<id>` |
//! | Username `<name>` | User `u_<name>` |

use async_trait::async_trait;
use futures::stream::{self, Stream};
use poly_client::*;
use std::pin::Pin;

use crate::{RedditClient, RedditError, SortKind};
use crate::parser::{RawDm, RawPost, UserProfile};

// ─── ID helpers ─────────────────────────────────────────────────────────────

fn server_id_for_sub(sub: &str) -> String {
    format!("r_{sub}")
}

fn sub_from_server_id(id: &str) -> Option<&str> {
    id.strip_prefix("r_")
}

fn channel_id_for_sub(sub: &str) -> String {
    format!("c_posts_{sub}")
}

fn sub_from_channel_id(id: &str) -> Option<&str> {
    id.strip_prefix("c_posts_")
}

fn message_id_for_post(post_id: &str) -> String {
    format!("t3_{post_id}")
}

fn _message_id_for_comment(comment_id: &str) -> String {
    format!("t1_{comment_id}")
}

fn message_id_for_dm(dm_id: &str) -> String {
    format!("t4_{dm_id}")
}

fn dm_channel_id_for_dm(dm_id: &str) -> String {
    format!("dm_{dm_id}")
}

fn _dm_id_from_channel_id(id: &str) -> Option<&str> {
    id.strip_prefix("dm_")
}

fn user_id_for_name(name: &str) -> String {
    format!("u_{name}")
}

// ─── Error mapping ───────────────────────────────────────────────────────────

impl From<RedditError> for ClientError {
    fn from(e: RedditError) -> Self {
        match e {
            RedditError::LoggedOut => {
                ClientError::AuthFailed("Session cookie missing or expired".to_string())
            }
            RedditError::Status(401) | RedditError::Status(403) => {
                ClientError::AuthFailed(format!("HTTP {}", e))
            }
            RedditError::Status(404) => ClientError::NotFound(e.to_string()),
            RedditError::Http(s) => ClientError::Network(s),
            RedditError::Parse(p) => ClientError::Internal(p.to_string()),
            RedditError::Status(s) => ClientError::Network(format!("HTTP {s}")),
        }
    }
}

// ─── Mapping helpers ─────────────────────────────────────────────────────────

fn raw_post_to_message(post: &RawPost, backend: &BackendType) -> Message {
    let content = if let Some(body) = &post.body {
        if !body.is_empty() {
            MessageContent::Text(format!("{}\n\n{}", post.title, body))
        } else {
            MessageContent::Text(post.title.clone())
        }
    } else if let Some(url) = &post.url {
        MessageContent::Text(format!("{}\n\n{}", post.title, url))
    } else {
        MessageContent::Text(post.title.clone())
    };

    // Add an attachment for image previews so the message view can render them.
    // For video posts we use the preview thumbnail (if available) and mark the
    // attachment content-type as video/mp4 as a hint. Galleries get a single
    // cover-image attachment.
    let mut attachments = Vec::new();
    if let Some(ref preview) = post.preview_url {
        let (content_type, filename) = if post.is_video {
            ("video/mp4", "video_preview.jpg")
        } else {
            ("image/png", "preview.png")
        };
        attachments.push(Attachment::remote(
            format!("reddit-preview-{}", post.id),
            filename.to_string(),
            content_type.to_string(),
            preview.clone(),
            0,
        ));
    }

    Message {
        id: message_id_for_post(&post.id),
        author: User {
            id: user_id_for_name(&post.author),
            display_name: post.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        content,
        timestamp: post.timestamp,
        attachments,
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: post.preview_url.clone(),
    }
}

fn raw_dm_to_dm_channel(dm: &RawDm, account_id: &str, backend: &BackendType) -> DmChannel {
    let last_message = Message {
        id: message_id_for_dm(&dm.id),
        author: User {
            id: user_id_for_name(&dm.author),
            display_name: dm.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        content: MessageContent::Text(dm.subject.clone()),
        timestamp: dm.timestamp,
        attachments: Vec::new(),
        reactions: Vec::new(),
        reply_to: None,
        edited: false,
        thread: None,
        preview_image_url: None,
    };

    DmChannel {
        id: dm_channel_id_for_dm(&dm.id),
        user: User {
            id: user_id_for_name(&dm.author),
            display_name: dm.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: backend.clone(),
        },
        last_message: Some(last_message),
        unread_count: 0,
        backend: backend.clone(),
        account_id: account_id.to_string(),
    }
}

fn user_profile_to_user(profile: &UserProfile, backend: &BackendType) -> User {
    User {
        id: user_id_for_name(&profile.name),
        display_name: profile.name.clone(),
        avatar_url: profile.avatar_url.clone(),
        presence: PresenceStatus::Offline,
        backend: backend.clone(),
    }
}

fn raw_post_to_viewrow(post: &RawPost, show_previews: bool) -> ViewRow {
    let secondary = format!("by u/{}", post.author);
    let preview_image_url = if show_previews { post.preview_url.clone() } else { None };

    ViewRow {
        id: message_id_for_post(&post.id),
        primary_text: post.title.clone(),
        secondary_text: Some(secondary),
        // SCORE: prefix is load-bearing for the forum-post-card render path in
        // list_body.rs — ListBodyRow renders the vote-card shape when it appears.
        meta_text: Some(format!("SCORE:{} · {} comments", post.score, post.comment_count)),
        icon: None,
        badge: None,
        context_menu_target_kind: MenuTargetKind::Message,
        preview_image_url,
        is_video: post.is_video,
    }
}

fn build_sub_server(
    sub: &str,
    account_id: &str,
    account_display_name: &str,
    backend: &BackendType,
) -> Server {
    Server {
        id: server_id_for_sub(sub),
        name: format!("r/{sub}"),
        icon_url: None,
        banner_url: None,
        categories: vec![Category {
            id: format!("cat_{sub}"),
            name: "Channels".to_string(),
            channel_ids: vec![channel_id_for_sub(sub)],
        }],
        backend: backend.clone(),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id: Some(channel_id_for_sub(sub)),
        description: None,
        star_count: None,
        language: None,
        forks_count: None,
        open_issues_count: None,
    }
}

fn build_sub_channel(sub: &str) -> Channel {
    Channel {
        id: channel_id_for_sub(sub),
        name: "posts".to_string(),
        channel_type: ChannelType::Forum,
        server_id: server_id_for_sub(sub),
        unread_count: 0,
        mention_count: 0,
        last_message_id: None,
        forum_tags: None,
        parent_channel_id: None,
        thread_metadata: None,
    }
}

// ─── State storage for session ───────────────────────────────────────────────

/// `ClientBackend` adapter wrapping a `RedditClient` + optional session.
pub struct RedditBackend {
    client: RedditClient,
    session: Option<Session>,
    /// In-memory settings storage (mirrors Lemmy's stub pattern, Phase 4).
    settings_storage: SettingsStorageCell,
}

impl RedditBackend {
    /// Create a new backend from an already-constructed `RedditClient`.
    pub fn new(client: RedditClient) -> Self {
        Self { client, session: None, settings_storage: SettingsStorageCell::new() }
    }

    /// Read the `show-media-previews` mechanism state.
    ///
    /// Defaults to `true` (previews shown) when the user has never toggled it.
    fn media_previews_enabled(&self) -> bool {
        self.settings_storage
            .get(SettingsScope::AccountGlobal, "", "show-media-previews")
            .is_none_or(|v| v != "false")
    }

    fn backend_type() -> BackendType {
        BackendType::from("reddit")
    }

    fn account_id(&self) -> &str {
        self.session.as_ref().map_or("reddit-anon", |s| s.id.as_str())
    }

    fn account_display_name(&self) -> &str {
        self.session
            .as_ref()
            .map_or("Anonymous", |s| s.user.display_name.as_str())
    }

    /// Build a `Session` for the given username.
    fn build_session(&self, username: &str) -> Session {
        // `token` is what gets persisted to KV and replayed via
        // `authenticate(Token(t))` on next app boot. It MUST be the
        // session-cookie value captured during login_with_password, not
        // the bare username — otherwise restore re-authenticates with
        // a string the server doesn't recognise as a session.
        // Falls back to username if (somehow) login didn't capture a
        // session — caller can still re-login with password from the UI.
        let token = self
            .client
            .session_cookie_value()
            .unwrap_or_else(|| username.to_string());
        Session {
            id: format!("reddit-{username}"),
            user: User {
                id: user_id_for_name(username),
                display_name: username.to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: Self::backend_type(),
            },
            token,
            backend: Self::backend_type(),
            icon_emoji: Some("🤖".to_string()),
            instance_id: "old.reddit.com".to_string(),
            backend_url: Some(self.client.base_url().to_string()),
        }
    }
}

// ─── ClientBackend impl ───────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for RedditBackend {
    // ── Authentication ───────────────────────────────────────────────────────

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        match credentials {
            AuthCredentials::EmailPassword { email, password } => {
                // Reddit's "email" field carries the username.
                self.client
                    .login_with_password(&email, &password)
                    .await
                    .map_err(ClientError::from)?;
                let session = self.build_session(&email);
                self.session = Some(session.clone());
                Ok(session)
            }
            AuthCredentials::Token(cookie) => {
                self.client
                    .login_with_session_cookie(&cookie)
                    .map_err(ClientError::from)?;
                // Probe who we are.
                let username = self
                    .client
                    .is_logged_in()
                    .await
                    .map_err(ClientError::from)?
                    .unwrap_or_else(|| "me".to_string());
                let session = self.build_session(&username);
                self.session = Some(session.clone());
                Ok(session)
            }
            AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. } => Err(ClientError::NotSupported(
                "Reddit only supports EmailPassword and Token credentials".to_string(),
            )),
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    // ── Servers (subreddits) ─────────────────────────────────────────────────

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        // Delegates to RedditClient::list_subscribed_subreddits which sends
        // the manual session header (X-Mock-Session for browser fetch +
        // Cookie for native). Calling self.client.http().get(...) directly
        // here would skip the auth header and always come back empty.
        let subs = self.client.list_subscribed_subreddits().await?;
        let account_id = self.account_id();
        let account_display_name = self.account_display_name();
        let bt = Self::backend_type();
        Ok(subs
            .iter()
            .map(|sub| {
                let mut server = build_sub_server(&sub.name, account_id, account_display_name, &bt);
                if let Some(url) = &sub.icon_url {
                    server.icon_url = Some(url.clone());
                }
                server
            })
            .collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let sub = sub_from_server_id(id)
            .ok_or_else(|| ClientError::NotFound(format!("server not found: {id}")))?;
        Ok(build_sub_server(
            sub,
            self.account_id(),
            self.account_display_name(),
            &Self::backend_type(),
        ))
    }

    // ── Channels ─────────────────────────────────────────────────────────────

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let sub = sub_from_server_id(server_id)
            .ok_or_else(|| ClientError::NotFound(format!("server not found: {server_id}")))?;
        Ok(vec![build_sub_channel(sub)])
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let sub = sub_from_channel_id(id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {id}")))?;
        Ok(build_sub_channel(sub))
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    async fn send_message(
        &self,
        _channel_id: &str,
        _content: MessageContent,
    ) -> ClientResult<Message> {
        Err(ClientError::NotSupported(
            "reddit submit (top-level post) not yet implemented".to_string(),
        ))
    }

    async fn send_reply_message(
        &self,
        _channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match &content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        };

        // reply_to_message_id is t3_<id> for posts or t1_<id> for comments.
        let is_post = reply_to_message_id.starts_with("t3_");
        let is_comment = reply_to_message_id.starts_with("t1_");

        if is_post || is_comment {
            self.client
                .reply_comment(reply_to_message_id, &text)
                .await
                .map_err(ClientError::from)?;
        } else {
            return Err(ClientError::NotSupported(format!(
                "cannot reply to id: {reply_to_message_id}"
            )));
        }

        // Reddit's reply endpoint does not return the new comment ID.
        // Return a placeholder message so the host can show optimistic send.
        let now = chrono::Utc::now();
        let account_display = self.account_display_name().to_string();
        let bt = Self::backend_type();
        Ok(Message {
            id: format!("t1_pending-{}", now.timestamp_millis()),
            author: User {
                id: self
                    .session
                    .as_ref()
                    .map_or("u_me".to_string(), |s| s.user.id.clone()),
                display_name: account_display,
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: bt,
            },
            content,
            timestamp: now,
            attachments: Vec::new(),
            reactions: Vec::new(),
            reply_to: None,
            edited: false,
            thread: None,
            preview_image_url: None,
        })
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let sub = sub_from_channel_id(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {channel_id}")))?;

        let posts = self
            .client
            .list_subreddit(sub, SortKind::Hot)
            .await
            .map_err(ClientError::from)?;

        let bt = Self::backend_type();
        Ok(posts.iter().map(|p| raw_post_to_message(p, &bt)).collect())
    }

    // ── Users ─────────────────────────────────────────────────────────────────

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let name = id
            .strip_prefix("u_")
            .ok_or_else(|| ClientError::NotFound(format!("user not found: {id}")))?;

        let profile = self
            .client
            .get_user(name)
            .await
            .map_err(ClientError::from)?;

        Ok(user_profile_to_user(&profile, &Self::backend_type()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // ── Groups ───────────────────────────────────────────────────────────────

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    // ── DMs ──────────────────────────────────────────────────────────────────

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let dms = self.client.inbox().await.map_err(ClientError::from)?;
        let account_id = self.account_id();
        let bt = Self::backend_type();
        Ok(dms
            .iter()
            .map(|dm| raw_dm_to_dm_channel(dm, account_id, &bt))
            .collect())
    }

    // ── Notifications ─────────────────────────────────────────────────────────

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // ── Voice ─────────────────────────────────────────────────────────────────

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(Vec::new())
    }

    // ── Presence ─────────────────────────────────────────────────────────────

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Reddit has no presence system".to_string(),
        ))
    }

    // ── Real-time events ─────────────────────────────────────────────────────

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(stream::empty())
    }

    // ── Backend info ─────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::from("reddit")
    }

    fn backend_name(&self) -> &str {
        "Reddit"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            ..BackendCapabilities::MESSAGING_NO_SOCIAL
        }
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["old.reddit.com".to_string()],
            description:
                "Reddit client. Scrapes old.reddit.com HTML for posts and DMs. \
                 Anonymous and session-cookie auth supported."
                    .to_string(),
            homepage: Some("https://old.reddit.com".to_string()),
        }
    }

    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        SignupMethod::InApp("/signup/reddit".to_string())
    }

    // ── D-series UI extension (stubs matching hackernews pattern) ────────────

    async fn get_context_menu_items(
        &self,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(Vec::new())
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown action: {action_id}")))
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        // Reddit has no per-server / per-channel settings exposed yet.
        Ok(Vec::new())
    }

    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        if let Some(value) = self.settings_storage.get(scope, scope_id, key) {
            return Ok(value);
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
        self.settings_storage.set(scope, scope_id, key, value)
    }

    /// Declares the `show-media-previews` mechanism, which controls whether
    /// image/video thumbnail previews are rendered next to forum post titles.
    ///
    /// Default ON. Set to `false` to hide all preview thumbnails.
    ///
    /// TODO: add `navigator.connection.effectiveType` auto-disable for
    /// `slow-2g`/`2g` connections when the Web Connection API is available in
    /// WASM contexts.
    async fn client_mechanisms(&self) -> ClientResult<Vec<Mechanism>> {
        let enabled = self.media_previews_enabled();
        Ok(vec![Mechanism {
            id: "show-media-previews".to_string(),
            name_key: "plugin-reddit-mechanism-show-media-previews-label".to_string(),
            enabled,
            requires_host_cap: None,
            description_key: Some(
                "plugin-reddit-mechanism-show-media-previews-desc".to_string(),
            ),
        }])
    }

    /// Toggle the `show-media-previews` mechanism.
    async fn set_client_mechanism(&self, id: &str, enabled: bool) -> ClientResult<()> {
        match id {
            "show-media-previews" => self.settings_storage.set(
                SettingsScope::AccountGlobal,
                "",
                "show-media-previews",
                if enabled { "true" } else { "false" },
            ),
            _ => Err(ClientError::NotFound(format!("unknown mechanism: {id}"))),
        }
    }

    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Feed,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!(
            "unknown sidebar action: {action_id}"
        )))
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: None,
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("author".to_string()),
                    meta_field: Some("score-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 25,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        let sub = sub_from_channel_id(channel_id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {channel_id}")))?;

        let posts = self
            .client
            .list_subreddit(sub, SortKind::Hot)
            .await
            .map_err(ClientError::from)?;

        let show_previews = self.media_previews_enabled();

        let rows = posts
            .iter()
            .map(|p| raw_post_to_viewrow(p, show_previews))
            .collect();

        Ok(ViewRowsPage { rows, next_cursor: None })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotFound(format!("row not found: {row_id}")))
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
        Err(ClientError::NotFound(format!(
            "unknown composer action: {action_id}"
        )))
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!(
            "unknown message action: {action_id}"
        )))
    }
}
