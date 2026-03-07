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
use chrono::Utc;
use futures::stream::Stream;
use std::pin::Pin;
use tracing::debug;

use crate::http::{PolyServerConfig, PolyServerHttpClient};
use crate::models::{self as srv, ChannelKind};
use crate::ws::PolyServerWsClient;
use poly_client::*;

/// A [`ClientBackend`] implementation for poly-server instances.
///
/// Wraps the HTTP and WebSocket clients, mapping poly-server wire types
/// to the unified `poly_client` types.
pub struct PolyServerBackend {
    /// HTTP client for REST API calls.
    http: PolyServerHttpClient,
    /// WebSocket client for real-time events.
    ws: PolyServerWsClient,
    /// Base URL of the server.
    base_url: String,
    /// Account ID assigned by the server after auth.
    account_id: Option<String>,
    /// Display name from the server profile.
    display_name: Option<String>,
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
        let ws = PolyServerWsClient::new(base_url, http.session_lock());
        Self {
            http,
            ws,
            base_url: base_url.to_string(),
            account_id: None,
            display_name: None,
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
            backend: BackendType::Poly,
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
            banner_url: None, // Poly server protocol does not yet supply banner images
            categories: cats,
            backend: BackendType::Poly,
            unread_count: 0,
            account_id: account_id.to_string(),
            account_display_name: account_display_name.to_string(),
        }
    }

    /// Map a server-wire `WireChannel` to a `poly_client::Channel`.
    fn map_channel(ch: &srv::WireChannel) -> Channel {
        let channel_type = match ch.kind {
            ChannelKind::Text => ChannelType::Text,
            ChannelKind::Voice => ChannelType::Voice,
        };
        Channel {
            id: ch.id.clone(),
            name: ch.name.clone(),
            channel_type,
            server_id: ch.server_id.clone().unwrap_or_default(),
            unread_count: 0,
            last_message_id: None,
        }
    }

    /// Map a server-wire `WireMessage` to a `poly_client::Message`.
    fn map_message(msg: &srv::WireMessage) -> Message {
        Message {
            id: msg.id.clone(),
            author: User {
                id: msg.author_id.clone(),
                display_name: msg.author_id.clone(), // Will be resolved later
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: BackendType::Poly,
            },
            content: MessageContent::Text(msg.content.clone()),
            timestamp: msg.created_at,
            attachments: Vec::new(),
            reactions: Vec::new(),
            edited: msg.edited_at.is_some(),
        }
    }
}

#[async_trait]
impl ClientBackend for PolyServerBackend {
    // ── Authentication ───────────────────────────────────────────────────────

    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        match credentials {
            AuthCredentials::PolyServer {
                server_url: _,
                private_key_bytes: _,
                username,
                display_name,
                is_signup,
            } => {
                let auth = if is_signup {
                    let uname = username.as_deref().unwrap_or("user");
                    self.http
                        .signup(uname, display_name.as_deref())
                        .await
                        .map_err(|e| ClientError::AuthFailed(e.to_string()))?
                } else {
                    self.http
                        .signin()
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

                // Start WebSocket connection.
                self.ws.connect();

                Ok(Session {
                    id: auth.user_id.clone(),
                    user: Self::map_user(&profile),
                    token: auth.token,
                    backend: BackendType::Poly,
                    icon_emoji: Some("\u{1f536}".to_string()), // 🔶
                    instance_id: self.base_url.clone(),
                })
            }
            AuthCredentials::Token(token) => {
                // Re-authenticate with an existing token (e.g. from storage).
                let _ = token;
                let auth = self
                    .http
                    .signin()
                    .await
                    .map_err(|e| ClientError::AuthFailed(e.to_string()))?;
                self.account_id = Some(auth.user_id.clone());

                let profile = self
                    .http
                    .get_me()
                    .await
                    .map_err(|e| ClientError::Network(e.to_string()))?;
                self.display_name = Some(profile.display_name.clone());
                self.ws.connect();

                Ok(Session {
                    id: auth.user_id.clone(),
                    user: Self::map_user(&profile),
                    token: auth.token,
                    backend: BackendType::Poly,
                    icon_emoji: Some("\u{1f536}".to_string()),
                    instance_id: self.base_url.clone(),
                })
            }
            _ => Err(ClientError::AuthFailed(
                "PolyServerBackend only supports PolyServer or Token credentials".into(),
            )),
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
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
        // The server doesn't have a dedicated GET /channels/:id endpoint.
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

        Ok(Self::map_message(&msg))
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

        Ok(msgs.iter().map(Self::map_message).collect())
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
        Ok(Vec::new())
    }

    // ── DMs ──────────────────────────────────────────────────────────────────

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let channels = self
            .http
            .get_dm_channels()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;

        let account_id = self.account_id.clone().unwrap_or_default();

        Ok(channels
            .iter()
            .map(|ch| DmChannel {
                id: ch.id.clone(),
                user: User {
                    id: String::new(),
                    display_name: ch.name.clone(),
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::Poly,
                },
                last_message: None,
                unread_count: 0,
                backend: BackendType::Poly,
                account_id: account_id.clone(),
            })
            .collect())
    }

    // ── Notifications ────────────────────────────────────────────────────────

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(Vec::new())
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

    // ── Backend info ─────────────────────────────────────────────────────────

    fn backend_type(&self) -> BackendType {
        BackendType::Poly
    }

    fn backend_name(&self) -> &str {
        "Poly Server"
    }
}

/// Map a poly-server `ServerEvent` to a `poly_client::ClientEvent`.
fn map_server_event(event: srv::ServerEvent) -> Option<ClientEvent> {
    match event {
        srv::ServerEvent::MessageCreated(payload) => Some(ClientEvent::MessageReceived {
            channel_id: payload.channel_id,
            message: Message {
                id: payload.id,
                author: User {
                    id: payload.author_id.clone(),
                    display_name: payload.author_id,
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::Poly,
                },
                content: MessageContent::Text(payload.content),
                timestamp: payload.created_at,
                attachments: Vec::new(),
                reactions: Vec::new(),
                edited: payload.edited_at.is_some(),
            },
        }),
        srv::ServerEvent::MessageEdited(payload) => Some(ClientEvent::MessageEdited {
            channel_id: payload.channel_id,
            message: Message {
                id: payload.id,
                author: User {
                    id: payload.author_id.clone(),
                    display_name: payload.author_id,
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::Poly,
                },
                content: MessageContent::Text(payload.content),
                timestamp: payload.created_at,
                attachments: Vec::new(),
                reactions: Vec::new(),
                edited: true,
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
                    backend: BackendType::Poly,
                },
            })
        }
        srv::ServerEvent::DeviceRevoked => Some(ClientEvent::ConnectionStateChanged {
            backend: BackendType::Poly,
            connected: false,
        }),
        // Events we don't map yet (voice, server updates, etc).
        _ => None,
    }
}
