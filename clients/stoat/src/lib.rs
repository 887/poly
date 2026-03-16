//! # poly-stoat
//!
//! Stoat (formerly Revolt) messenger client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using the Stoat REST and
//! WebSocket APIs from `developers.stoat.chat`.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.
//!
//! DECISION(D21): WASM Plugin Backends.

// TODO(phase-3.1): Implement Stoat client

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
pub use api::StoatRootConfig;
#[cfg(feature = "native")]
use api::{StoatChannelUnread, StoatSendMessageRequest, reply_preview_from_message};
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
pub use config::{OFFICIAL_STOAT_BASE_URL, StoatAuthInput, StoatConfig, StoatConfigError};
#[cfg(feature = "native")]
use futures::{
    future,
    stream::{self, Stream},
};
#[cfg(feature = "native")]
use http::StoatHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use reqwest::{Method, RequestBuilder};
#[cfg(feature = "native")]
use std::collections::HashMap;
#[cfg(feature = "native")]
use std::pin::Pin;
#[cfg(feature = "native")]
use uuid::Uuid;

/// Return the raw FTL translation source for the Stoat client plugin.
///
/// Mirrors the WIT `plugin-metadata.get-translations(locale)` export used by
/// the WASM plugin host.
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Stoat (Revolt) messenger client.
#[cfg(feature = "native")]
pub struct StoatClient {
    http: StoatHttpClient,
}

#[cfg(feature = "native")]
impl StoatClient {
    /// Create a new Stoat client pointed at the official Stoat API.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(StoatConfig::official())
    }

    /// Create a Stoat client for a custom instance.
    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self, StoatConfigError> {
        StoatConfig::new(base_url).map(Self::with_config)
    }

    /// Create a Stoat client from pre-validated configuration.
    #[must_use]
    pub fn with_config(config: StoatConfig) -> Self {
        Self {
            http: StoatHttpClient::new(config),
        }
    }

    /// Normalized REST API base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }

    /// Bonfire websocket URL derived from the configured API root.
    #[must_use]
    pub fn websocket_url(&self) -> String {
        self.http.websocket_url()
    }

    /// Stable instance identifier derived from the configured base URL.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.http.instance_id()
    }

    /// Inspect the currently loaded session token, if any.
    #[must_use]
    pub fn session_token(&self) -> Option<String> {
        self.http.session().map(|session| session.token)
    }

    /// Load a previously persisted Stoat session token into the transport.
    pub fn load_session_token(&self, token: String) -> ClientResult<()> {
        self.http.set_session_token(token)
    }

    /// Build a REST request against the configured Stoat API root.
    pub fn request_builder(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, path)
    }

    /// Build an authenticated request using the currently loaded Stoat token.
    pub fn authenticated_request_builder(
        &self,
        method: Method,
        path: &str,
    ) -> ClientResult<RequestBuilder> {
        self.http.authenticated_request(method, path)
    }

    /// Fetch Stoat instance configuration from `GET /`.
    pub async fn fetch_server_config(&self) -> ClientResult<StoatRootConfig> {
        self.http.fetch_server_config().await
    }

    fn build_session(&self, authenticated: api::StoatAuthenticatedSession) -> Session {
        Session {
            id: authenticated.session_id,
            user: authenticated.user,
            token: authenticated.token,
            backend: BackendType::Stoat,
            icon_emoji: Some("🦦".to_string()),
            instance_id: self.instance_id(),
            backend_url: Some(self.base_url().to_string()),
        }
    }

    fn current_account_metadata(&self) -> ClientResult<(String, String)> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;

        let account_id = session.user_id.ok_or_else(|| {
            ClientError::Internal("Stoat session is missing the authenticated user id".to_string())
        })?;

        let account_display_name = session
            .user_display_name
            .unwrap_or_else(|| account_id.clone());

        Ok((account_id, account_display_name))
    }

    fn index_unreads(unreads: Vec<StoatChannelUnread>) -> HashMap<String, StoatChannelUnread> {
        unreads
            .into_iter()
            .map(|unread| (unread.key.channel.clone(), unread))
            .collect()
    }

    fn current_user_id(&self) -> Option<String> {
        self.http.session().and_then(|session| session.user_id)
    }

    async fn send_message_internal(
        &self,
        channel_id: &str,
        content: MessageContent,
        reply_to_message_id: Option<&str>,
    ) -> ClientResult<Message> {
        let request = StoatSendMessageRequest::from_poly_content(
            content,
            reply_to_message_id.map(std::string::ToString::to_string),
            Uuid::new_v4().simple().to_string(),
        )?;

        let reply_lookup = async {
            if let Some(reply_id) = reply_to_message_id {
                self.http
                    .fetch_message(channel_id, reply_id)
                    .await
                    .map(Some)
            } else {
                Ok(None)
            }
        };

        let (raw_message, root_config, reply_message) = future::try_join3(
            self.http.send_message(channel_id, &request),
            self.http.fetch_server_config(),
            reply_lookup,
        )
        .await?;

        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);
        let current_user_id = self.current_user_id();
        let bundled_users = HashMap::new();
        let bundled_members = HashMap::new();

        let mut message = raw_message.into_poly_message(
            &bundled_users,
            &bundled_members,
            current_user_id.as_deref(),
            autumn_base_url.as_deref(),
        );

        if let Some(reply_message) = reply_message {
            let reply_preview_source = reply_message.into_poly_message(
                &bundled_users,
                &bundled_members,
                current_user_id.as_deref(),
                autumn_base_url.as_deref(),
            );
            message.reply_to = Some(reply_preview_from_message(&reply_preview_source));
        }

        Ok(message)
    }
}

#[cfg(feature = "native")]
impl Default for StoatClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for StoatClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let authenticated = match StoatAuthInput::try_from(credentials)? {
            StoatAuthInput::SessionToken(token) => self.http.authenticate_with_token(token).await?,
            StoatAuthInput::EmailPassword { email, password } => {
                self.http
                    .login_with_password(&email, &password, Some("Poly"))
                    .await?
            }
        };

        Ok(self.build_session(authenticated))
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.logout().await
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Err(ClientError::NotSupported(
            "Stoat joined-server discovery requires websocket ready-state or a dedicated collection endpoint".to_string(),
        ))
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let (server, unreads, root_config) = future::try_join3(
            self.http.fetch_server(id),
            self.http.fetch_unreads(),
            self.http.fetch_server_config(),
        )
        .await?;
        let (account_id, account_display_name) = self.current_account_metadata()?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url();

        let (unread_count, mention_count) = server
            .channels
            .iter()
            .filter_map(|channel_id| unread_index.get(channel_id))
            .fold((0_u32, 0_u32), |(unreads_acc, mentions_acc), unread| {
                (
                    unreads_acc.saturating_add(unread.approximate_unread_count()),
                    mentions_acc.saturating_add(unread.mention_count()),
                )
            });

        Ok(server.into_poly_server(
            account_id,
            account_display_name,
            unread_count,
            mention_count,
            autumn_base_url,
        ))
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let (server, unreads) =
            future::try_join(self.http.fetch_server(server_id), self.http.fetch_unreads()).await?;
        let unread_index = Self::index_unreads(unreads);

        let channels = future::try_join_all(
            server
                .channels
                .iter()
                .map(|channel_id| self.http.fetch_channel(channel_id)),
        )
        .await?;

        channels
            .into_iter()
            .map(|channel| {
                let unread = unread_index.get(&channel.id);
                let unread_count = unread
                    .map(StoatChannelUnread::approximate_unread_count)
                    .unwrap_or(0);
                let mention_count = unread.map(StoatChannelUnread::mention_count).unwrap_or(0);

                channel.into_poly_server_channel(unread_count, mention_count)
            })
            .collect()
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let (channel, unreads) =
            future::try_join(self.http.fetch_channel(id), self.http.fetch_unreads()).await?;
        let unread_index = Self::index_unreads(unreads);
        let unread = unread_index.get(&channel.id);
        let unread_count = unread
            .map(StoatChannelUnread::approximate_unread_count)
            .unwrap_or(0);
        let mention_count = unread.map(StoatChannelUnread::mention_count).unwrap_or(0);

        channel.into_poly_server_channel(unread_count, mention_count)
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        self.send_message_internal(channel_id, content, None).await
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        self.send_message_internal(channel_id, content, Some(reply_to_message_id))
            .await
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let (response, root_config) = future::try_join(
            self.http.fetch_messages(channel_id, &query),
            self.http.fetch_server_config(),
        )
        .await?;
        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);
        let current_user_id = self.current_user_id();
        let (raw_messages, bundled_users, bundled_members) = response.into_parts();

        let mut messages_with_replies: Vec<(Message, Option<String>)> = raw_messages
            .into_iter()
            .map(|raw| {
                let reply_id = raw.primary_reply_id().map(str::to_string);
                let message = raw.into_poly_message(
                    &bundled_users,
                    &bundled_members,
                    current_user_id.as_deref(),
                    autumn_base_url.as_deref(),
                );
                (message, reply_id)
            })
            .collect();

        let preview_index: HashMap<String, MessageReplyPreview> = messages_with_replies
            .iter()
            .map(|(message, _)| (message.id.clone(), reply_preview_from_message(message)))
            .collect();

        let mut messages: Vec<Message> = messages_with_replies
            .drain(..)
            .map(|(mut message, reply_id)| {
                message.reply_to =
                    reply_id.and_then(|reply_id| preview_index.get(&reply_id).cloned());
                message
            })
            .collect();

        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(messages)
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        Err(ClientError::NotFound(format!("User {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(vec![])
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
        // TODO(phase-3): Implement voice participant fetching for Stoat
        Ok(vec![])
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(stream::empty())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Stoat
    }

    fn backend_name(&self) -> &str {
        "Stoat"
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::{OFFICIAL_STOAT_BASE_URL, StoatClient};
    use crate::http::StoatSessionState;
    use axum::{
        Json, Router,
        extract::State,
        http::HeaderMap,
        response::IntoResponse,
        routing::{get, post},
    };
    use poly_client::{BackendType, ClientBackend, PresenceStatus};
    use reqwest::Method;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct TestServerState {
        captured_requests: Arc<Mutex<Vec<serde_json::Value>>>,
        captured_tokens: Arc<Mutex<Vec<String>>>,
    }

    async fn launch_test_server(
        state: TestServerState,
    ) -> Result<(String, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
        async fn root_config() -> impl IntoResponse {
            Json(json!({
                "revolt": "0.11.5",
                "ws": "wss://ws.example.test",
                "features": {}
            }))
        }

        async fn send_message(
            State(state): State<TestServerState>,
            headers: HeaderMap,
            Json(payload): Json<serde_json::Value>,
        ) -> impl IntoResponse {
            let response_content = payload
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let response_replies = payload.get("replies").cloned();

            if let Ok(mut requests) = state.captured_requests.lock() {
                requests.push(payload);
            }

            if let Some(token) = headers
                .get("x-session-token")
                .and_then(|value| value.to_str().ok())
                .map(std::string::ToString::to_string)
                && let Ok(mut tokens) = state.captured_tokens.lock()
            {
                tokens.push(token);
            }

            Json(json!({
                "_id": "01HZZZZZZZZZZZZZZZZZZZZZZZ",
                "channel": "channel_1",
                "author": "user_1",
                "content": response_content,
                "user": {
                    "_id": "user_1",
                    "username": "stoaty",
                    "discriminator": "0001",
                    "display_name": "Stoaty",
                    "online": true
                },
                "replies": response_replies.map(|replies| {
                    replies
                        .as_array()
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|entry| entry.get("id").cloned())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            }))
        }

        async fn fetch_message() -> impl IntoResponse {
            Json(json!({
                "_id": "01HYYYYYYYYYYYYYYYYYYYYYYY",
                "channel": "channel_1",
                "author": "user_2",
                "content": "Original reply target",
                "user": {
                    "_id": "user_2",
                    "username": "other",
                    "discriminator": "0002",
                    "display_name": "Other User",
                    "online": false
                }
            }))
        }

        let app = Router::new()
            .route("/", get(root_config))
            .route("/channels/{channel_id}/messages", post(send_message))
            .route(
                "/channels/{channel_id}/messages/{message_id}",
                get(fetch_message),
            )
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            let _ignored = axum::serve(listener, app).await;
        });

        Ok((format!("http://{addr}"), handle))
    }

    #[test]
    fn default_client_uses_official_instance() {
        let client = StoatClient::new();
        assert_eq!(client.base_url(), OFFICIAL_STOAT_BASE_URL);
        assert_eq!(
            client.websocket_url(),
            "wss://api.stoat.chat/ws".to_string()
        );
    }

    #[test]
    fn custom_client_exposes_instance_metadata() {
        let client = StoatClient::with_base_url("http://127.0.0.1:7001/api");
        assert_eq!(
            client.map(|stoat| {
                (
                    stoat.base_url().to_string(),
                    stoat.websocket_url(),
                    stoat.instance_id(),
                )
            }),
            Ok((
                "http://127.0.0.1:7001/api".to_string(),
                "ws://127.0.0.1:7001/api/ws".to_string(),
                "127.0.0.1:7001~api".to_string(),
            ))
        );
    }

    #[test]
    fn request_builder_uses_configured_base_url() {
        let client = StoatClient::with_base_url("https://chat.example.test/api");
        assert_eq!(
            client.map_err(|error| error.to_string()).and_then(|stoat| {
                stoat
                    .request_builder(Method::GET, "/servers")
                    .build()
                    .map(|request| request.url().to_string())
                    .map_err(|error| error.to_string())
            }),
            Ok("https://chat.example.test/api/servers".to_string())
        );
    }

    #[test]
    fn server_config_deserializes_through_public_type() {
        let config: Result<super::StoatRootConfig, _> = serde_json::from_value(json!({
            "revolt": "0.11.5",
            "ws": "wss://ws.example.test",
        }));

        assert!(matches!(
            config,
            Ok(super::StoatRootConfig { revolt, ws, .. })
                if revolt == "0.11.5" && ws == "wss://ws.example.test"
        ));
    }

    #[test]
    fn build_session_uses_stoat_backend_identity() {
        let session = StoatClient::with_base_url("https://chat.example.test/api").map(|client| {
            client.build_session(super::api::StoatAuthenticatedSession {
                session_id: "session_1".to_string(),
                user_id: "user_1".to_string(),
                token: "token_1".to_string(),
                user: poly_client::User {
                    id: "user_1".to_string(),
                    display_name: "Stoaty".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Online,
                    backend: BackendType::Stoat,
                },
                session_name: Some("Poly".to_string()),
            })
        });

        assert!(matches!(
            session,
            Ok(poly_client::Session {
                backend: BackendType::Stoat,
                instance_id,
                backend_url,
                ..
            }) if instance_id == "chat.example.test~api"
                && backend_url == Some("https://chat.example.test/api".to_string())
        ));
    }

    #[tokio::test]
    async fn send_message_posts_text_payload_and_maps_response()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state.clone()).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let sent = client
            .send_message(
                "channel_1",
                poly_client::MessageContent::Text("Hello Stoat".to_string()),
            )
            .await?;

        server_handle.abort();

        let requests = state
            .captured_requests
            .lock()
            .map_err(|_| "captured request lock poisoned")?;
        let first_request = requests.first().ok_or("missing captured request")?;
        let nonce_present = first_request
            .get("nonce")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|nonce| !nonce.is_empty());

        assert!(nonce_present);
        assert_eq!(first_request.get("content"), Some(&json!("Hello Stoat")));
        assert_eq!(sent.author.display_name, "Stoaty");
        assert_eq!(
            sent.content,
            poly_client::MessageContent::Text("Hello Stoat".to_string())
        );

        let tokens = state
            .captured_tokens
            .lock()
            .map_err(|_| "captured token lock poisoned")?;
        assert_eq!(tokens.first().map(String::as_str), Some("token_123"));

        Ok(())
    }

    #[tokio::test]
    async fn send_reply_message_includes_reply_intent_and_preview()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state.clone()).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let sent = poly_client::ClientBackend::send_reply_message(
            &client,
            "channel_1",
            "01HYYYYYYYYYYYYYYYYYYYYYYY",
            poly_client::MessageContent::Text("Reply text".to_string()),
        )
        .await?;

        server_handle.abort();

        let requests = state
            .captured_requests
            .lock()
            .map_err(|_| "captured request lock poisoned")?;
        let first_request = requests.first().ok_or("missing captured request")?;
        assert_eq!(first_request.get("content"), Some(&json!("Reply text")));
        assert_eq!(
            first_request.get("replies"),
            Some(&json!([{
                "id": "01HYYYYYYYYYYYYYYYYYYYYYYY",
                "mention": false,
                "fail_if_not_exists": true
            }]))
        );

        assert!(matches!(
            sent.reply_to,
            Some(poly_client::MessageReplyPreview { ref message_id, ref author_display_name, ref snippet, .. })
                if message_id == "01HYYYYYYYYYYYYYYYYYYYYYYY"
                    && author_display_name == "Other User"
                    && snippet == "Original reply target"
        ));

        Ok(())
    }

    #[tokio::test]
    async fn send_message_rejects_unuploaded_attachments() -> Result<(), Box<dyn std::error::Error>>
    {
        let client = StoatClient::new();
        let result = client
            .send_message(
                "channel_1",
                poly_client::MessageContent::WithAttachments {
                    text: "Hello Stoat".to_string(),
                    attachments: vec![poly_client::Attachment {
                        id: "attachment_1".to_string(),
                        filename: "hello.txt".to_string(),
                        content_type: "text/plain".to_string(),
                        url: "https://example.test/hello.txt".to_string(),
                        size: 5,
                    }],
                },
            )
            .await;

        assert!(matches!(
            result,
            Err(poly_client::ClientError::NotSupported(message))
                if message == "Stoat attachment upload is not implemented yet"
        ));

        Ok(())
    }
}
