//! # poly-hackernews
//!
//! Hacker News client for Poly — read-only forum backend.
//!
//! Implements [`poly_client::ClientBackend`] using the public HN Firebase API
//! at `https://hacker-news.firebaseio.com/v0/`.
//!
//! HN requires no authentication for reading. The backend always provides a
//! guest session and returns stories as `Forum`-type channel messages.

#[cfg(feature = "native")]
mod api;
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
    build_channels, build_server, hn_comment_to_message, hn_item_to_message, hn_user_to_user,
    post_id_from_channel,
};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;
#[cfg(feature = "native")]
use types::HnFeed;

/// Return FTL translation source for the HN client plugin.
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
}

#[cfg(feature = "native")]
impl HackerNewsClient {
    /// Create a new HN client using the official Firebase API.
    #[must_use]
    pub fn new() -> Self {
        Self {
            api: HnApiClient::new(),
            session: None,
        }
    }

    /// Create a new HN client with a custom base URL (for tests).
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            api: HnApiClient::with_base_url(base_url.into()),
            session: None,
        }
    }

    /// Build a named session for a named HN user.
    pub fn named_session(&mut self, username: String) -> Session {
        let session = Session {
            id: format!("hn-{}", username),
            user: User {
                id: username.clone(),
                display_name: username.clone(),
                avatar_url: Some("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 40 40'%3E%3Crect width='40' height='40' rx='8' fill='%23ff6600'/%3E%3Ctext x='20' y='27' font-family='sans-serif' font-size='15' font-weight='bold' text-anchor='middle' fill='white'%3EHN%3C/text%3E%3C/svg%3E".to_string()),
                presence: PresenceStatus::Offline,
                backend: BackendType::from("hackernews"),
            },
            token: username.clone(),
            backend: BackendType::from("hackernews"),
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
                backend: BackendType::from("hackernews"),
            },
            token: String::new(),
            backend: BackendType::from("hackernews"),
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
impl ClientBackend for HackerNewsClient {
    // --- Authentication ---

    async fn authenticate(&mut self, _credentials: AuthCredentials) -> ClientResult<Session> {
        // HN does not require authentication — always succeed with a guest session.
        Ok(self.guest_session())
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.session.is_some()
    }

    // --- Servers ---

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.session.as_ref().map(|s| s.id.as_str()).unwrap_or("hn-anonymous");
        Ok(vec![build_server(account_id)])
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        if id == "hn" {
            let account_id = self.session.as_ref().map(|s| s.id.as_str()).unwrap_or("hn-anonymous");
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
        _channel_id: &str,
        _content: MessageContent,
    ) -> ClientResult<Message> {
        Err(ClientError::NotSupported(
            "Hacker News is read-only; posting requires authentication via news.ycombinator.com"
                .to_string(),
        ))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let limit = query.limit.unwrap_or(20) as usize;

        // Check if this is a post's comment thread channel (hn-post-{id})
        if let Some(post_id) = post_id_from_channel(channel_id) {
            return self.get_comment_thread(post_id, limit).await;
        }

        // Otherwise it's a story feed channel
        let feed = HnFeed::from_channel_id(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!("unknown channel: {channel_id}"))
        })?;

        let mut ids = self.api.get_feed_ids(feed).await?;

        // Apply pagination: if `before` is set, find the offset in the ID list
        if let Some(ref before_id) = query.before {
            if let Ok(before_num) = before_id.parse::<u64>() {
                if let Some(pos) = ids.iter().position(|&id| id == before_num) {
                    ids = ids[pos + 1..].to_vec();
                }
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

    // --- Users ---

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

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(Vec::new())
    }

    // --- Groups ---

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(Vec::new())
    }

    // --- DMs ---

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(Vec::new())
    }

    // --- Notifications ---

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // --- Voice ---

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(Vec::new())
    }

    // --- Presence ---

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Hacker News has no presence system".to_string(),
        ))
    }

    // --- Real-time events ---

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // HN has no WebSocket. Return an empty stream; callers can poll
        // `/v0/updates.json` separately if real-time updates are needed.
        Box::pin(stream::empty())
    }

    // --- Backend info ---

    fn backend_type(&self) -> BackendType {
        BackendType::from("hackernews")
    }

    fn backend_name(&self) -> &str {
        "Hacker News"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::READ_ONLY_FEED
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

        // Collected (item, parent_id) pairs.
        let mut collected: Vec<(types::HnItem, u64)> = Vec::new();
        let max = limit.max(1).min(300);

        while !queue.is_empty() && collected.len() < max {
            let remaining = max - collected.len();
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
