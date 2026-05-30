//! # poly-matrix
//!
//! Matrix messenger client for Poly.
//!
//! Implements [`poly_client::IsBackend`] using the Matrix client-server
//! HTTP API directly (no matrix-sdk). Maps Matrix Spaces to Poly servers,
//! Matrix rooms to channels.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.
//!
//! DECISION(D21): WASM Plugin Backends.


/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "matrix";

#[cfg(feature = "native")]
mod api;

#[cfg(feature = "native")]
mod config;

#[cfg(feature = "native")]
mod http;

#[cfg(feature = "native")]
mod moderation_log;

#[cfg(feature = "native")]
pub mod signup;

/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

// ── SOLID D.3 — per-trait sibling modules ────────────────────────────────────

#[cfg(feature = "native")]
mod is_backend;

#[cfg(feature = "native")]
mod moderation;

#[cfg(feature = "native")]
mod social_graph;

#[cfg(feature = "native")]
mod dms_groups;

#[cfg(feature = "native")]
mod messaging;

#[cfg(feature = "native")]
mod server_admin;

#[cfg(feature = "native")]
mod settings;

#[cfg(feature = "native")]
mod view_descriptor;

#[cfg(feature = "native")]
mod context_action;

// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "native")]
pub use config::{DEFAULT_HOMESERVER_URL, MatrixAuthInput, MatrixConfig, MatrixConfigError};

#[cfg(feature = "native")]
use http::{MatrixHttpClient, MatrixSessionState};
#[cfg(feature = "native")]
use poly_client::{SettingsStorageCell, Session, User, PresenceStatus, BackendType, ClientResult, ClientError, Message, MessageContent, SidebarItem, IconSource, SidebarRouteKind, MenuSlot, MenuItemVariant, MenuItem};
#[cfg(feature = "native")]
use std::collections::HashSet;

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
    pub(crate) http: MatrixHttpClient,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    pub(crate) settings_storage: SettingsStorageCell,
    /// F10 — in-memory muted room ids. Persistent storage is F9 — out of scope.
    pub(crate) muted_rooms: std::sync::RwLock<HashSet<String>>,
    /// F10 — in-memory ignored user ids.
    pub(crate) ignored_users: std::sync::RwLock<HashSet<String>>,
    /// F10 — rooms the user has explicitly marked read via the context menu.
    pub(crate) marked_read: std::sync::RwLock<HashSet<String>>,
    /// Stored version override (None = use http::DEFAULT_CLIENT_VERSION).
    pub(crate) version_override: std::sync::Mutex<Option<String>>,
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

    pub(crate) fn build_session(
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
                backend: BackendType::from(crate::SLUG),
            },
            token: session_state.access_token.clone(),
            backend: BackendType::from(crate::SLUG),
            icon_emoji: Some("🟦".to_string()),
            instance_id: self.instance_id(),
            backend_url: Some(self.homeserver_url().to_string()),
        }
    }

    pub(crate) fn current_user_id(&self) -> ClientResult<String> {
        self.http
            .session()
            .map(|s| s.user_id)
            .ok_or_else(|| ClientError::AuthFailed("not logged in".into()))
    }

    /// The session/device ID used as the account key in the app.
    /// Must match `Session::id` (= `device_id`) so sidebar lookups find the
    /// right session.
    pub(crate) fn current_account_id(&self) -> ClientResult<String> {
        self.http
            .session()
            .map(|s| s.device_id)
            .ok_or_else(|| ClientError::AuthFailed("not logged in".into()))
    }

    pub(crate) fn room_event_to_message(event: &api::RoomEvent) -> Option<Message> {
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
                backend: BackendType::from(crate::SLUG),
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

    pub(crate) fn extract_room_name(state: &[api::RoomEvent], fallback: &str) -> String {
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
    pub(crate) fn extract_avatar_url(state: &[api::RoomEvent], homeserver_url: &str) -> Option<String> {
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
    pub(crate) async fn hydrate_message_authors(&self, mut messages: Vec<Message>) -> Vec<Message> {
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

    pub(crate) fn is_space_room(state: &[api::RoomEvent]) -> bool {
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
    pub(crate) fn cache_session_profile(&self, profile: &api::ProfileResponse) {
        let display_name = profile.displayname.clone();
        let avatar_url = profile
            .avatar_url
            .as_deref()
            .map(|raw| mxc_to_http_thumbnail(raw, self.homeserver_url()));
        drop(self.http.update_session_profile(display_name, avatar_url));
    }

    pub(crate) fn build_message_from_send(
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
                avatar_url: session.avatar_url,
                presence: PresenceStatus::Online,
                backend: BackendType::from(crate::SLUG),
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

    pub(crate) fn extract_body(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text)
            | MessageContent::WithAttachments { text, .. } => text.clone(),
        }
    }

    /// Extract the room topic from state events (`m.room.topic`).
    pub(crate) fn extract_room_topic(state: &[api::RoomEvent]) -> Option<String> {
        state
            .iter()
            .find(|ev| ev.event_type == "m.room.topic")
            .and_then(|ev| ev.content.get("topic"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    /// Extract the canonical alias from state events (`m.room.canonical_alias`).
    pub(crate) fn extract_canonical_alias(state: &[api::RoomEvent]) -> Option<String> {
        state
            .iter()
            .find(|ev| ev.event_type == "m.room.canonical_alias")
            .and_then(|ev| ev.content.get("alias"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    /// Count joined members from state events (`m.room.member` with
    /// `membership == "join"`).
    pub(crate) fn count_joined_members(state: &[api::RoomEvent]) -> usize {
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
    pub(crate) async fn fetch_space_tree(&self) -> ClientResult<Vec<SpaceTreeEntry>> {
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

/// F10 — shared menu-item builder (used by ContextActionBackend impl).
#[cfg(feature = "native")]
impl MatrixClient {
    pub(crate) fn simple_item(
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;
    // `is_authenticated` is provided by the `IsBackend` trait impl in
    // `is_backend.rs`; bring the trait into scope so the unit tests can call it.
    use poly_client::IsBackend;

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
