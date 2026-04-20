//! # poly-matrix
//!
//! Matrix messenger client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using the Matrix client-server
//! HTTP API directly (no matrix-sdk). Maps Matrix Spaces to Poly servers,
//! Matrix rooms to channels.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.
//!
//! DECISION(D21): WASM Plugin Backends.


#[cfg(feature = "native")]
mod api;

#[cfg(feature = "native")]
mod config;

#[cfg(feature = "native")]
mod http;

#[cfg(feature = "native")]
pub mod signup;

/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

#[cfg(feature = "native")]
pub use config::{DEFAULT_HOMESERVER_URL, MatrixAuthInput, MatrixConfig, MatrixConfigError};

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::Stream;
#[cfg(all(feature = "native", target_arch = "wasm32"))]
use futures::stream;
#[cfg(feature = "native")]
use http::{MatrixHttpClient, MatrixSessionState};
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::collections::HashSet;
#[cfg(feature = "native")]
use std::pin::Pin;

/// Return Fluent translations for the given locale.
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        _ => include_str!("../locales/en/plugin.ftl").to_string(),
    }
}

/// Matrix messenger client.
#[cfg(feature = "native")]
pub struct MatrixClient {
    http: MatrixHttpClient,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
    /// F10 — in-memory muted room ids. Persistent storage is F9 — out of scope.
    muted_rooms: std::sync::RwLock<HashSet<String>>,
    /// F10 — in-memory ignored user ids.
    ignored_users: std::sync::RwLock<HashSet<String>>,
    /// F10 — rooms the user has explicitly marked read via the context menu.
    marked_read: std::sync::RwLock<HashSet<String>>,
}

#[cfg(feature = "native")]
impl MatrixClient {
    /// Create a new Matrix client for the default homeserver (matrix.org).
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: MatrixHttpClient::new(MatrixConfig::default_homeserver()),
            settings_storage: SettingsStorageCell::new(),
            muted_rooms: std::sync::RwLock::new(HashSet::new()),
            ignored_users: std::sync::RwLock::new(HashSet::new()),
            marked_read: std::sync::RwLock::new(HashSet::new()),
        }
    }

    /// Create a new Matrix client for a custom homeserver URL.
    pub fn with_homeserver(url: impl Into<String>) -> Result<Self, MatrixConfigError> {
        let config = MatrixConfig::new(url)?;
        Ok(Self {
            http: MatrixHttpClient::new(config),
            settings_storage: SettingsStorageCell::new(),
            muted_rooms: std::sync::RwLock::new(HashSet::new()),
            ignored_users: std::sync::RwLock::new(HashSet::new()),
            marked_read: std::sync::RwLock::new(HashSet::new()),
        })
    }
}

#[cfg(feature = "native")]
impl Default for MatrixClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
impl MatrixClient {
    /// Normalized homeserver URL.
    #[must_use]
    pub fn homeserver_url(&self) -> &str {
        self.http.homeserver_url()
    }

    /// Stable instance identifier for multi-account routing.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.http.instance_id()
    }

    fn build_session(
        &self,
        session_state: &MatrixSessionState,
        profile: &api::ProfileResponse,
    ) -> Session {
        let display_name = profile
            .displayname
            .clone()
            .unwrap_or_else(|| session_state.user_id.clone());

        Session {
            id: session_state.device_id.clone(),
            user: User {
                id: session_state.user_id.clone(),
                display_name,
                avatar_url: profile.avatar_url.clone(),
                presence: PresenceStatus::Online,
                backend: BackendType::from("matrix"),
            },
            token: session_state.access_token.clone(),
            backend: BackendType::from("matrix"),
            icon_emoji: Some("🟦".to_string()),
            instance_id: self.instance_id(),
            backend_url: Some(self.homeserver_url().to_string()),
        }
    }

    fn current_user_id(&self) -> ClientResult<String> {
        self.http
            .session()
            .map(|s| s.user_id)
            .ok_or_else(|| ClientError::AuthFailed("not logged in".into()))
    }

    /// The session/device ID used as the account key in the app.
    /// Must match `Session::id` (= `device_id`) so sidebar lookups find the
    /// right session.
    fn current_account_id(&self) -> ClientResult<String> {
        self.http
            .session()
            .map(|s| s.device_id)
            .ok_or_else(|| ClientError::AuthFailed("not logged in".into()))
    }

    fn room_event_to_message(event: &api::RoomEvent) -> Option<Message> {
        if event.event_type != "m.room.message" {
            return None;
        }
        let event_id = event.event_id.as_deref()?;
        let sender = event.sender.as_deref()?;
        let ts = event.origin_server_ts?;

        let body = event
            .content
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();

        Some(Message {
            id: event_id.to_string(),
            author: User {
                id: sender.to_string(),
                display_name: sender.to_string(),
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from("matrix"),
            },
            content: MessageContent::Text(body),
            timestamp: chrono::DateTime::from_timestamp_millis(ts as i64)
                .unwrap_or_default(),
            edited: false,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            thread: None,
        })
    }

    fn extract_room_name(state: &[api::RoomEvent], fallback: &str) -> String {
        state
            .iter()
            .find(|ev| ev.event_type == "m.room.name")
            .and_then(|ev| ev.content.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    }

    fn extract_avatar_url(state: &[api::RoomEvent]) -> Option<String> {
        state
            .iter()
            .find(|ev| ev.event_type == "m.room.avatar")
            .and_then(|ev| ev.content.get("url"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    fn is_space_room(state: &[api::RoomEvent]) -> bool {
        state.iter().any(|ev| {
            ev.event_type == "m.room.create"
                && ev
                    .content
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("m.space")
        })
    }

    fn build_message_from_send(
        &self,
        event_id: String,
        body: String,
    ) -> ClientResult<Message> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::Internal("session not available after send".into())
        })?;

        Ok(Message {
            id: event_id,
            author: User {
                id: session.user_id.clone(),
                display_name: session
                    .display_name
                    .clone()
                    .unwrap_or_else(|| session.user_id.clone()),
                avatar_url: None,
                presence: PresenceStatus::Online,
                backend: BackendType::from("matrix"),
            },
            content: MessageContent::Text(body),
            timestamp: chrono::Utc::now(),
            edited: false,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            thread: None,
        })
    }

    fn extract_body(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        }
    }
}

// ─── F4: Space tree ────────────────────────────────────────────────────────

/// F4 — one entry in the flattened space/room tree used to build sidebar items.
///
/// Extracted so the pure [`build_sidebar_items`] function can be unit-tested
/// without any network calls.
#[cfg(feature = "native")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceTreeEntry {
    /// Matrix room ID (e.g. `!abc:example.org`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// `true` if this room is a Matrix Space (`m.space`).
    pub is_space: bool,
    /// Parent space room ID, if this room lives inside a space.
    pub parent_id: Option<String>,
}

/// F4 — convert a flat `Vec<SpaceTreeEntry>` into `Vec<SidebarItem>`.
///
/// This is a pure function with no I/O — all async fetching is done by the
/// caller.  Cycle detection: a room whose `parent_id` refers back to its own
/// ancestor is handled by the host's tree-reconstruction step (items that
/// point at non-existent parents are silently dropped).  Within this function
/// we only validate that items with a `parent_id` reference a known `id`.
///
/// FTL label note: `label_key` is set to the room's display name directly.
/// The host's `t()` function falls back to `title_case_fallback(key)` when no
/// FTL match is found; for room names that are plain words or short phrases
/// this produces an acceptable label.  Hyphens in room names will be replaced
/// by spaces in the rendered label — a known limitation documented here.
#[cfg(feature = "native")]
pub fn build_sidebar_items(entries: Vec<SpaceTreeEntry>) -> Vec<SidebarItem> {
    let known_ids: std::collections::HashSet<String> =
        entries.iter().map(|e| e.id.clone()).collect();

    entries
        .into_iter()
        .map(|entry| {
            let parent_id = entry.parent_id.filter(|pid| known_ids.contains(pid.as_str()));
            let icon = if entry.is_space {
                Some(IconSource::Emoji("\u{1f30c}".to_string())) // 🌌
            } else {
                Some(IconSource::Emoji("\u{1f4ac}".to_string())) // 💬
            };
            let route_kind = if entry.is_space {
                SidebarRouteKind::CustomView
            } else {
                SidebarRouteKind::Channel
            };
            SidebarItem {
                id: entry.id,
                parent_id,
                label_key: entry.name,
                icon,
                badge: None,
                route_kind,
            }
        })
        .collect()
}

/// F4 — fetch the full space/room tree for the authenticated user.
///
/// Algorithm:
/// 1. Fetch all joined room IDs via `/joined_rooms`.
/// 2. For each joined room, fetch its state and detect spaces.
/// 3. For each space, call the hierarchy endpoint and collect all child entries.
/// 4. Non-space rooms that are not already listed as space children appear as
///    orphans (parent_id == None).
///
/// Cycle detection: a space that appears as its own child is skipped via the
/// `visited` set.
#[cfg(feature = "native")]
impl MatrixClient {
    async fn fetch_space_tree(&self) -> ClientResult<Vec<SpaceTreeEntry>> {
        use std::collections::HashSet;

        let joined = self.http.fetch_joined_rooms().await?;
        let mut entries: Vec<SpaceTreeEntry> = Vec::new();
        let mut child_ids: HashSet<String> = HashSet::new();
        let mut visited_spaces: HashSet<String> = HashSet::new();

        // First pass: identify which joined rooms are spaces.
        let mut space_ids: Vec<String> = Vec::new();
        let mut room_names: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut room_is_space: std::collections::HashMap<String, bool> =
            std::collections::HashMap::new();

        for room_id in &joined.joined_rooms {
            let state = self
                .http
                .fetch_room_state(room_id)
                .await
                .unwrap_or_default();
            let is_space = Self::is_space_room(&state);
            let name = Self::extract_room_name(&state, room_id);
            room_names.insert(room_id.clone(), name);
            room_is_space.insert(room_id.clone(), is_space);
            if is_space {
                space_ids.push(room_id.clone());
            }
        }

        // Second pass: for each top-level space, walk the hierarchy.
        for space_id in &space_ids {
            if !visited_spaces.insert(space_id.clone()) {
                continue;
            }
            let hierarchy = self
                .http
                .fetch_space_hierarchy(space_id)
                .await
                .unwrap_or(api::SpaceHierarchyResponse { rooms: vec![] });

            for room in &hierarchy.rooms {
                // The hierarchy root (the space itself) is already tracked as a joined room.
                if room.room_id == *space_id {
                    continue;
                }

                let is_space = room.room_type.as_deref() == Some("m.space");
                let name = room.name.clone().unwrap_or_else(|| room.room_id.clone());

                // Track that this room is a child (may be claimed by multiple parents;
                // first claim wins via insertion order).
                child_ids.insert(room.room_id.clone());

                entries.push(SpaceTreeEntry {
                    id: room.room_id.clone(),
                    name,
                    is_space,
                    parent_id: Some(space_id.clone()),
                });

                // If this child is itself a space, mark it visited so we don't
                // recurse into it again if it appears in another parent's hierarchy.
                if is_space {
                    visited_spaces.insert(room.room_id.clone());
                }
            }
        }

        // Third pass: emit top-level entries for all joined spaces.
        let mut top_level: Vec<SpaceTreeEntry> = space_ids
            .iter()
            .map(|id| {
                let name = room_names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.clone());
                SpaceTreeEntry {
                    id: id.clone(),
                    name,
                    is_space: true,
                    parent_id: None,
                }
            })
            .collect();

        // Fourth pass: orphan rooms — joined rooms that are neither spaces nor
        // already claimed as a child of a space.
        let orphan_rooms: Vec<SpaceTreeEntry> = joined
            .joined_rooms
            .iter()
            .filter(|id| {
                !room_is_space.get(*id).copied().unwrap_or(false) && !child_ids.contains(*id)
            })
            .map(|id| {
                let name = room_names.get(id).cloned().unwrap_or_else(|| id.clone());
                SpaceTreeEntry {
                    id: id.clone(),
                    name,
                    is_space: false,
                    parent_id: None,
                }
            })
            .collect();

        top_level.extend(entries);
        top_level.extend(orphan_rooms);
        Ok(top_level)
    }
}

/// F10 — shared menu-item builder (also used inside the trait impl).
#[cfg(feature = "native")]
impl MatrixClient {
    fn simple_item(
        id: &str,
        slot: MenuSlot,
        label_key: &str,
        item_variant: MenuItemVariant,
    ) -> MenuItem {
        MenuItem {
            id: id.to_string(),
            parent_id: None,
            slot,
            label_key: label_key.to_string(),
            icon: None,
            item_variant,
            shortcut: None,
            block: None,
        }
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for MatrixClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let auth_input = MatrixAuthInput::try_from(credentials)?;

        match auth_input {
            MatrixAuthInput::AccessToken(token) => {
                let whoami = self.http.authenticate_with_token(token).await?;
                let profile = self.http.fetch_profile(&whoami.user_id).await.unwrap_or(
                    api::ProfileResponse {
                        displayname: None,
                        avatar_url: None,
                    },
                );
                let session_state = self.http.session().ok_or_else(|| {
                    ClientError::Internal("session not set after token auth".into())
                })?;
                Ok(self.build_session(&session_state, &profile))
            }
            MatrixAuthInput::UsernamePassword { username, password } => {
                let login = self.http.login_with_password(&username, &password).await?;
                let profile = self
                    .http
                    .fetch_profile(&login.user_id)
                    .await
                    .unwrap_or(api::ProfileResponse {
                        displayname: None,
                        avatar_url: None,
                    });
                let session_state = self.http.session().ok_or_else(|| {
                    ClientError::Internal("session not set after password auth".into())
                })?;
                Ok(self.build_session(&session_state, &profile))
            }
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.logout().await
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let joined = self.http.fetch_joined_rooms().await?;
        let account_id = self.current_account_id()?;
        let display_name = self.current_user_id().unwrap_or_else(|_| account_id.clone());
        let mut servers = Vec::new();

        for room_id in &joined.joined_rooms {
            let state = self.http.fetch_room_state(room_id).await.unwrap_or_default();
            if Self::is_space_room(&state) {
                servers.push(Server {
                    id: room_id.clone(),
                    name: Self::extract_room_name(&state, "Unnamed Space"),
                    icon_url: Self::extract_avatar_url(&state),
                    banner_url: None,
                    unread_count: 0,
                    mention_count: 0,
                    categories: vec![],
                    backend: BackendType::from("matrix"),
                    account_id: account_id.clone(),
                    account_display_name: display_name.clone(),
                    default_channel_id: None,
                });
            }
        }

        Ok(servers)
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let state = self.http.fetch_room_state(id).await?;
        let account_id = self.current_account_id()?;
        let display_name = self.current_user_id().unwrap_or_else(|_| account_id.clone());

        Ok(Server {
            id: id.to_string(),
            name: Self::extract_room_name(&state, "Unnamed Space"),
            icon_url: Self::extract_avatar_url(&state),
            banner_url: None,
            unread_count: 0,
            mention_count: 0,
            categories: vec![],
            backend: BackendType::from("matrix"),
            account_id: account_id.clone(),
            default_channel_id: None,
            account_display_name: display_name,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let hierarchy = self.http.fetch_space_hierarchy(server_id).await?;
        let channels: Vec<Channel> = hierarchy
            .rooms
            .iter()
            .filter(|room| room.room_type.as_deref() != Some("m.space"))
            .map(|room| Channel {
                id: room.room_id.clone(),
                name: room.name.clone().unwrap_or_else(|| room.room_id.clone()),
                server_id: server_id.to_string(),
                channel_type: ChannelType::Text,
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
            })
            .collect();

        Ok(channels)
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let state = self.http.fetch_room_state(id).await?;

        Ok(Channel {
            id: id.to_string(),
            name: Self::extract_room_name(&state, id),
            server_id: String::new(),
            channel_type: ChannelType::Text,
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        })
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let txn_id = uuid::Uuid::new_v4().to_string();
        let body = Self::extract_body(&content);

        let send_req = api::SendMessageRequest {
            msgtype: "m.text".to_string(),
            body: body.clone(),
            formatted_body: None,
            format: None,
            relates_to: None,
        };

        let result = self
            .http
            .send_message(channel_id, &txn_id, &send_req)
            .await?;

        self.build_message_from_send(result.event_id, body)
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let txn_id = uuid::Uuid::new_v4().to_string();
        let body = Self::extract_body(&content);

        let send_req = api::SendMessageRequest {
            msgtype: "m.text".to_string(),
            body: body.clone(),
            formatted_body: None,
            format: None,
            relates_to: Some(api::RelatesTo {
                in_reply_to: Some(api::InReplyTo {
                    event_id: reply_to_message_id.to_string(),
                }),
            }),
        };

        let result = self
            .http
            .send_message(channel_id, &txn_id, &send_req)
            .await?;

        self.build_message_from_send(result.event_id, body)
    }

    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        // Stub: Matrix supports PUT /_matrix/client/v3/rooms/{roomId}/typing/{userId}
        // but the HTTP wiring is not yet plumbed through MatrixHttpClient.
        // TODO: wire real endpoint in http.rs.
        tracing::warn!("send_typing stub for matrix (channel_id={channel_id})");
        Ok(())
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let from = if let Some(before) = &query.before {
            before.clone()
        } else {
            let session = self.http.session().ok_or_else(|| {
                ClientError::AuthFailed("not logged in".into())
            })?;
            session.sync_next_batch.unwrap_or_default()
        };

        if from.is_empty() {
            // No pagination token; do an initial sync to get one
            let sync = self.http.sync(None, Some(0)).await?;
            let prev_batch = sync
                .rooms
                .as_ref()
                .and_then(|rooms| rooms.join.as_ref())
                .and_then(|join| join.get(channel_id))
                .and_then(|room| room.timeline.as_ref())
                .and_then(|tl| tl.prev_batch.clone())
                .unwrap_or(sync.next_batch);

            let limit = u64::from(query.limit.unwrap_or(50));
            let response = self
                .http
                .fetch_messages(channel_id, &prev_batch, "b", Some(limit))
                .await?;

            return Ok(response
                .chunk
                .iter()
                .filter_map(Self::room_event_to_message)
                .collect());
        }

        let dir = if query.after.is_some() { "f" } else { "b" };
        let limit = u64::from(query.limit.unwrap_or(50));
        let response = self
            .http
            .fetch_messages(channel_id, &from, dir, Some(limit))
            .await?;

        Ok(response
            .chunk
            .iter()
            .filter_map(Self::room_event_to_message)
            .collect())
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let profile = self.http.fetch_profile(id).await?;
        Ok(User {
            id: id.to_string(),
            display_name: profile.displayname.unwrap_or_else(|| id.to_string()),
            avatar_url: profile.avatar_url,
            presence: PresenceStatus::Offline,
            backend: BackendType::from("matrix"),
        })
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        let members = self.http.fetch_room_members(channel_id).await?;
        let users: Vec<User> = members
            .chunk
            .iter()
            .filter(|ev| {
                ev.event_type == "m.room.member"
                    && ev
                        .content
                        .get("membership")
                        .and_then(serde_json::Value::as_str)
                        == Some("join")
            })
            .filter_map(|ev| {
                let user_id = ev.state_key.as_deref()?;
                let display_name = ev
                    .content
                    .get("displayname")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(user_id)
                    .to_string();
                let avatar_url = ev
                    .content
                    .get("avatar_url")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);

                Some(User {
                    id: user_id.to_string(),
                    display_name,
                    avatar_url,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("matrix"),
                })
            })
            .collect();

        Ok(users)
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let user_id = self.current_user_id()?;
        let m_direct = self
            .http
            .fetch_account_data(&user_id, "m.direct")
            .await
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let mut dms = Vec::new();
        if let Some(obj) = m_direct.as_object() {
            for (other_user_id, room_ids) in obj {
                if let Some(room_id) = room_ids
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(serde_json::Value::as_str)
                {
                    dms.push(DmChannel {
                        id: room_id.to_string(),
                        user: User {
                            id: other_user_id.clone(),
                            display_name: other_user_id.clone(),
                            avatar_url: None,
                            presence: PresenceStatus::Offline,
                            backend: BackendType::from("matrix"),
                        },
                        last_message: None,
                        unread_count: 0,
                        backend: BackendType::from("matrix"),
                        account_id: user_id.clone(),
                    });
                }
            }
        }

        Ok(dms)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(vec![])
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tokio::sync::mpsc;
            let http = self.http.clone();
            let (tx, rx) = mpsc::channel::<ClientEvent>(128);

            tokio::spawn(async move {
                let mut since = http.sync_next_batch();
                loop {
                    match http.sync(since.as_deref(), Some(30000)).await {
                        Ok(response) => {
                            // Update the batch token
                            since = Some(response.next_batch.clone());
                            http.set_sync_next_batch(response.next_batch);

                            // Process joined rooms
                            if let Some(rooms) = &response.rooms
                                && let Some(joined) = &rooms.join {
                                    for (room_id, room) in joined {
                                        // Timeline events → MessageReceived
                                        if let Some(timeline) = &room.timeline {
                                            for event in &timeline.events {
                                                if let Some(msg) =
                                                    MatrixClient::room_event_to_message(event)
                                                {
                                                    let _ = tx
                                                        .send(ClientEvent::MessageReceived {
                                                            channel_id: room_id.clone(),
                                                            message: msg,
                                                        })
                                                        .await;
                                                }
                                            }
                                        }
                                        // Ephemeral events → typing
                                        if let Some(ephemeral) = &room.ephemeral {
                                            for ev in &ephemeral.events {
                                                if ev
                                                    .get("type")
                                                    .and_then(|t| t.as_str())
                                                    == Some("m.typing")
                                                    && let Some(user_ids) = ev
                                                        .get("content")
                                                        .and_then(|c| c.get("user_ids"))
                                                        .and_then(|u| u.as_array())
                                                {
                                                    for uid in user_ids {
                                                        if let Some(user_id) = uid.as_str() {
                                                            let _ = tx
                                                                .send(
                                                                    ClientEvent::TypingStarted {
                                                                        channel_id: room_id
                                                                            .clone(),
                                                                        user_id: user_id
                                                                            .to_string(),
                                                                        timestamp: chrono::Utc::now(),
                                                                    },
                                                                )
                                                                .await;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Matrix sync error: {e:?}");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            });

            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(stream::empty())
        }
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from("matrix")
    }

    fn backend_name(&self) -> &str {
        "Matrix"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            voice: VoiceSupport::None,
            landing: poly_client::LandingPage::DirectMessages,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    // --- Client-provided UI surface (WP 1 / plan-client-ui-surface) ---

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match target {
            // ── Server / Space ─────────────────────────────────────────────
            MenuTargetKind::Server => Ok(vec![
                Self::simple_item("space-settings", MenuSlot::AfterFavorites, "plugin-matrix-menu-space-settings-label", MenuItemVariant::Normal),
                Self::simple_item("edit-per-space-profile", MenuSlot::AfterFavorites, "plugin-matrix-menu-edit-per-space-profile-label", MenuItemVariant::Normal),
                Self::simple_item("e2ee-verification", MenuSlot::AfterFavorites, "plugin-matrix-menu-e2ee-verification-label", MenuItemVariant::Normal),
                // F10 additions
                Self::simple_item("browse-rooms-in-space", MenuSlot::AfterFavorites, "plugin-matrix-menu-browse-rooms-in-space-label", MenuItemVariant::Normal),
                Self::simple_item("add-room-to-space", MenuSlot::AfterFavorites, "plugin-matrix-menu-add-room-to-space-label", MenuItemVariant::Normal),
                Self::simple_item("leave-space", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-space-label", MenuItemVariant::Destructive),
            ]),

            // ── Channel / Room ─────────────────────────────────────────────
            MenuTargetKind::Channel => {
                // Distinct id per state: mark-read-room / mark-unread-room
                // Poisoned lock treated as "not read" — safe default.
                let is_read = self.marked_read.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let read_item = if is_read {
                    Self::simple_item("mark-unread-room", MenuSlot::Top, "plugin-matrix-menu-mark-unread-room-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("mark-read-room", MenuSlot::Top, "plugin-matrix-menu-mark-read-room-label", MenuItemVariant::Normal)
                };

                // Distinct id per state: mute-room / unmute-room
                let is_muted = self.muted_rooms.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let mute_item = if is_muted {
                    Self::simple_item("unmute-room", MenuSlot::AfterFavorites, "plugin-matrix-menu-unmute-room-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("mute-room", MenuSlot::AfterFavorites, "plugin-matrix-menu-mute-room-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    read_item,
                    mute_item,
                    Self::simple_item("leave-room", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-room-label", MenuItemVariant::Destructive),
                ])
            }

            // ── DM Channel ─────────────────────────────────────────────────
            MenuTargetKind::Dm => {
                let is_read = self.marked_read.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let read_item = if is_read {
                    Self::simple_item("mark-unread-room", MenuSlot::Top, "plugin-matrix-menu-mark-unread-room-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("mark-read-room", MenuSlot::Top, "plugin-matrix-menu-mark-read-room-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    read_item,
                    Self::simple_item("leave-dm", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-dm-label", MenuItemVariant::Destructive),
                ])
            }

            // ── User ───────────────────────────────────────────────────────
            MenuTargetKind::User => {
                // Distinct id per state: ignore-user / unignore-user
                let is_ignored = self.ignored_users.read()
                    .map(|g| g.contains(target_id))
                    .unwrap_or(false);
                let ignore_item = if is_ignored {
                    Self::simple_item("unignore-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-unignore-user-label", MenuItemVariant::Normal)
                } else {
                    Self::simple_item("ignore-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-ignore-user-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    Self::simple_item("open-dm", MenuSlot::Top, "plugin-matrix-menu-open-dm-label", MenuItemVariant::Normal),
                    Self::simple_item("view-profile", MenuSlot::Top, "plugin-matrix-menu-view-profile-label", MenuItemVariant::Normal),
                    // Cross-signing stub
                    Self::simple_item("verify-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-verify-user-label", MenuItemVariant::Normal),
                    ignore_item,
                ])
            }

            // ── Message ────────────────────────────────────────────────────
            MenuTargetKind::Message => Ok(vec![
                Self::simple_item("react-message", MenuSlot::Top, "plugin-matrix-menu-react-message-label", MenuItemVariant::Normal),
                Self::simple_item("reply-in-thread", MenuSlot::Top, "plugin-matrix-menu-reply-in-thread-label", MenuItemVariant::Normal),
                Self::simple_item("copy-permalink", MenuSlot::AfterFavorites, "plugin-matrix-menu-copy-permalink-label", MenuItemVariant::Normal),
                // Destructive — author or admin only
                Self::simple_item("redact-message", MenuSlot::BeforeLeave, "plugin-matrix-menu-redact-message-label", MenuItemVariant::Destructive),
            ]),

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
            // ── Server / Space ──────────────────────────────────────────────
            "space-settings"
            | "edit-per-space-profile"
            | "e2ee-verification"
            | "browse-rooms-in-space"
            | "add-room-to-space"
            | "leave-space" => Ok(ActionOutcome::Noop),

            // ── Channel / Room — state mutations ────────────────────────────
            // Poisoned lock treated as a no-op write — silent, non-panicking.
            "mark-read-room" => {
                if let Ok(mut g) = self.marked_read.write() {
                    g.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Noop)
            }
            "mark-unread-room" => {
                if let Ok(mut g) = self.marked_read.write() {
                    g.remove(target_id);
                }
                Ok(ActionOutcome::Noop)
            }
            "mute-room" => {
                if let Ok(mut g) = self.muted_rooms.write() {
                    g.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Noop)
            }
            "unmute-room" => {
                if let Ok(mut g) = self.muted_rooms.write() {
                    g.remove(target_id);
                }
                Ok(ActionOutcome::Noop)
            }
            "leave-room" | "leave-dm" => Ok(ActionOutcome::Noop),

            // ── User — state mutations ───────────────────────────────────────
            "open-dm" | "view-profile" | "verify-user" => Ok(ActionOutcome::Noop),
            "ignore-user" => {
                if let Ok(mut g) = self.ignored_users.write() {
                    g.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Noop)
            }
            "unignore-user" => {
                if let Ok(mut g) = self.ignored_users.write() {
                    g.remove(target_id);
                }
                Ok(ActionOutcome::Noop)
            }

            // ── Message ───────────────────────────────────────────────────────
            "react-message"
            | "reply-in-thread"
            | "copy-permalink"
            | "redact-message" => Ok(ActionOutcome::Noop),

            _ => Err(ClientError::NotFound(format!("unknown action: {action_id}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "space-settings".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "display-name".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "topic".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                ],
                info_block: None,
            },
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "privacy".to_string(),
                icon: None,
                fields: vec![SettingDescriptor {
                    key: "allow-guests".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
                    extra: String::new(),
                }],
                info_block: None,
            },
        ])
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
        // F4: Switch to Custom layout so the host renders our full space tree.
        let entries = self.fetch_space_tree().await.unwrap_or_default();
        let items = build_sidebar_items(entries);
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Custom,
            sections: vec![SidebarSection {
                header_key: Some("plugin-matrix-sidebar-spaces-section".to_string()),
                collapsible: false,
                default_collapsed: false,
                items,
            }],
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        _channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        Err(ClientError::NotSupported("view-rows not yet implemented".into()))
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("view-detail not yet implemented".into()))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(vec![ComposerButton {
            id: "me-action".to_string(),
            label_key: "plugin-matrix-composer-me-label".to_string(),
            icon: "🎭".to_string(),
            position: ComposerSlot::LeftOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![MenuItem {
            id: "verify-sender".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-matrix-message-action-verify-sender-label".to_string(),
            icon: None,
            item_variant: MenuItemVariant::Normal,
            shortcut: None,
            block: None,
        }])
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "me-action" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown composer action: {other}"))),
        }
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "verify-sender" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn default_client_is_not_authenticated() {
        let client = MatrixClient::new();
        assert!(!client.is_authenticated());
    }

    #[test]
    fn custom_homeserver_client() {
        let client = MatrixClient::with_homeserver("https://my.server.tld").unwrap();
        assert!(!client.is_authenticated());
    }

    #[test]
    fn translations_return_nonempty_for_en() {
        let t = plugin_translations("en");
        assert!(t.contains("plugin-matrix-title"));
    }

    // ─── F4: build_sidebar_items unit tests ───────────────────────────────────

    /// Fixture: two top-level spaces, one nested space, and three rooms.
    ///
    /// Layout:
    /// - `!parent:s`  (space, top-level)
    ///   - `!nested:s` (space, child of parent)
    ///     - `!room-nested:s` (room, child of nested)
    ///   - `!room-parent:s` (room, child of parent)
    /// - `!orphan:s`  (room, no space parent)
    fn fixture_entries() -> Vec<SpaceTreeEntry> {
        vec![
            SpaceTreeEntry {
                id: "!parent:s".to_string(),
                name: "Parent Space".to_string(),
                is_space: true,
                parent_id: None,
            },
            SpaceTreeEntry {
                id: "!nested:s".to_string(),
                name: "Nested Space".to_string(),
                is_space: true,
                parent_id: Some("!parent:s".to_string()),
            },
            SpaceTreeEntry {
                id: "!room-nested:s".to_string(),
                name: "Nested Room".to_string(),
                is_space: false,
                parent_id: Some("!nested:s".to_string()),
            },
            SpaceTreeEntry {
                id: "!room-parent:s".to_string(),
                name: "Parent Room".to_string(),
                is_space: false,
                parent_id: Some("!parent:s".to_string()),
            },
            SpaceTreeEntry {
                id: "!orphan:s".to_string(),
                name: "Orphan Room".to_string(),
                is_space: false,
                parent_id: None,
            },
        ]
    }

    #[test]
    fn build_sidebar_items_preserves_parent_id_graph() {
        let entries = fixture_entries();
        let items = build_sidebar_items(entries);

        // Helper: find item by id.
        let find = |id: &str| -> &SidebarItem {
            items.iter().find(|i| i.id == id).expect("item not found")
        };

        // Parent space has no parent.
        assert!(find("!parent:s").parent_id.is_none(), "parent space must be root");

        // Nested space is child of parent.
        assert_eq!(
            find("!nested:s").parent_id.as_deref(),
            Some("!parent:s"),
            "nested space parent_id"
        );

        // Room inside nested space has nested space as parent.
        assert_eq!(
            find("!room-nested:s").parent_id.as_deref(),
            Some("!nested:s"),
            "nested room parent_id"
        );

        // Room inside parent space has parent space as parent.
        assert_eq!(
            find("!room-parent:s").parent_id.as_deref(),
            Some("!parent:s"),
            "parent room parent_id"
        );

        // Orphan room has no parent.
        assert!(find("!orphan:s").parent_id.is_none(), "orphan room must be root");
    }

    #[test]
    fn build_sidebar_items_drops_unknown_parent_ids() {
        // A room that references a parent not present in the list.
        let entries = vec![
            SpaceTreeEntry {
                id: "!room:s".to_string(),
                name: "Room".to_string(),
                is_space: false,
                parent_id: Some("!nonexistent:s".to_string()),
            },
        ];
        let items = build_sidebar_items(entries);
        assert_eq!(items.len(), 1, "one item");
        // parent_id must be None because the referenced parent is unknown.
        assert!(
            items[0].parent_id.is_none(),
            "unknown parent_id must be dropped"
        );
    }

    #[test]
    fn build_sidebar_items_spaces_use_customview_route() {
        let entries = fixture_entries();
        let items = build_sidebar_items(entries);
        for item in &items {
            let expected_is_space = matches!(item.id.as_str(), "!parent:s" | "!nested:s");
            if expected_is_space {
                assert_eq!(
                    item.route_kind,
                    SidebarRouteKind::CustomView,
                    "space {} must use CustomView route",
                    item.id
                );
            } else {
                assert_eq!(
                    item.route_kind,
                    SidebarRouteKind::Channel,
                    "room {} must use Channel route",
                    item.id
                );
            }
        }
    }

    #[test]
    fn build_sidebar_items_label_key_is_room_name() {
        let entries = fixture_entries();
        let items = build_sidebar_items(entries);
        let find = |id: &str| items.iter().find(|i| i.id == id).expect("item not found");
        assert_eq!(find("!parent:s").label_key, "Parent Space");
        assert_eq!(find("!orphan:s").label_key, "Orphan Room");
    }

    #[test]
    fn ftl_contains_sidebar_section_key() {
        let ftl = plugin_translations("en");
        assert!(
            ftl.contains("plugin-matrix-sidebar-spaces-section"),
            "FTL must contain the sidebar section header key"
        );
    }
}
