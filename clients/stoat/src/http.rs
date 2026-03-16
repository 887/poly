//! Native HTTP transport scaffolding for the Stoat backend.
//!
//! This file only manages connection/session plumbing. Endpoint-specific API
//! methods are added in later increments so each step remains small and easy to
//! resume after interruptions.

use crate::config::StoatConfig;
use poly_client::{ClientError, ClientResult};
use reqwest::{Client, Method, RequestBuilder};
use std::sync::{Arc, RwLock};

const STOAT_SESSION_TOKEN_HEADER: &str = "x-session-token";

/// Minimal authenticated Stoat session state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoatSessionState {
    /// Session token returned by the Stoat auth API.
    pub token: String,
}

/// reqwest-backed HTTP transport for one Stoat instance.
#[derive(Debug, Clone)]
pub struct StoatHttpClient {
    config: StoatConfig,
    http: Client,
    session: Arc<RwLock<Option<StoatSessionState>>>,
}

impl StoatHttpClient {
    /// Create a new transport for the provided instance configuration.
    #[must_use]
    pub fn new(config: StoatConfig) -> Self {
        Self {
            config,
            http: Client::new(),
            session: Arc::new(RwLock::new(None)),
        }
    }

    /// Normalized REST API base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.config.base_url()
    }

    /// Bonfire websocket endpoint derived from the API root.
    #[must_use]
    pub fn websocket_url(&self) -> String {
        self.config.websocket_url()
    }

    /// Stable instance identifier derived from the configured base URL.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.config.instance_id()
    }

    /// Whether a session token is currently loaded.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.session
            .read()
            .map(|session| session.is_some())
            .unwrap_or(false)
    }

    /// Read the current session state, if present.
    #[must_use]
    pub fn session(&self) -> Option<StoatSessionState> {
        self.session.read().ok().and_then(|session| session.clone())
    }

    /// Replace the current session token.
    pub fn set_session_token(&self, token: String) -> ClientResult<()> {
        let mut session = self
            .session
            .write()
            .map_err(|_| ClientError::Internal("Stoat session lock poisoned".to_string()))?;
        *session = Some(StoatSessionState { token });
        Ok(())
    }

    /// Clear any authenticated session state.
    pub fn clear_session(&self) -> ClientResult<()> {
        let mut session = self
            .session
            .write()
            .map_err(|_| ClientError::Internal("Stoat session lock poisoned".to_string()))?;
        *session = None;
        Ok(())
    }

    /// Create an unauthenticated HTTP request builder.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, self.config.rest_url(path))
    }

    /// Create an authenticated request builder using Stoat's session header.
    pub fn authenticated_request(
        &self,
        method: Method,
        path: &str,
    ) -> ClientResult<RequestBuilder> {
        let token = self.session().map(|session| session.token).ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;

        Ok(self
            .request(method, path)
            .header(STOAT_SESSION_TOKEN_HEADER, token))
    }
}

#[cfg(test)]
mod tests {
    use super::StoatHttpClient;
    use crate::config::StoatConfig;
    use reqwest::Method;

    #[test]
    fn request_uses_normalized_base_url() {
        let client = StoatConfig::new("https://chat.example.test/api/")
            .map(StoatHttpClient::new)
            .map_err(|error| error.to_string())
            .and_then(|http| {
                http.request(Method::GET, "servers")
                    .build()
                    .map(|request| request.url().to_string())
                    .map_err(|error| error.to_string())
            });
        assert_eq!(
            client,
            Ok("https://chat.example.test/api/servers".to_string())
        );
    }

    #[test]
    fn authenticated_request_injects_stoat_session_header() {
        let client = StoatConfig::new("https://chat.example.test/api")
            .map(StoatHttpClient::new)
            .map_err(|error| error.to_string())
            .and_then(|http| {
                http.set_session_token("session-123".to_string())
                    .map_err(|error| error.to_string())?;
                http.authenticated_request(Method::GET, "/servers")
                    .map_err(|error| error.to_string())?
                    .build()
                    .map_err(|error| error.to_string())
                    .and_then(|request| {
                        request
                            .headers()
                            .get("x-session-token")
                            .and_then(|value| value.to_str().ok())
                            .map(std::string::ToString::to_string)
                            .ok_or_else(|| "missing x-session-token header".to_string())
                    })
            });
        assert_eq!(client, Ok("session-123".to_string()));
    }

    #[test]
    fn clear_session_resets_authenticated_state() {
        let client = StoatConfig::new("https://chat.example.test/api")
            .map(StoatHttpClient::new)
            .map_err(|error| error.to_string())
            .and_then(|http| {
                http.set_session_token("session-123".to_string())
                    .map_err(|error| error.to_string())?;
                http.clear_session().map_err(|error| error.to_string())?;
                Ok(http.is_authenticated())
            });
        assert_eq!(client, Ok(false));
    }
}
