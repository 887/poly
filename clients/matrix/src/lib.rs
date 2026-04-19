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
}

#[cfg(feature = "native")]
impl MatrixClient {
    /// Create a new Matrix client for the default homeserver (matrix.org).
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: MatrixHttpClient::new(MatrixConfig::default_homeserver()),
            settings_storage: SettingsStorageCell::new(),
        }
    }

    /// Create a new Matrix client for a custom homeserver URL.
    pub fn with_homeserver(url: impl Into<String>) -> Result<Self, MatrixConfigError> {
        let config = MatrixConfig::new(url)?;
        Ok(Self {
            http: MatrixHttpClient::new(config),
            settings_storage: SettingsStorageCell::new(),
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
        })
    }

    fn extract_body(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
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
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        match target {
            MenuTargetKind::Server => Ok(vec![
                MenuItem {
                    id: "space-settings".into(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-matrix-menu-space-settings-label".into(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "edit-per-space-profile".into(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-matrix-menu-edit-per-space-profile-label".into(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "e2ee-verification".into(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-matrix-menu-e2ee-verification-label".into(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "explore-rooms".into(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-matrix-menu-explore-rooms-label".into(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
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
            "space-settings" | "edit-per-space-profile" | "e2ee-verification"
            | "explore-rooms" => Ok(ActionOutcome::Noop),
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
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::SpacesRooms,
            sections: Vec::new(),
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
}
