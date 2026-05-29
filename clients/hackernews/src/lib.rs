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
mod dms_and_groups;
#[cfg(feature = "native")]
mod mapping;
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
use api::HnApiClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use mapping::{
    build_channels, build_server, hn_comment_to_message, hn_item_to_message,
    post_id_from_channel,
};
#[cfg(feature = "native")]
use poly_client::{
    IsBackend, Session, SettingsStorageCell, User, PresenceStatus, BackendType, AuthCredentials,
    ClientResult, PluginManifest, Server, ClientError, Channel, MessageQuery, Message,
    Notification, ClientEvent, BackendCapabilities, SignupMethod, MessageContent,
};
#[cfg(feature = "native")]
use std::pin::Pin;
#[cfg(feature = "native")]
use types::HnItem;

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
    pub(crate) api: HnApiClient,
    pub(crate) session: Option<Session>,
    /// In-memory settings storage (persists only for the session lifetime).
    pub(crate) settings_storage: SettingsStorageCell,
    /// Stored version override (None = use api::DEFAULT_CLIENT_VERSION).
    pub(crate) version_override: std::sync::Mutex<Option<String>>,
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
                let cookie: String = auth::login(self.api.http_client(), &self.api.ua(), &email, &password)
                    .await?;
                let mut session = self.named_session(&email);
                session.token = cookie;
                let session_out = session.clone();
                self.session = Some(session);
                Ok(session_out)
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

    // ── Writable messaging (plan-trait-split-readable-vs-writable) ──────────

    fn as_writable_messaging(&self) -> Option<&dyn poly_client::WritableMessagingBackend> {
        Some(self)
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
        let feed = types::HnFeed::from_channel_id(channel_id).ok_or_else(|| {
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

    // ── Social graph (H.3.b — SocialGraphBackend in social_graph.rs) ─────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // ── DMs and groups (H.3.c — DmsAndGroupsBackend in dms_and_groups.rs) ────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    // --- Notifications ---

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // --- Settings / Views — sub-traits in settings.rs / view_descriptor.rs ---

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

    fn backend_name(&self) -> &'static str {
        "Hacker News"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::READ_ONLY_FEED
    }

    // --- Client-provided UI surface (WP 1.D) ---

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

// ── get_comment_thread (BFS over HN comment trees) ───────────────────────────

#[cfg(feature = "native")]
impl HackerNewsClient {
    /// Fetch the full comment tree for a story using BFS, up to `limit` total
    /// comments. Each fetched comment records its parent ID so the UI can
    /// render nested threads correctly.
    pub(crate) async fn get_comment_thread(
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
        let mut collected: Vec<(HnItem, u64)> = Vec::new();
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

// ── send_message helpers ──────────────────────────────────────────────────────
//
// These functions decompose the three logical stages of `send_message` so
// each stage has a single reason to change (SRP) and future channel types
// only need to extend `require_post_channel` (Open/Closed).

#[cfg(feature = "native")]
/// Validate that a write-capable (non-empty token) session exists.
fn require_write_session(session: Option<&Session>) -> ClientResult<&Session> {
    let session = session.ok_or_else(|| {
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
    Ok(session)
}

#[cfg(feature = "native")]
/// Map a channel ID to the numeric HN item ID it wraps, or return a
/// `NotSupported` error for channels that don't accept comments yet.
fn require_post_channel(channel_id: &str) -> ClientResult<u64> {
    post_id_from_channel(channel_id).ok_or_else(|| {
        ClientError::NotSupported(format!(
            "Posting from this channel is not supported yet (channel: {channel_id})"
        ))
    })
}

#[cfg(feature = "native")]
/// Accept plain-text content, reject everything else with `NotSupported`.
///
/// HN comments are plain text + URLs; markdown / attachments are not
/// supported by the site form.
fn require_text_content(content: MessageContent) -> ClientResult<String> {
    match content {
        MessageContent::Text(s) => Ok(s),
        other @ MessageContent::WithAttachments { .. } => Err(ClientError::NotSupported(format!(
            "Hacker News comments only accept plain text (got: {other:?})"
        ))),
    }
}

#[cfg(feature = "native")]
/// Build an optimistic placeholder `Message` for a just-posted comment.
///
/// HN doesn't return the new item ID. The placeholder is surfaced in the
/// UI immediately; the real comment appears on the next channel reload.
fn build_pending_message(session: &Session, text: String) -> Message {
    let now = chrono::Utc::now();
    Message {
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
    }
}

// ── WritableMessagingBackend (plan-trait-split-readable-vs-writable) ─────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableMessagingBackend for HackerNewsClient {
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let session = require_write_session(self.session.as_ref())?;
        let parent_id = require_post_channel(channel_id)?;
        let text = require_text_content(content)?;

        let http = self.api.http_client();
        let ua = self.api.ua();
        let cookie = &session.token;

        let hmac = auth::fetch_reply_hmac(http, &ua, parent_id, cookie).await?;
        auth::post_comment(http, &ua, parent_id, &text, cookie, &hmac).await?;

        Ok(build_pending_message(session, text))
    }
}
