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
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        _ => include_str!("../locales/en/plugin.ftl").to_string(),
    }
}

/// Convert a Matrix `mxc://` URI to an HTTP thumbnail URL.
///
/// Matrix room/user avatars are always stored as `mxc://{server}/{media_id}`.
/// Browsers cannot load these directly — they require a Matrix media API call.
/// This function maps the MXC URI to:
/// `{homeserver_url}/_matrix/media/v3/thumbnail/{server}/{media_id}?width=64&height=64&method=scale`
///
/// If `mxc_uri` does not start with `mxc://` (e.g. a plain HTTP URL in test data),
/// it is returned unchanged so the test suite works without a media endpoint.
#[cfg(feature = "native")]
pub(crate) fn mxc_to_http_thumbnail(mxc_uri: &str, homeserver_url: &str) -> String {
    let Some(rest) = mxc_uri.strip_prefix("mxc://") else {
        return mxc_uri.to_string();
    };
    let base = homeserver_url.trim_end_matches('/');
    format!("{base}/_matrix/media/v3/thumbnail/{rest}?width=64&height=64&method=scale")
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
    /// Stored version override (None = use http::DEFAULT_CLIENT_VERSION).
    version_override: std::sync::Mutex<Option<String>>,
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
            version_override: std::sync::Mutex::new(None),
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
            version_override: std::sync::Mutex::new(None),
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

        let avatar_url = profile
            .avatar_url
            .as_deref()
            .map(|url| mxc_to_http_thumbnail(url, self.homeserver_url()));
        Session {
            id: session_state.device_id.clone(),
            user: User {
                id: session_state.user_id.clone(),
                display_name,
                avatar_url,
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
            timestamp: chrono::DateTime::from_timestamp_millis(
                i64::try_from(ts).unwrap_or(i64::MAX),
            )
            .unwrap_or_default(),
            edited: false,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            thread: None,
            preview_image_url: None,
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

    /// Extract and resolve the room avatar URL from state events.
    ///
    /// The `m.room.avatar` content `url` field is always an `mxc://` URI per
    /// the Matrix spec.  Browsers cannot load `mxc://` directly, so we convert
    /// it to an HTTP thumbnail URL via `mxc_to_http_thumbnail`.
    ///
    /// Non-`mxc://` values (e.g. direct HTTP URLs in test data) are returned
    /// as-is so the test suite works without a media endpoint.
    fn extract_avatar_url(state: &[api::RoomEvent], homeserver_url: &str) -> Option<String> {
        let raw = state
            .iter()
            .find(|ev| ev.event_type == "m.room.avatar")
            .and_then(|ev| ev.content.get("url"))
            .and_then(serde_json::Value::as_str)?;
        Some(mxc_to_http_thumbnail(raw, homeserver_url))
    }

    /// Patch each message's `author.display_name` + `author.avatar_url` with
    /// the sender's profile data. Matrix message events only carry the raw
    /// MXID — without this the chat shows `@user:server` text and a blank
    /// avatar for every author. Profiles are fetched in parallel and
    /// deduplicated per unique sender to avoid N requests for N messages.
    async fn hydrate_message_authors(&self, mut messages: Vec<Message>) -> Vec<Message> {
        use std::collections::{HashMap, HashSet};
        let unique_senders: Vec<String> = {
            let mut seen = HashSet::new();
            messages
                .iter()
                .map(|m| m.author.id.clone())
                .filter(|id| seen.insert(id.clone()))
                .collect()
        };
        if unique_senders.is_empty() {
            return messages;
        }
        let homeserver_url = self.homeserver_url().to_string();
        let profile_futures: Vec<_> = unique_senders
            .iter()
            .map(|id| self.http.fetch_profile(id))
            .collect();
        let profiles = futures::future::join_all(profile_futures).await;
        let mut by_id: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
        for (id, result) in unique_senders.into_iter().zip(profiles) {
            if let Ok(p) = result {
                let avatar = p
                    .avatar_url
                    .as_deref()
                    .map(|raw| mxc_to_http_thumbnail(raw, &homeserver_url));
                by_id.insert(id, (p.displayname, avatar));
            }
        }
        for m in &mut messages {
            if let Some((display, avatar)) = by_id.get(&m.author.id) {
                if let Some(d) = display {
                    m.author.display_name = d.clone();
                }
                if let Some(a) = avatar {
                    m.author.avatar_url = Some(a.clone());
                }
            }
        }
        messages
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

    /// Stash the resolved profile (display name + HTTP-thumbnail avatar URL)
    /// onto the session state so `build_message_from_send` can populate the
    /// message author block without re-fetching `/profile/{userId}` for
    /// every send.
    fn cache_session_profile(&self, profile: &api::ProfileResponse) {
        let display_name = profile.displayname.clone();
        let avatar_url = profile
            .avatar_url
            .as_deref()
            .map(|raw| mxc_to_http_thumbnail(raw, self.homeserver_url()));
        drop(self.http.update_session_profile(display_name, avatar_url));
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
                avatar_url: session.avatar_url.clone(),
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
            preview_image_url: None,
        })
    }

    fn extract_body(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text)
            | MessageContent::WithAttachments { text, .. } => text.clone(),
        }
    }

    /// Extract the room topic from state events (`m.room.topic`).
    fn extract_room_topic(state: &[api::RoomEvent]) -> Option<String> {
        state
            .iter()
            .find(|ev| ev.event_type == "m.room.topic")
            .and_then(|ev| ev.content.get("topic"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    /// Extract the canonical alias from state events (`m.room.canonical_alias`).
    fn extract_canonical_alias(state: &[api::RoomEvent]) -> Option<String> {
        state
            .iter()
            .find(|ev| ev.event_type == "m.room.canonical_alias")
            .and_then(|ev| ev.content.get("alias"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    /// Count joined members from state events (`m.room.member` with
    /// `membership == "join"`).
    fn count_joined_members(state: &[api::RoomEvent]) -> usize {
        state
            .iter()
            .filter(|ev| {
                ev.event_type == "m.room.member"
                    && ev
                        .content
                        .get("membership")
                        .and_then(serde_json::Value::as_str)
                        == Some("join")
            })
            .count()
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
#[must_use]
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
                self.cache_session_profile(&profile);
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
                self.cache_session_profile(&profile);
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

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["<homeserver from account>".to_string()],
            description: "Matrix backend. Federated, end-to-end-encrypted \
                          messaging via the client-server API. Connects to \
                          whichever homeserver each signed-in account specifies \
                          (matrix.org, your own, or any compliant server)."
                .to_string(),
            homepage: Some("https://matrix.org".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let joined = self.http.fetch_joined_rooms().await?;
        let account_id = self.current_account_id()?;
        let display_name = self.current_user_id().unwrap_or_else(|_| account_id.clone());
        let mut servers = Vec::new();

        let homeserver_url = self.homeserver_url().to_string();
        for room_id in &joined.joined_rooms {
            let state = self.http.fetch_room_state(room_id).await.unwrap_or_default();
            if Self::is_space_room(&state) {
                servers.push(Server {
                    id: room_id.clone(),
                    name: Self::extract_room_name(&state, "Unnamed Space"),
                    icon_url: Self::extract_avatar_url(&state, &homeserver_url),
                    banner_url: None,
                    unread_count: 0,
                    mention_count: 0,
                    categories: vec![],
                    backend: BackendType::from("matrix"),
                    account_id: account_id.clone(),
                    account_display_name: display_name.clone(),
                    default_channel_id: None,
                    description: None,
                    star_count: None,
                    language: None,
                    forks_count: None,
                    open_issues_count: None,
                });
            }
        }

        Ok(servers)
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let state = self.http.fetch_room_state(id).await?;
        let account_id = self.current_account_id()?;
        let display_name = self.current_user_id().unwrap_or_else(|_| account_id.clone());
        let homeserver_url = self.homeserver_url().to_string();

        Ok(Server {
            id: id.to_string(),
            name: Self::extract_room_name(&state, "Unnamed Space"),
            icon_url: Self::extract_avatar_url(&state, &homeserver_url),
            banner_url: None,
            unread_count: 0,
            mention_count: 0,
            categories: vec![],
            backend: BackendType::from("matrix"),
            account_id: account_id.clone(),
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
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

        let messages = if from.is_empty() {
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

            response
                .chunk
                .iter()
                .filter_map(Self::room_event_to_message)
                .collect::<Vec<_>>()
        } else {
            let dir = if query.after.is_some() { "f" } else { "b" };
            let limit = u64::from(query.limit.unwrap_or(50));
            let response = self
                .http
                .fetch_messages(channel_id, &from, dir, Some(limit))
                .await?;

            response
                .chunk
                .iter()
                .filter_map(Self::room_event_to_message)
                .collect::<Vec<_>>()
        };

        Ok(self.hydrate_message_authors(messages).await)
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let profile = self.http.fetch_profile(id).await?;
        let avatar_url = profile
            .avatar_url
            .as_deref()
            .map(|url| mxc_to_http_thumbnail(url, self.homeserver_url()));
        Ok(User {
            id: id.to_string(),
            display_name: profile.displayname.unwrap_or_else(|| id.to_string()),
            avatar_url,
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

        // F-MX-1: collect (room_id, user_id) pairs first, then fetch profiles concurrently.
        let mut pairs: Vec<(String, String)> = Vec::new();
        if let Some(obj) = m_direct.as_object() {
            for (other_user_id, room_ids) in obj {
                if let Some(room_id) = room_ids
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(serde_json::Value::as_str)
                {
                    pairs.push((room_id.to_string(), other_user_id.clone()));
                }
            }
        }

        // Fetch profiles in parallel; fall back to MXID on any error.
        let profile_futures: Vec<_> = pairs
            .iter()
            .map(|(_, uid)| self.http.fetch_profile(uid))
            .collect();
        let profiles = futures::future::join_all(profile_futures).await;

        let mut dms = Vec::new();
        for ((room_id, other_user_id), profile_result) in pairs.into_iter().zip(profiles) {
            let (display_name, avatar_url) = match profile_result {
                Ok(p) => (
                    p.displayname.unwrap_or_else(|| other_user_id.clone()),
                    p.avatar_url,
                ),
                Err(_) => (other_user_id.clone(), None),
            };
            dms.push(DmChannel {
                id: room_id,
                user: User {
                    id: other_user_id,
                    display_name,
                    avatar_url,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("matrix"),
                },
                last_message: None,
                unread_count: 0,
                backend: BackendType::from("matrix"),
                account_id: user_id.clone(),
            });
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
                                                    drop(
                                                        tx.send(ClientEvent::MessageReceived {
                                                            channel_id: room_id.clone(),
                                                            message: msg,
                                                        })
                                                        .await,
                                                    );
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
                                                            drop(
                                                                tx.send(
                                                                    ClientEvent::TypingStarted {
                                                                        channel_id: room_id
                                                                            .clone(),
                                                                        user_id: user_id
                                                                            .to_string(),
                                                                        timestamp: chrono::Utc::now(),
                                                                    },
                                                                )
                                                                .await,
                                                            );
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

    // ── Moderation (B-MX — plan-permissions-moderation.md §1.2 + §4) ─────────

    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let user_id = self.current_user_id()?;
        let pl = self.http.fetch_power_levels(server_id).await?;
        let level = pl.user_level(&user_id);

        // Matrix rule: level ≥ required threshold means the action is allowed.
        // `state_default` gates room-setting state events (name, topic, power_levels).
        let display_role = if level >= 100 {
            "Admin".to_string()
        } else if level >= 50 {
            "Moderator".to_string()
        } else {
            "Member".to_string()
        };

        Ok(MemberPermissions {
            manage_server: level >= pl.state_default,
            manage_channels: level >= pl.state_default,
            manage_roles: level >= pl.state_default,
            kick_members: level >= pl.kick,
            ban_members: level >= pl.ban,
            manage_messages: level >= pl.redact,
            // Matrix has no timeout concept — always false.
            timeout_members: false,
            display_role,
            power_level: Some(level),
        })
    }

    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        // In Matrix, server_id is the room_id. Kick = remove membership.
        self.http.kick_member(server_id, member_id, reason).await
    }

    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        _delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        // Matrix bans are permanent and have no message-history deletion parameter.
        // `_delete_message_history_secs` is silently ignored.
        self.http.ban_member(server_id, member_id, reason).await
    }

    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http.unban_member(server_id, member_id).await
    }

    /// Matrix has no native timeout primitive.
    ///
    /// Temporary mutes require modifying the user's power level for a period,
    /// which cannot be expressed as a single atomic timed operation in the spec.
    /// `has_timed_ban = false` in `backend_capabilities()` ensures the Timeout
    /// button is hidden in the UI before this method is ever called.
    async fn timeout_member(
        &self,
        _server_id: &str,
        _member_id: &str,
        _until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "timeout: Matrix has no timeout primitive; use power level changes for moderation"
                .to_string(),
        ))
    }

    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let members_resp = self.http.fetch_banned_members(server_id).await?;

        let banned: Vec<BannedMember> = members_resp
            .chunk
            .iter()
            .filter(|ev| {
                ev.event_type == "m.room.member"
                    && ev
                        .content
                        .get("membership")
                        .and_then(serde_json::Value::as_str)
                        == Some("ban")
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
                let reason = ev
                    .content
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                Some(BannedMember {
                    user_id: user_id.to_string(),
                    display_name,
                    avatar_url,
                    reason,
                    // Matrix bans have no expiry.
                    expires_at: None,
                    banned_at: None,
                })
            })
            .collect();

        Ok(banned)
    }

    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        // Matrix uses "redact" terminology. The txn_id is a UUID for idempotency.
        let txn_id = uuid::Uuid::new_v4().to_string();
        self.http
            .redact_event(channel_id, message_id, &txn_id, None)
            .await
    }

    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        // Matrix supports name and topic only. position, slow_mode_secs, and nsfw
        // have no Matrix equivalent — log a warning and ignore those fields.
        if update.position.is_some() {
            tracing::warn!(
                "update_channel(matrix): `position` has no Matrix equivalent — ignored"
            );
        }
        if update.slow_mode_secs.is_some() {
            tracing::warn!(
                "update_channel(matrix): `slow_mode_secs` has no Matrix equivalent — ignored"
            );
        }
        if update.nsfw.is_some() {
            tracing::warn!(
                "update_channel(matrix): `nsfw` has no Matrix equivalent — ignored"
            );
        }

        if let Some(name) = &update.name {
            self.http.set_room_name(channel_id, name).await?;
        }
        if let Some(topic) = &update.topic {
            self.http.set_room_topic(channel_id, topic).await?;
        }

        Ok(())
    }

    async fn reorder_channels(
        &self,
        _server_id: &str,
        _ordering: Vec<String>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "Matrix spaces order via space hierarchy state events; reorder not exposed in trait shape"
                .to_string(),
        ))
    }

    /// Matrix has no native audit log.
    ///
    /// Walking room events to synthesise a log is expensive and not yet
    /// implemented. Returns an empty list with `has_moderation_log = false`
    /// ensuring the UI tab is hidden.
    async fn get_moderation_log(
        &self,
        _server_id: &str,
        _limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        // TODO(B-MX): Walk room events for m.room.member + m.room.redaction and
        // synthesise entries. Deferred — see plan-permissions-moderation.md §4 B-MX.
        Ok(Vec::new())
    }

    // ── Block / Ignore (m.ignored_user_list account data) ─────────────────────

    /// Block a user. Matrix conflates block and ignore via `m.ignored_user_list`.
    ///
    /// Fetches the current ignore list, adds `user_id`, and writes it back via
    /// `PUT /_matrix/client/v3/user/:user_id/account_data/m.ignored_user_list`.
    async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users
            .entry(user_id.to_string())
            .or_insert(serde_json::Value::Object(serde_json::Map::new()));
        self.http.put_ignored_user_list(&me, &list).await
    }

    /// Ignore a user — Matrix conflates block and ignore via `m.ignored_user_list`.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()> {
        // Same operation as block_user — Matrix uses m.ignored_user_list for both.
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users
            .entry(user_id.to_string())
            .or_insert(serde_json::Value::Object(serde_json::Map::new()));
        self.http.put_ignored_user_list(&me, &list).await
    }

    /// Unblock a user. Removes them from `m.ignored_user_list`.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users.remove(user_id);
        self.http.put_ignored_user_list(&me, &list).await
    }

    /// Unignore a user — same as `unblock_user` in Matrix.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()> {
        // Same operation as unblock_user — Matrix uses m.ignored_user_list for both.
        let me = self.current_user_id()?;
        let mut list = self.http.fetch_ignored_user_list(&me).await?;
        list.ignored_users.remove(user_id);
        self.http.put_ignored_user_list(&me, &list).await
    }

    // ── Friend system (not native to Matrix) ──────────────────────────────────

    /// Matrix has no native friend concept — returns `NotSupported`.
    // TODO(matrix): no native friend concept
    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_friend: Matrix has no native friend concept".to_string(),
        ))
    }

    /// Matrix has no native friend concept — returns `NotSupported`.
    // TODO(matrix): no native friend concept
    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "remove_friend: Matrix has no native friend concept".to_string(),
        ))
    }

    /// Matrix has no native friend concept — returns `NotSupported`.
    // TODO(matrix): no native friend concept
    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_friend_nickname: Matrix has no native friend concept".to_string(),
        ))
    }

    // ── User notes (not native to Matrix) ─────────────────────────────────────

    /// Matrix has no native per-user note system — returns `NotSupported`.
    // TODO(matrix): no native user-note storage; could store in account_data
    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_user_note: Matrix has no native user note system".to_string(),
        ))
    }

    // ── Conversation lifecycle ─────────────────────────────────────────────────

    /// Close a DM channel: leave the room, forget it, and remove from `m.direct`.
    ///
    /// Matrix does not have a "hide without leaving" concept for DMs, so this
    /// issues a leave + forget (matching how Element Web implements "close DM")
    /// and removes the room from the `m.direct` account data map.
    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()> {
        // Leave first (required before forget).
        self.http.leave_room(channel_id).await?;
        // Forget the room so it disappears from the room list.
        self.http.forget_room(channel_id).await?;
        // Remove from m.direct so it no longer appears as a DM.
        let me = self.current_user_id()?;
        let mut m_direct = self.http.fetch_m_direct(&me).await?;
        if let Some(obj) = m_direct.as_object_mut() {
            // m.direct maps other_user_id → [room_id, ...]; remove rooms matching channel_id.
            for rooms in obj.values_mut() {
                if let Some(arr) = rooms.as_array_mut() {
                    arr.retain(|v| v.as_str() != Some(channel_id));
                }
            }
            // Remove entries whose room list became empty.
            obj.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
        }
        self.http.put_m_direct(&me, &m_direct).await?;
        Ok(())
    }

    /// Mute a conversation via a room-level push rule (`dont_notify`).
    ///
    /// Matrix push rules have no native expiry; the `until` timestamp is
    /// documented but cannot be enforced by the homeserver — it is ignored here.
    async fn mute_conversation(
        &self,
        channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        self.http.put_room_push_rule_mute(channel_id).await
    }

    /// Unmute a conversation by deleting the room-level push rule.
    async fn unmute_conversation(&self, channel_id: &str) -> ClientResult<()> {
        self.http.delete_room_push_rule(channel_id).await
    }

    /// Leave a group DM via `POST /_matrix/client/v3/rooms/{roomId}/leave`.
    async fn leave_group_dm(&self, channel_id: &str) -> ClientResult<()> {
        self.http.leave_room(channel_id).await
    }

    /// Update a group DM's name and/or avatar.
    ///
    /// - `name`: sets `m.room.name` state event.
    /// - `avatar_url`: sets `m.room.avatar` state event. Non-`mxc://` URLs are
    ///   skipped with a warning because Matrix only accepts `mxc://` URIs for
    ///   room avatars.
    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        if let Some(n) = name {
            self.http.set_room_name(channel_id, n).await?;
        }
        if let Some(url) = avatar_url {
            if url.starts_with("mxc://") {
                self.http.set_room_avatar(channel_id, url).await?;
            } else {
                tracing::warn!(
                    "edit_group_dm(matrix): avatar_url {url:?} is not an mxc:// URI — skipped"
                );
            }
        }
        Ok(())
    }

    /// Add one or more users to a group DM (room) via per-user invites.
    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()> {
        for uid in user_ids {
            self.http.invite_to_room(channel_id, uid).await?;
        }
        Ok(())
    }

    /// Invite a user to a server (Matrix Space).
    ///
    /// Matrix has no "server invite" concept equivalent to Discord. The closest
    /// mapping is inviting to the Space room directly. If `server_id` looks like
    /// a Matrix room ID (`!...`) the invite is sent; otherwise `NotSupported` is
    /// returned.
    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()> {
        if server_id.starts_with('!') {
            self.http.invite_to_room(server_id, user_id).await
        } else {
            Err(ClientError::NotSupported(
                "invite_user_to_server: server_id is not a Matrix room ID; \
                 Matrix has no invite-link concept — pass the Space room ID instead"
                    .to_string(),
            ))
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
            landing: poly_client::LandingPage::Overview,
            // Moderation flags (B-MX)
            has_roles: false,        // Power levels are different; expose differently in UI
            has_kick: true,
            has_ban: true,
            has_timed_ban: false,    // No native timeout primitive
            has_channel_mgmt: true,  // Name + topic only
            has_moderation_log: false, // Expensive synthesis deferred
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
            // ── Noop actions (Server/Space, leave-room/dm, message ops, profile views)
            // All return Ok(Noop); merged into one arm to satisfy
            // clippy::match_same_arms.
            "space-settings"
            | "edit-per-space-profile"
            | "e2ee-verification"
            | "browse-rooms-in-space"
            | "add-room-to-space"
            | "leave-space"
            | "leave-room"
            | "leave-dm"
            | "open-dm"
            | "view-profile"
            | "verify-user"
            | "react-message"
            | "reply-in-thread"
            | "copy-permalink"
            | "redact-message" => Ok(ActionOutcome::Noop),

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
            // ── User — state mutations ───────────────────────────────────────
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

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-matrix-overview-title".to_string()),
                subtitle_key: Some("plugin-matrix-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if !channel_id.is_empty() && channel_id != "account-overview" {
            return Err(ClientError::NotSupported("view-rows not yet implemented".into()));
        }

        let joined = self.http.fetch_joined_rooms().await?;
        let homeserver_url = self.homeserver_url().to_string();

        let mut rows = Vec::new();
        for room_id in &joined.joined_rooms {
            let state = self.http.fetch_room_state(room_id).await.unwrap_or_default();

            let name = Self::extract_canonical_alias(&state)
                .unwrap_or_else(|| Self::extract_room_name(&state, room_id));
            let topic = Self::extract_room_topic(&state);
            let member_count = Self::count_joined_members(&state);
            let icon = Self::extract_avatar_url(&state, &homeserver_url);

            // Unread / mention counts are not tracked in-memory by this backend
            // (no persistent sync state). They default to 0.
            let unread: u32 = 0;
            let mentions: u32 = 0;

            let meta_text = format!(
                "{member_count} members · {unread} unread · @{mentions} mentions"
            );

            let is_space = Self::is_space_room(&state);
            rows.push(ViewRow {
                id: room_id.clone(),
                primary_text: name,
                secondary_text: topic,
                meta_text: Some(meta_text),
                icon,
                badge: None,
                preview_image_url: None,
                context_menu_target_kind: if is_space {
                    MenuTargetKind::Server
                } else {
                    MenuTargetKind::Channel
                },
            });
        }

        Ok(ViewRowsPage { rows, next_cursor: None })
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

    fn get_signup_method(&self, server_url: Option<&str>) -> SignupMethod {
        let base = server_url.unwrap_or("https://matrix.org");
        SignupMethod::External(format!("{}/_matrix/client/v3/register", base.trim_end_matches('/')))
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
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
