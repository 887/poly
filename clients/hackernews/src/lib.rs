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
    build_channels, build_server, hn_comment_to_message, hn_item_to_message, hn_item_to_view_row,
    hn_user_to_user, post_id_from_channel,
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
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
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
        }
    }

    /// Create a new HN client with a custom base URL (for tests).
    #[must_use]
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            api: HnApiClient::with_base_url(base_url.into()),
            session: None,
            settings_storage: SettingsStorageCell::new(),
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
        if let Some(ref before_id) = query.before
            && let Ok(before_num) = before_id.parse::<u64>()
            && let Some(pos) = ids.iter().position(|&id| id == before_num)
        {
            if let Some(tail) = ids.get(pos + 1..) {
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

    // --- Client-provided UI surface (WP 1.D) ---

    async fn get_context_menu_items(
        &self,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // HackerNews is a read-only feed — no server/channel/user/message
        // concepts that support declarative menu items.
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
            layout: SidebarLayoutKind::Feed,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::FlatList,
            header: Some(ViewHeader {
                title_key: Some("plugin-hackernews-view-stories-title".to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: None, // HN toolbar already lives in the sidebar
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
        let feed = HnFeed::from_channel_id(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!("unknown channel: {channel_id}"))
        })?;

        // Determine page offset from cursor (Offset kind).
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

        // Get the view descriptor's page_size; default to 30.
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
                value: (offset + page_size).to_string(),
            })
        } else {
            None
        };

        let items = self.api.get_items_batch(&slice).await?;

        let rows = items
            .iter()
            .filter(|item| !item.deleted.unwrap_or(false) && !item.dead.unwrap_or(false))
            .map(hn_item_to_view_row)
            .collect();

        Ok(ViewRowsPage { rows, next_cursor })
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        let story_id: u64 = row_id.parse().map_err(|_| {
            ClientError::NotFound(format!("invalid story id: {row_id}"))
        })?;

        let story = self
            .api
            .get_item(story_id)
            .await?
            .ok_or_else(|| ClientError::NotFound(format!("story not found: {story_id}")))?;

        // Build the body block: prefer text body, fall back to URL.
        let body_html = if let Some(ref text) = story.text {
            format!("<p>{text}</p>")
        } else if let Some(ref url) = story.url {
            let title = story.title.as_deref().unwrap_or("Link");
            format!("<p><a href=\"{url}\">{title}</a></p>")
        } else {
            let title = story.title.as_deref().unwrap_or("(no title)");
            format!("<p>{title}</p>")
        };

        // Fetch top-level comments (depth 1 for Pack E).
        let top_kids: Vec<u64> = story.kids.clone().unwrap_or_default()
            .into_iter()
            .take(50)
            .collect();
        let _comments = if !top_kids.is_empty() {
            self.api.get_items_batch(&top_kids).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        let comments_section = if !top_kids.is_empty() {
            Some(poly_client::TreeSpec {
                root_page_size: top_kids.len() as u32,
                max_depth: 1,
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

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        // HackerNews is read-only — no composer.
        Ok(Vec::new())
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // HackerNews is read-only — no per-message actions.
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
        let max = limit.clamp(1, 300);

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
