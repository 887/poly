//! # poly-hackernews
//!
//! Hacker News client for Poly — read-only forum backend.
//!
//! Implements [`poly_client::IsBackend`] using the public HN Firebase API
//! at `https://hacker-news.firebaseio.com/v0/`.
//!
//! HN requires no authentication for reading. The backend always provides a
//! guest session and returns stories as `Forum`-type channel messages.

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "hackernews";

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
pub mod auth;
#[cfg(feature = "native")]
mod cache;
#[cfg(feature = "native")]
mod mapping;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
mod types;

#[cfg(feature = "native")]
use api::HnApiClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use mapping::{
    build_channels, build_server, hn_comment_to_message, hn_item_to_message, hn_item_to_overview_row,
    hn_item_to_view_row, hn_user_to_user, post_id_from_channel,
};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;
#[cfg(feature = "native")]
use types::HnFeed;

/// Return FTL translation source for the HN client plugin.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Hacker News read-only client.
#[cfg(feature = "native")]
pub struct HackerNewsClient {
    api: HnApiClient,
    session: Option<Session>,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
    /// Stored version override (None = use api::DEFAULT_CLIENT_VERSION).
    version_override: std::sync::Mutex<Option<String>>,
}

#[cfg(feature = "native")]
impl HackerNewsClient {
    /// Create a new HN client using the official Firebase API.
    #[must_use]
    pub fn new() -> Self {
        Self {
            api: HnApiClient::new(),
            session: None,
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Create a new HN client with a custom base URL (for tests).
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            api: HnApiClient::with_base_url(base_url.into()),
            session: None,
            settings_storage: SettingsStorageCell::new(),
            version_override: std::sync::Mutex::new(None),
        }
    }

    /// Build a named session for a named HN user.
    pub fn named_session(&mut self, username: &str) -> Session {
        let session = Session {
            id: format!("hn-{username}"),
            user: User {
                id: username.to_string(),
                display_name: username.to_string(),
                avatar_url: Some("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 40'%3E%3Crect width='40' height='40' rx='8' fill='%23ff6600'/%3E%3Ctext x='20' y='27' font-family='sans-serif' font-size='15' font-weight='bold' text-anchor='middle' fill='white'%3EHN%3C/text%3E%3C/svg%3E".to_string()),
                presence: PresenceStatus::Offline,
                backend: BackendType::from(crate::SLUG),
            },
            token: username.to_string(),
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("🔶".to_string()),
            instance_id: "news.ycombinator.com".to_string(),
            backend_url: Some("https://hacker-news.firebaseio.com".to_string()),
        };
        self.session = Some(session.clone());
        session
    }

    /// Build a guest session (no auth required).
    pub fn guest_session(&mut self) -> Session {
        let session = Session {
            id: "hn-anonymous".to_string(),
            user: User {
                id: "anonymous".to_string(),
                display_name: "Anonymous".to_string(),
                avatar_url: Some("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 40'%3E%3Crect width='40' height='40' rx='8' fill='%23ff6600'/%3E%3Ctext x='20' y='27' font-family='sans-serif' font-size='15' font-weight='bold' text-anchor='middle' fill='white'%3EHN%3C/text%3E%3C/svg%3E".to_string()),
                presence: PresenceStatus::Offline,
                backend: BackendType::from(crate::SLUG),
            },
            token: String::new(),
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("🔶".to_string()),
            instance_id: "news.ycombinator.com".to_string(),
            backend_url: Some("https://hacker-news.firebaseio.com".to_string()),
        };
        self.session = Some(session.clone());
        session
    }
}

#[cfg(feature = "native")]
impl Default for HackerNewsClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl IsBackend for HackerNewsClient {
    // --- Authentication ---

    /// HN supports two modes:
    /// - **Anonymous read-only** — pass any unrecognised variant (or no
    ///   credentials at all) and you get a guest session. Read API works,
    ///   write APIs return `NotSupported`.
    /// - **Logged-in** — pass `EmailPassword { email: <hn username>, password }`.
    ///   We POST to `news.ycombinator.com/login`, capture the `user` cookie
    ///   into `Session.token`, and use it for write requests
    ///   (comments, submissions). Multiple accounts are managed by the host
    ///   spawning multiple `HackerNewsClient` instances.
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        match credentials {
            AuthCredentials::EmailPassword { email, password } if !email.is_empty() => {
                let cookie = auth::login(self.api.http_client(), &self.api.ua(), &email, &password)
                    .await?;
                let mut session = self.named_session(&email);
                session.token = cookie.clone();
                self.session = Some(session.clone());
                Ok(session)
            }
            // Anonymous fallback (Token(""), OAuth{token:""}, or anything else
            // we don't have a real login flow for) — guest session, read-only.
            AuthCredentials::Token(_)
            | AuthCredentials::EmailPassword { .. }
            | AuthCredentials::OAuth { .. }
            | AuthCredentials::DeviceCode { .. }
            | AuthCredentials::PolyServer { .. } => Ok(self.guest_session()),
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        // Both guest sessions and logged-in sessions count as "authenticated"
        // for the purpose of having a session at all. Write-capability is a
        // separate check based on `session.token`.
        self.session.is_some()
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![
                "hacker-news.firebaseio.com".to_string(),
                "news.ycombinator.com".to_string(),
            ],
            description: "Hacker News client. Anonymous read-only browsing of \
                          top stories, Ask HN, Show HN, and job posts via the \
                          Firebase API; signed-in accounts can comment and \
                          submit via news.ycombinator.com."
                .to_string(),
            homepage: Some("https://news.ycombinator.com".to_string()),
        }
    }

    // --- Servers ---

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.session.as_ref().map_or("hn-anonymous", |s| s.id.as_str());
        Ok(vec![build_server(account_id)])
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        if id == "hn" {
            let account_id = self.session.as_ref().map_or("hn-anonymous", |s| s.id.as_str());
            Ok(build_server(account_id))
        } else {
            Err(ClientError::NotFound(format!("server not found: {id}")))
        }
    }

    // --- Channels ---

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        if server_id == "hn" {
            Ok(build_channels())
        } else {
            Err(ClientError::NotFound(format!(
                "server not found: {server_id}"
            )))
        }
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        build_channels()
            .into_iter()
            .find(|ch| ch.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("channel not found: {id}")))
    }

    // --- Messages ---

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        // Need a logged-in session: guest sessions have an empty token.
        let session = self.session.as_ref().ok_or_else(|| {
            ClientError::AuthFailed(
                "Sign in with your news.ycombinator.com account to post comments.".to_string(),
            )
        })?;
        if session.token.is_empty() {
            return Err(ClientError::AuthFailed(
                "This is an anonymous Hacker News session — sign in with a \
                 news.ycombinator.com account to post comments."
                    .to_string(),
            ));
        }

        // Channel must be a post comment thread (`hn-post-{id}`); replying
        // to a specific comment is a future enhancement.
        let parent_id = post_id_from_channel(channel_id).ok_or_else(|| {
            ClientError::NotSupported(format!(
                "Posting from this channel is not supported yet (channel: {channel_id})"
            ))
        })?;

        let text = match content {
            MessageContent::Text(s) => s,
            // HN comments are plain text + URLs; markdown / attachments are
            // not supported by the site form.
            other => return Err(ClientError::NotSupported(format!(
                "Hacker News comments only accept plain text (got: {other:?})"
            ))),
        };

        let http = self.api.http_client();
        let ua = self.api.ua();
        let cookie = &session.token;

        let hmac = auth::fetch_reply_hmac(http, &ua, parent_id, cookie).await?;
        auth::post_comment(http, &ua, parent_id, &text, cookie, &hmac).await?;

        // HN doesn't return the new item ID. Fabricate a placeholder so the
        // host can render a "sent" optimistic message; the real comment
        // will surface on the next channel reload.
        let now = chrono::Utc::now();
        let _ = channel_id; // referenced via parent_id; kept to clarify intent
        Ok(Message {
            id: format!("hn-pending-{}", now.timestamp_millis()),
            author: User {
                id: session.user.id.clone(),
                display_name: session.user.display_name.clone(),
                avatar_url: session.user.avatar_url.clone(),
                presence: session.user.presence,
                backend: session.user.backend.clone(),
            },
            content: MessageContent::Text(text),
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
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let limit = usize::try_from(query.limit.unwrap_or(20)).unwrap_or(20);

        // Check if this is a post's comment thread channel (hn-post-{id}).
        // F6 — bump the comment-thread default from 20 (the feed-page default)
        // to 300 so the recursive BFS fetches enough rows to populate a real
        // discussion. Host can still pass an explicit query.limit to override.
        if let Some(post_id) = post_id_from_channel(channel_id) {
            let comment_limit = query
                .limit
                .map_or(300, |l| usize::try_from(l).unwrap_or(300));
            return self.get_comment_thread(post_id, comment_limit).await;
        }

        // Otherwise it's a story feed channel
        let feed = HnFeed::from_channel_id(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!("unknown channel: {channel_id}"))
        })?;

        let mut ids = self.api.get_feed_ids(feed).await?;

        // Apply pagination: if `before` is set, find the offset in the ID list
        if let Some(ref before_id) = query.before
            && let Ok(before_num) = before_id.parse::<u64>()
            && let Some(pos) = ids.iter().position(|&id| id == before_num)
        {
            if let Some(tail) = ids.get(pos.saturating_add(1)..) {
                ids = tail.to_vec();
            } else {
                ids.clear();
            }
        }

        let ids: Vec<u64> = ids.into_iter().take(limit).collect();
        let items = self.api.get_items_batch(&ids).await?;

        let messages = items
            .iter()
            .filter(|item| !item.deleted.unwrap_or(false) && !item.dead.unwrap_or(false))
            .map(hn_item_to_message)
            .collect();

        Ok(messages)
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    // --- Notifications ---

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // --- Voice / Settings / Views / Context actions: moved to C.1 sub-traits below ---

    fn as_settings(&self) -> Option<&dyn poly_client::SettingsBackend> {
        Some(self)
    }

    fn as_view_descriptor(&self) -> Option<&dyn poly_client::ViewDescriptorBackend> {
        Some(self)
    }

    // --- Real-time events ---

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // HN has no WebSocket. Return an empty stream; callers can poll
        // `/v0/updates.json` separately if real-time updates are needed.
        Box::pin(stream::empty())
    }

    // --- Backend info ---

    fn backend_type(&self) -> BackendType {
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Hacker News"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::READ_ONLY_FEED
    }

    // --- Client-provided UI surface (WP 1.D) — sub-traits below ---

    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        // HN login page serves as registration too
        SignupMethod::External("https://news.ycombinator.com/login".into())
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
        self.api.set_user_agent(new_ua);
        Ok(())
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for HackerNewsClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let hn_user = self
            .api
            .get_user(id)
            .await?
            .ok_or_else(|| ClientError::NotFound(format!("user not found: {id}")))?;
        Ok(hn_user_to_user(&hn_user))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no friend system".to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no friend system".to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no friend system".to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no friend system".to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no user note system".to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews: block not supported via this interface".to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews: unblock not supported via this interface".to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no ignore concept".to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("HackerNews has no ignore concept".to_string()))
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        // HN has no presence concept. Returning Ok(Offline) used to lie to
        // the UI (presence dot would show grey "offline" forever); use
        // Unknown so the dot is suppressed entirely. set_presence already
        // returns NotSupported below — read/write are now consistent.
        Ok(PresenceStatus::Unknown)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no presence system".to_string()))
    }
}

// ── H.3.c — DmsAndGroupsBackend ───────────────────────────────────────────────
// Hacker News has no DM or group DM concept.

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::DmsAndGroupsBackend for HackerNewsClient {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported("Hacker News has no DM concept".to_string()))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported("Hacker News has no saved-messages concept".to_string()))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no group DMs".to_string()))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no group DMs".to_string()))
    }

    async fn add_users_to_group_dm(&self, _channel_id: &str, _user_ids: &[String]) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no group DMs".to_string()))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no DM concept".to_string()))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no conversation mute".to_string()))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no conversation mute".to_string()))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no group DMs".to_string()))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("Hacker News has no group DMs".to_string()))
    }
}

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for HackerNewsClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::AccountGlobal,
            section_key: "preferences".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "default-feed".to_string(),
                    kind: SettingKind::Select,
                    default_value: "\"top\"".to_string(),
                    extra: "[\"top\",\"new\",\"best\",\"ask\",\"show\",\"jobs\"]".to_string(),
                },
                SettingDescriptor {
                    key: "items-per-page".to_string(),
                    kind: SettingKind::Slider,
                    default_value: "30".to_string(),
                    extra: "{\"min\":10,\"max\":100,\"step\":5}".to_string(),
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
impl poly_client::ViewDescriptorBackend for HackerNewsClient {
    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Feed,
            sections: Vec::new(),
            header_block: None,
        })
    }

    /// HN account overview — show the top stories as a curated welcome view.
    /// HN has no concept of multiple servers/accounts beyond "the front page",
    /// so the overview is simply the current Top feed rendered as a ListBody.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: Some(ViewHeader {
                title_key: Some("plugin-hackernews-overview-title".to_string()),
                subtitle_key: Some("plugin-hackernews-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("author-domain".to_string()),
                    meta_field: Some("points-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 30,
            }),
        })
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: Some(ViewHeader {
                title_key: Some("plugin-hackernews-view-stories-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::ListBody(ListSpec {
                row_template: RowTemplate {
                    primary_field: "title".to_string(),
                    secondary_field: Some("url".to_string()),
                    meta_field: Some("score-comments-age".to_string()),
                    icon_field: None,
                },
                page_size: 30,
            }),
        })
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        let (feed, is_overview) = if channel_id.is_empty() {
            (HnFeed::Top, true)
        } else {
            let f = HnFeed::from_channel_id(channel_id).ok_or_else(|| {
                ClientError::NotFound(format!("unknown channel: {channel_id}"))
            })?;
            (f, false)
        };

        let offset: usize = cursor
            .as_ref()
            .and_then(|c| {
                if c.kind == CursorKind::Offset {
                    c.value.parse().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let page_size: usize = 30;

        let ids = self.api.get_feed_ids(feed).await?;
        let slice: Vec<u64> = ids
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect();

        let next_cursor = if slice.len() == page_size {
            Some(Cursor {
                kind: CursorKind::Offset,
                value: offset.saturating_add(page_size).to_string(),
            })
        } else {
            None
        };

        let items = self.api.get_items_batch(&slice).await?;

        let rows = items
            .iter()
            .filter(|item| !item.deleted.unwrap_or(false) && !item.dead.unwrap_or(false))
            .map(|item| {
                if is_overview {
                    hn_item_to_overview_row(item)
                } else {
                    hn_item_to_view_row(item)
                }
            })
            .collect();

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        let story_id: u64 = row_id.parse().map_err(|_e| {
            ClientError::NotFound(format!("invalid story id: {row_id}"))
        })?;

        let story = self
            .api
            .get_item(story_id)
            .await?
            .ok_or_else(|| ClientError::NotFound(format!("story not found: {story_id}")))?;

        let body_html = if let Some(ref text) = story.text {
            format!("<p>{text}</p>")
        } else if let Some(ref url) = story.url {
            let title = story.title.as_deref().unwrap_or("Link");
            format!("<p><a href=\"{url}\">{title}</a></p>")
        } else {
            let title = story.title.as_deref().unwrap_or("(no title)");
            format!("<p>{title}</p>")
        };

        let has_comments = story.kids.as_ref().is_some_and(|k| !k.is_empty());
        let comments_section = if has_comments {
            Some(poly_client::TreeSpec {
                root_page_size: 30,
                max_depth: 8,
            })
        } else {
            None
        };

        Ok(ViewDetail {
            body_block: CustomBlock {
                sanitized_html: body_html,
                stylesheet: None,
                max_height_px: None,
            },
            comments_section,
        })
    }
}

#[cfg(feature = "native")]
impl HackerNewsClient {
    /// Fetch the full comment tree for a story using BFS, up to `limit` total
    /// comments. Each fetched comment records its parent ID so the UI can
    /// render nested threads correctly.
    async fn get_comment_thread(
        &self,
        story_id: u64,
        limit: usize,
    ) -> ClientResult<Vec<Message>> {
        let story = self
            .api
            .get_item(story_id)
            .await?
            .ok_or_else(|| ClientError::NotFound(format!("story not found: {story_id}")))?;

        let top_kids = story.kids.unwrap_or_default();
        if top_kids.is_empty() {
            return Ok(Vec::new());
        }

        // BFS queue: (item_id, parent_id). Top-level comments parent = story_id.
        let mut queue: Vec<(u64, u64)> = top_kids
            .into_iter()
            .map(|id| (id, story_id))
            .collect();

        // Collected (item, parent_id) pairs. F6 — raise BFS ceiling from 300
        // to 1000 so deep HN threads (which routinely run 500+ items) render
        // fully instead of truncating mid-conversation. Caller's `limit` still
        // wins when it's smaller (host supplies query.limit per page).
        let mut collected: Vec<(types::HnItem, u64)> = Vec::new();
        let max = limit.clamp(1, 1000);

        while !queue.is_empty() && collected.len() < max {
            let remaining = max.saturating_sub(collected.len());
            let batch_pairs: Vec<(u64, u64)> = queue
                .drain(..queue.len().min(remaining))
                .collect();
            let ids: Vec<u64> = batch_pairs.iter().map(|(id, _)| *id).collect();
            let id_to_parent: std::collections::HashMap<u64, u64> =
                batch_pairs.into_iter().collect();

            let items = self.api.get_items_batch(&ids).await?;
            for item in items {
                // Enqueue this item's children for the next BFS round.
                if let Some(kids) = &item.kids {
                    for &kid in kids {
                        queue.push((kid, item.id));
                    }
                }
                let parent = id_to_parent.get(&item.id).copied().unwrap_or(story_id);
                collected.push((item, parent));
            }
        }

        let messages = collected
            .iter()
            .map(|(item, parent)| {
                hn_comment_to_message(
                    item,
                    if *parent == story_id { None } else { Some(*parent) },
                    story_id,
                )
            })
            .collect();
        Ok(messages)
    }
}
