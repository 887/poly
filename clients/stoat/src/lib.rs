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
mod config;

#[cfg(feature = "native")]
mod http;

/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
pub use config::{OFFICIAL_STOAT_BASE_URL, StoatAuthInput, StoatConfig, StoatConfigError};
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use http::StoatHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use reqwest::{Method, RequestBuilder};
#[cfg(feature = "native")]
use std::pin::Pin;

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
        let _auth = StoatAuthInput::try_from(credentials)?;
        Err(ClientError::Internal(
            "Stoat auth transport is configured, but the login/session flow is not implemented yet"
                .into(),
        ))
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.clear_session()
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Ok(vec![])
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        Err(ClientError::NotFound(format!("Server {id}")))
    }

    async fn get_channels(&self, _server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(vec![])
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        Err(ClientError::NotFound(format!("Channel {id}")))
    }

    async fn send_message(
        &self,
        _channel_id: &str,
        _content: MessageContent,
    ) -> ClientResult<Message> {
        Err(ClientError::Internal(
            "Stoat client not yet implemented".into(),
        ))
    }

    async fn get_messages(
        &self,
        _channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Ok(vec![])
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
    use reqwest::Method;

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
}
