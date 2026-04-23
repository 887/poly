//! Poly Server [`ClientBackend`] implementation.
//!
//! Bridges the poly-server HTTP + WS clients into the [`ClientBackend`](poly_client::ClientBackend)
//! trait so poly-server instances appear as first-class accounts in the UI.
//!
//! ## Architecture
//!
//! ```text
//! PolyServerBackend
//!   ├── PolyServerHttpClient  — REST API (reqwest)
//!   └── PolyServerWsClient    — Real-time events (tokio-tungstenite)
//! ```

use async_trait::async_trait;
#[cfg(feature = "native")]
use chrono::Utc;
use futures::stream::Stream;
use std::pin::Pin;
use tracing::debug;

use crate::http::{PolyServerConfig, PolyServerHttpClient};
use crate::models::{self as srv, ChannelKind};
#[cfg(feature = "native")]
use crate::ws::PolyServerWsClient;
use poly_client::*;

/// A [`ClientBackend`] implementation for poly-server instances.
///
/// Wraps the HTTP and WebSocket clients, mapping poly-server wire types
/// to the unified `poly_client` types.
pub struct PolyServerBackend {
    /// HTTP client for REST API calls.
    http: PolyServerHttpClient,
    /// WebSocket client for real-time events (native only).
    #[cfg(feature = "native")]
    ws: PolyServerWsClient,
    /// Base URL of the server.
    base_url: String,
    /// Account ID assigned by the server after auth.
    account_id: Option<String>,
    /// Display name from the server profile.
    display_name: Option<String>,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
}

impl std::fmt::Debug for PolyServerBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolyServerBackend")
            .field("base_url", &self.base_url)
            .field("account_id", &self.account_id)
            .finish()
    }
}

impl PolyServerBackend {
    /// Create a new backend for a poly-server instance.
    ///
    /// `private_key_bytes` is the raw 32-byte Ed25519 signing key from the user's identity.
    pub fn new(base_url: &str, private_key_bytes: [u8; 32]) -> Self {
        let config = PolyServerConfig {
            base_url: base_url.to_string(),
            private_key_bytes,
        };
        let http = PolyServerHttpClient::new(config);
        #[cfg(feature = "native")]
        let ws = PolyServerWsClient::new(base_url, http.session_lock());
        Self {
            http,
            #[cfg(feature = "native")]
            ws,
            base_url: base_url.to_string(),
            account_id: None,
            display_name: None,
            settings_storage: SettingsStorageCell::new(),
        }
    }

    /// Get the base URL of the server.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the HTTP client for direct API calls.
    pub fn http(&self) -> &PolyServerHttpClient {
        &self.http
    }

    /// Map a server-wire `UserProfile` to a `poly_client::User`.
    fn map_user(profile: &srv::UserProfile) -> User {
        User {
            id: profile.id.clone(),
            display_name: profile.display_name.clone(),
            avatar_url: profile.avatar_url.clone(),
            presence: PresenceStatus::Offline,
            backend: BackendType::from("poly"),
        }
    }

    /// Map a server-wire `WireServer` to a `poly_client::Server`.
    fn map_server(
        srv: &srv::WireServer,
        categories: &[srv::WireCategory],
        account_id: &str,
        account_display_name: &str,
    ) -> Server {
        let id = srv.id.clone().unwrap_or_default();
        let cats = categories
            .iter()
            .filter(|c| c.server == id)
            .map(|c| Category {
                id: c.id.clone(),
                name: c.name.clone(),
                channel_ids: Vec::new(), // Populated when channels are loaded
            })
            .collect();

        Server {
            id,
            name: srv.name.clone(),
            icon_url: srv.icon_url.clone(),
            banner_url: srv.banner_url.clone(),
            categories: cats,
            backend: BackendType::from("poly"),
            unread_count: 0,
            mention_count: 0,
            account_id: account_id.to_string(),
            account_display_name: account_display_name.to_string(),
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            updated_at: None,
        }
    }

    /// Map a server-wire `WireChannel` to a `poly_client::Channel`.
    fn map_channel(ch: &srv::WireChannel) -> Channel {
        let channel_type = match ch.kind {
            ChannelKind::Text => ChannelType::Text,
            ChannelKind::Voice => ChannelType::Voice,
            ChannelKind::Forum => ChannelType::Forum,
        };
        Channel {
            id: ch.id.clone(),
            name: ch.name.clone(),
            channel_type,
            server_id: ch.server_id.clone().unwrap_or_default(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        }
    }

    /// Map a server-wire `WireMessage` to a `poly_client::Message`.
    ///
    /// `base_url` is needed to construct attachment download URLs.
    fn map_message(msg: &srv::WireMessage, base_url: &str) -> Message {
        let attachments = msg
            .attachments
            .iter()
            .map(|att| {
                Attachment::remote(
                    att.id.clone(),
                    att.filename.clone(),
                    att.mime_type.clone(),
                    format!("{base_url}/attachments/{}", att.id),
                    att.size_bytes,
                )
            })
            .collect();

        Message {
            id: msg.id.clone(),
            author: User {
                id: msg.author_id.clone(),
                display_name: msg.author_id.clone(), // Will be resolved later
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::from("poly"),
            },
            content: MessageContent::Text(msg.content.clone()),
            timestamp: msg.created_at,
            attachments,
            reactions: Vec::new(),
            reply_to: None,
            edited: msg.edited_at.is_some(),
            thread: None,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for PolyServerBackend {
    // ── Authentication ───────────────────────────────────────────────────────

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        match credentials {
            AuthCredentials::PolyServer {
                server_url: _,
                private_key_bytes: _,
                username,
                email,
                display_name,
                selected_user_id,
                is_signup,
            } => {
                let auth = if is_signup {
                    let uname = username.as_deref().unwrap_or("user");
                    let signup_email = email.as_deref().ok_or_else(|| {
                        ClientError::AuthFailed("email required for Poly Server signup".to_string())
                    })?;
                    self.http
                        .signup(uname, signup_email, display_name.as_deref())
                        .await
                        .map_err(|e| ClientError::AuthFailed(e.to_string()))?
                } else {
                    self.http
                        .signin(selected_user_id.as_deref())
                        .await
                        .map_err(|e| ClientError::AuthFailed(e.to_string()))?
                };

                self.account_id = Some(auth.user_id.clone());

                // Fetch the user's display name from the server.
                let profile = self
                    .http
                    .get_me()
                    .await
                    .map_err(|e| ClientError::Network(e.to_string()))?;
                self.display_name = Some(profile.display_name.clone());

                // Start WebSocket connection (native only — WASM uses polling).
                #[cfg(feature = "native")]
                self.ws.connect();

                Ok(Session {
                    id: auth.user_id.clone(),
                    user: Self::map_user(&profile),
                    token: auth.token,
                    backend: BackendType::from("poly"),
                    icon_emoji: Some("\u{1f536}".to_string()), // 🔶
                    // Strip "http(s)://" so instance_id is a URL-path-safe segment
                    // (e.g. "127.0.0.1:7080" or "my.poly.server.com").
                    instance_id: self
                        .base_url
                        .trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .trim_end_matches('/')
                        .to_string(),
                    // Full URL (with protocol) for token persistence + reconnect.
                    backend_url: Some(self.base_url.trim_end_matches('/').to_string()),
                })
            }
            AuthCredentials::Token(token) => {
                // Re-authenticate with an existing token (e.g. from storage).
                let _ = token;
                let auth = self
                    .http
                    .signin(None)
                    .await
                    .map_err(|e| ClientError::AuthFailed(e.to_string()))?;
                self.account_id = Some(auth.user_id.clone());

                let profile = self
                    .http
                    .get_me()
                    .await
                    .map_err(|e| ClientError::Network(e.to_string()))?;
                self.display_name = Some(profile.display_name.clone());
                #[cfg(feature = "native")]
                self.ws.connect();

                Ok(Session {
                    id: auth.user_id.clone(),
                    user: Self::map_user(&profile),
                    token: auth.token,
                    backend: BackendType::from("poly"),
                    icon_emoji: Some("\u{1f536}".to_string()),
                    // Strip "http(s)://" so instance_id is a URL-path-safe segment.
                    instance_id: self
                        .base_url
                        .trim_start_matches("https://")
                        .trim_start_matches("http://")
                        .trim_end_matches('/')
                        .to_string(),
                    // Full URL (with protocol) for token persistence + reconnect.
                    backend_url: Some(self.base_url.trim_end_matches('/').to_string()),
                })
            }
            _ => Err(ClientError::AuthFailed(
                "PolyServerBackend only supports PolyServer or Token credentials".into(),
            )),
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        #[cfg(feature = "native")]
        self.ws.disconnect();
        if let Err(e) = self.http.signout().await {
            debug!("Signout error (non-fatal): {e}");
        }
        self.account_id = None;
        self.display_name = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.account_id.is_some()
    }

    // ── Servers ──────────────────────────────────────────────────────────────

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let servers = self
            .http
            .get_servers()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let account_id = self.account_id.clone().unwrap_or_default();
        let display_name = self.display_name.clone().unwrap_or_default();

        Ok(servers
            .iter()
            .map(|s| Self::map_server(s, &[], &account_id, &display_name))
            .collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let detail = self
            .http
            .get_server(id)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let account_id = self.account_id.clone().unwrap_or_default();
        let display_name = self.display_name.clone().unwrap_or_default();

        // Build categories with channel IDs.
        let mut server = Self::map_server(
            &detail.server,
            &detail.categories,
            &account_id,
            &display_name,
        );

        // Populate channel_ids in categories.
        for cat in &mut server.categories {
            cat.channel_ids = detail
                .channels
                .iter()
                .filter(|ch| ch.category_id.as_deref() == Some(&cat.id))
                .map(|ch| ch.id.clone())
                .collect();
        }

        Ok(server)
    }

    // ── Channels ─────────────────────────────────────────────────────────────

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let channels = self
            .http
            .get_channels(server_id)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(channels.iter().map(Self::map_channel).collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        // The server has no dedicated GET /channels/:id endpoint.
        // Try to find the channel in the DM list (covers both DMs and group DMs).
        let dms = self
            .http
            .get_dm_channels()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        if let Some(ch) = dms.iter().find(|c| c.id == id) {
            return Ok(Self::map_channel(ch));
        }

        // Server channels require knowing the server_id; without it we cannot look
        // them up. Callers should use get_channels(server_id) instead.
        Err(ClientError::NotFound(format!("channel {id}")))
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        };

        let msg = self
            .http
            .send_message(channel_id, &text, None, None)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(Self::map_message(&msg, &self.base_url))
    }

    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        #[cfg(feature = "native")]
        {
            self.ws
                .send_typing(channel_id)
                .await
                .map_err(|e| ClientError::Network(e.to_string()))
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = channel_id;
            Ok(())
        }
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match &content {
            MessageContent::Text(t) => t.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        };

        let msg = self
            .http
            .send_message(channel_id, &text, Some(reply_to_message_id), None)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(Self::map_message(&msg, &self.base_url))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let msgs = self
            .http
            .get_messages(channel_id, query.limit, query.before.as_deref())
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(msgs
            .iter()
            .map(|m| Self::map_message(m, &self.base_url))
            .collect())
    }

    // ── Users ────────────────────────────────────────────────────────────────

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let profile = self
            .http
            .get_user(id)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        Ok(Self::map_user(&profile))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        let profiles = self
            .http
            .get_friends()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        Ok(profiles.iter().map(Self::map_user).collect())
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        let participants = self
            .http
            .get_participants(channel_id)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let mut users = Vec::new();
        for p in &participants {
            if let Ok(user) = self.get_user(&p.user).await {
                users.push(user);
            }
        }
        Ok(users)
    }

    // ── Groups ───────────────────────────────────────────────────────────────

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        // Group DMs are DM-like channels (no server_id) with a user-specified name.
        // We identify them by fetching all DM-kind channels and checking participant count:
        // >2 participants (including self) indicates a group DM.
        let channels = self
            .http
            .get_dm_channels()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let account_id = self.account_id.clone().unwrap_or_default();
        let mut groups = Vec::new();

        for ch in channels.iter().filter(|c| c.server_id.is_none()) {
            let participants = self
                .http
                .get_participants(&ch.id)
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            // A group DM has more than 2 participants (or has a name not matching a user).
            // We use participant count > 2 as the primary signal.
            if participants.len() > 2 {
                let mut members = Vec::with_capacity(participants.len());
                for p in &participants {
                    if let Ok(user) = self.http.get_user(&p.user).await {
                        members.push(Self::map_user(&user));
                    }
                }
                groups.push(Group {
                    id: ch.id.clone(),
                    name: Some(ch.name.clone()),
                    members,
                    last_message: None,
                    backend: BackendType::from("poly"),
                    account_id: account_id.clone(),
                });
            }
        }
        Ok(groups)
    }

    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        self.http
            .add_group_member(group_id, user_id)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    // ── DMs ──────────────────────────────────────────────────────────────────

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let channels = self
            .http
            .get_dm_channels()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let account_id = self.account_id.clone().unwrap_or_default();
        // Only keep single-participant DMs (no group DMs — those go through get_groups).
        let dm_channels: Vec<_> = channels
            .iter()
            .filter(|ch| ch.server_id.is_none())
            .collect();

        let mut result = Vec::with_capacity(dm_channels.len());
        for ch in dm_channels {
            // Resolve the other participant's profile.
            let participants = self
                .http
                .get_participants(&ch.id)
                .await
                .map_err(|e| ClientError::Network(e.to_string()))?;

            // The other participant is the one who isn't us.
            let other = participants.iter().find(|p| p.user != account_id);

            let user = if let Some(p) = other {
                self.http
                    .get_user(&p.user)
                    .await
                    .map(|profile| Self::map_user(&profile))
                    .unwrap_or_else(|_| User {
                        id: p.user.clone(),
                        display_name: ch.name.clone(),
                        avatar_url: None,
                        presence: PresenceStatus::Offline,
                        backend: BackendType::from("poly"),
                    })
            } else {
                // Fallback: use the channel name as display name.
                User {
                    id: String::new(),
                    display_name: ch.name.clone(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("poly"),
                }
            };

            result.push(DmChannel {
                id: ch.id.clone(),
                user,
                last_message: None,
                unread_count: 0,
                backend: BackendType::from("poly"),
                account_id: account_id.clone(),
            });
        }
        Ok(result)
    }

    // ── Notifications ────────────────────────────────────────────────────────

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
    }

    // ── Server management ────────────────────────────────────────────────────

    async fn create_server(&self, name: &str) -> ClientResult<Server> {
        let wire = self
            .http
            .create_server(name)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let account_id = self.account_id.clone().unwrap_or_default();
        let display_name = self.display_name.clone().unwrap_or_default();
        Ok(Self::map_server(&wire, &[], &account_id, &display_name))
    }

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        self.http
            .update_server_banner(server_id, banner_url)
            .await
            .map(|_| ())
            .map_err(|e| ClientError::Network(e.to_string()))
    }

    async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        let kind_str = match channel_type {
            ChannelType::Text | ChannelType::Thread | ChannelType::Announcement => "text",
            ChannelType::Voice | ChannelType::Video => "voice",
            ChannelType::Forum | ChannelType::HackerNews => "forum",
            ChannelType::Code => "code",
        };
        let wire = self
            .http
            .create_channel(server_id, name, kind_str, None)
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        Ok(Self::map_channel(&wire))
    }

    // ── Voice ────────────────────────────────────────────────────────────────

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
        Ok(())
    }

    // ── Events ───────────────────────────────────────────────────────────────

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(feature = "native")]
        {
            let rx = self.ws.subscribe();
            let stream = tokio_stream::wrappers::BroadcastStream::new(rx);

            Box::pin(futures::stream::StreamExt::filter_map(stream, |result| {
                let event = match result {
                    Ok(srv_event) => map_server_event(srv_event),
                    Err(_) => None,
                };
                async move { event }
            }))
        }
        #[cfg(not(feature = "native"))]
        {
            // WASM: no WebSocket support — return an empty stream.
            Box::pin(futures::stream::empty())
        }
    }

    // ── Backend info ─────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::from("poly")
    }

    fn backend_name(&self) -> &str {
        "Poly Server"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::FULL_SOCIAL_CHAT
    }

    // ── Client-provided UI surface (WP 1) ────────────────────────────────────

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        if target != MenuTargetKind::Server {
            return Ok(Vec::new());
        }
        Ok(vec![
            MenuItem {
                id: "invite-people".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-poly-menu-invite-people-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "privacy-settings".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-poly-menu-privacy-settings-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "edit-per-server-profile".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-poly-menu-edit-per-server-profile-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
            MenuItem {
                id: "federation-settings".to_string(),
                parent_id: None,
                slot: MenuSlot::AfterFavorites,
                label_key: "plugin-poly-menu-federation-settings-label".to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            },
        ])
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "invite-people" | "privacy-settings" | "edit-per-server-profile"
            | "federation-settings" => Ok(ActionOutcome::Noop),
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
                section_key: "profile".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "nickname".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "avatar-url".to_string(),
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
                    key: "allow-dms-from-server-members".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "true".to_string(),
                    extra: String::new(),
                }],
                info_block: None,
            },
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "federation".to_string(),
                icon: None,
                fields: vec![SettingDescriptor {
                    key: "allow-federation".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "true".to_string(),
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
            layout: SidebarLayoutKind::ChannelList,
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
        // poly-server is a generic host; composer extensions are declared by server-side plugins, not the client.
        Ok(Vec::new())
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        // poly-server is a generic host; message actions are declared by server-side plugins, not the client.
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

/// Map a poly-server `ServerEvent` to a `poly_client::ClientEvent`.
///
/// Only used with the `native` feature (WebSocket events).
#[cfg(feature = "native")]
fn map_server_event(event: srv::ServerEvent) -> Option<ClientEvent> {
    match event {
        srv::ServerEvent::MessageCreated(payload) => Some(ClientEvent::MessageReceived {
            channel_id: payload.channel_id.clone(),
            message: Message {
                id: payload.id,
                author: User {
                    id: payload.author_id.clone(),
                    display_name: payload.author_id,
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("poly"),
                },
                content: MessageContent::Text(payload.content),
                timestamp: payload.created_at,
                attachments: Vec::new(),
                reactions: Vec::new(),
                reply_to: None,
                edited: payload.edited_at.is_some(),
                thread: None,
            },
        }),
        srv::ServerEvent::MessageEdited(payload) => Some(ClientEvent::MessageEdited {
            channel_id: payload.channel_id.clone(),
            message: Message {
                id: payload.id,
                author: User {
                    id: payload.author_id.clone(),
                    display_name: payload.author_id,
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("poly"),
                },
                content: MessageContent::Text(payload.content),
                timestamp: payload.created_at,
                attachments: Vec::new(),
                reactions: Vec::new(),
                reply_to: None,
                edited: true,
                thread: None,
            },
        }),
        srv::ServerEvent::MessageDeleted {
            channel_id,
            message_id,
        } => Some(ClientEvent::MessageDeleted {
            channel_id,
            message_id,
        }),
        srv::ServerEvent::TypingStart { channel_id, user } => Some(ClientEvent::TypingStarted {
            channel_id,
            user_id: user.id,
            timestamp: Utc::now(),
        }),
        srv::ServerEvent::PresenceUpdate { user_id, online } => {
            Some(ClientEvent::PresenceChanged {
                user_id,
                status: if online {
                    PresenceStatus::Online
                } else {
                    PresenceStatus::Offline
                },
            })
        }
        srv::ServerEvent::FriendRequestReceived { from, .. } => {
            Some(ClientEvent::FriendRequestReceived {
                from_user: User {
                    id: from.id.clone(),
                    display_name: from.display_name,
                    avatar_url: from.avatar_url,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("poly"),
                },
            })
        }
        srv::ServerEvent::DeviceRevoked => Some(ClientEvent::ConnectionStateChanged {
            backend: BackendType::from("poly"),
            connected: false,
        }),
        srv::ServerEvent::VoiceStateUpdate {
            channel_id,
            user_id,
            joined,
        } => {
            if joined {
                Some(ClientEvent::VoiceUserJoined {
                    channel_id,
                    participant: VoiceParticipant {
                        user: User {
                            id: user_id,
                            display_name: String::new(),
                            avatar_url: None,
                            presence: PresenceStatus::Online,
                            backend: BackendType::from("poly"),
                        },
                        is_muted: false,
                        is_deafened: false,
                        is_streaming: false,
                        is_video_on: false,
                        is_speaking: false,
                    },
                })
            } else {
                Some(ClientEvent::VoiceUserLeft {
                    channel_id,
                    user_id,
                })
            }
        }
        // Server metadata updated — wrap into ServerUpdated client event.
        // We don't have a full Server struct here, so we emit a reduced channel update.
        // Future: expose a dedicated ServerMetaUpdated event in poly_client.
        srv::ServerEvent::ServerMemberJoined { .. }
        | srv::ServerEvent::ServerMemberLeft { .. }
        | srv::ServerEvent::ServerUpdated { .. }
        | srv::ServerEvent::ChannelCreated { .. }
        | srv::ServerEvent::ChannelDeleted { .. }
        | srv::ServerEvent::FriendRequestAccepted { .. }
        | srv::ServerEvent::VoiceSignalRelay { .. }
        | srv::ServerEvent::Ping => None,
        // ReactionAdded / ReactionRemoved — poly_client::ClientEvent has no reaction
        // variants yet. Events are intentionally dropped here until the trait adds them.
        srv::ServerEvent::ReactionAdded { .. } | srv::ServerEvent::ReactionRemoved { .. } => None,
    }
}
