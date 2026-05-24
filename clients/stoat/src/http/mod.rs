//! Native HTTP transport scaffolding for the Stoat backend.
//!
//! This module holds the [`StoatHttpClient`] struct + connection/session
//! plumbing. Endpoint-specific methods live in domain sub-modules
//! ([`auth`], [`channels`], [`messages`], [`moderation`], [`social`]) and
//! attach to the same struct via additional `impl` blocks.
//!
//! Split layout introduced in SOLID-audit-stoat D.3.

use crate::config::StoatConfig;
use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::{HttpClient, HttpError, Method, RequestBuilder, Response};
use serde_json::Value;
use std::sync::{Arc, RwLock};

mod auth;
mod channels;
mod messages;
mod moderation;
mod social;

/// Default User-Agent for Stoat API requests.
pub const DEFAULT_CLIENT_VERSION: &str = "poly-stoat/0.0.0";

const STOAT_SESSION_TOKEN_HEADER: &str = "x-session-token";

/// Minimal authenticated Stoat session state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoatSessionState {
    /// Session token returned by the Stoat auth API.
    pub token: String,
    /// Optional session ID when known.
    pub session_id: Option<String>,
    /// Optional user ID when known.
    pub user_id: Option<String>,
    /// Display name of the authenticated account when known.
    pub user_display_name: Option<String>,
}

/// reqwest-backed HTTP transport for one Stoat instance.
#[derive(Debug, Clone)]
pub struct StoatHttpClient {
    config: StoatConfig,
    http: HttpClient,
    session: Arc<RwLock<Option<StoatSessionState>>>,
    /// WebSocket URL obtained from the server's root config (GET /).
    /// Set after successful authentication.
    ws_url: Arc<RwLock<Option<String>>>,
    user_agent: Arc<RwLock<String>>,
}

impl StoatHttpClient {
    /// Create a new transport for the provided instance configuration.
    #[must_use]
    pub fn new(config: StoatConfig) -> Self {
        Self {
            config,
            http: HttpClient::new(),
            session: Arc::new(RwLock::new(None)),
            ws_url: Arc::new(RwLock::new(None)),
            user_agent: Arc::new(RwLock::new(DEFAULT_CLIENT_VERSION.to_string())),
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

    /// WebSocket URL as returned by the server's root config (GET /).
    /// Populated after successful authentication. Used by both the native
    /// and WASM event-stream paths in `lib.rs` to open the Bonfire WS.
    #[must_use]
    pub fn ws_url(&self) -> Option<String> {
        self.ws_url.read().ok().and_then(|g| g.clone())
    }

    /// Store the WebSocket URL obtained from the server's root config.
    pub fn set_ws_url(&self, url: String) {
        if let Ok(mut guard) = self.ws_url.write() {
            *guard = Some(url);
        }
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
        self.set_session(StoatSessionState {
            token,
            session_id: None,
            user_id: None,
            user_display_name: None,
        })
    }

    /// Clear any authenticated session state.
    pub fn clear_session(&self) -> ClientResult<()> {
        let mut session = self
            .session
            .write()
            .map_err(|_err| ClientError::Internal("Stoat session lock poisoned".to_string()))?;
        *session = None;
        Ok(())
    }

    /// Replace the full authenticated session state.
    pub fn set_session(&self, session_state: StoatSessionState) -> ClientResult<()> {
        let mut session = self
            .session
            .write()
            .map_err(|_err| ClientError::Internal("Stoat session lock poisoned".to_string()))?;
        *session = Some(session_state);
        Ok(())
    }


    /// Update the User-Agent string.
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut guard) = self.user_agent.write() {
            *guard = ua;
        }
    }

    fn ua(&self) -> String {
        self.user_agent
            .read()
            .ok().map_or_else(|| DEFAULT_CLIENT_VERSION.to_string(), |g| g.clone())
    }

    /// Create an unauthenticated HTTP request builder.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, self.config.rest_url(path))
            .header("User-Agent", self.ua())
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

    fn network_error(error: &HttpError) -> ClientError {
        ClientError::Network(error.to_string())
    }

    async fn parse_error(response: Response) -> ClientError {
        let status = response.status();
        let retry_after_ms = response
            .headers()
            .get("retry-after")
            .and_then(|header| header.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| seconds.saturating_mul(1000));

        let payload = response.json::<Value>().await.ok();
        let detail = payload
            .as_ref()
            .and_then(extract_error_detail)
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));

        match status.as_u16() {
            401 => ClientError::AuthFailed(detail),
            403 => ClientError::PermissionDenied(detail),
            404 => ClientError::NotFound(detail),
            429 => ClientError::RateLimited {
                retry_after_ms: retry_after_ms.unwrap_or(1000),
            },
            _ => ClientError::Network(detail),
        }
    }
}

/// Hand-encode a single-file `multipart/form-data` body so we can ship it
/// through the host bridge as a raw byte body. The host bridge protocol
/// doesn't have a multipart variant, so we serialize once on the WASM side
/// and let the native shell forward the bytes verbatim.
fn encode_multipart_file(
    boundary: &str,
    field_name: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::with_capacity(bytes.len().saturating_add(256));
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{filename}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    body
}

fn extract_error_detail(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::StoatHttpClient;
    use crate::api::StoatRootConfig;
    use crate::config::StoatConfig;
    use poly_host_bridge::http::Method;
    use serde_json::json;

    #[test]
    fn request_uses_normalized_base_url() {
        let client = StoatConfig::new("https://chat.example.test/api/")
            .map(StoatHttpClient::new)
            .map_err(|error| error.to_string())
            .map(|http| {
                http.request(Method::GET, "servers")
                    .url_ref()
                    .to_string()
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
                let builder = http
                    .authenticated_request(Method::GET, "/servers")
                    .map_err(|error| error.to_string())?;
                builder
                    .header_value("x-session-token")
                    .map(std::string::ToString::to_string)
                    .ok_or_else(|| "missing x-session-token header".to_string())
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

    #[test]
    fn set_session_token_preserves_authentication_state() {
        let client = StoatConfig::new("https://chat.example.test/api")
            .map(StoatHttpClient::new)
            .map_err(|error| error.to_string())
            .and_then(|http| {
                http.set_session_token("session-456".to_string())
                    .map_err(|error| error.to_string())?;
                Ok(http.session().map(|session| session.token))
            });

        assert_eq!(client, Ok(Some("session-456".to_string())));
    }

    #[test]
    fn extract_error_detail_prefers_error_then_type_then_message() {
        assert_eq!(
            super::extract_error_detail(&json!({"error": "InvalidCredentials"})),
            Some("InvalidCredentials".to_string())
        );
        assert_eq!(
            super::extract_error_detail(&json!({"type": "Disabled"})),
            Some("Disabled".to_string())
        );
        assert_eq!(
            super::extract_error_detail(&json!({"message": "boom"})),
            Some("boom".to_string())
        );
    }

    #[test]
    fn root_config_deserializes_minimal_payload() {
        let config: Result<StoatRootConfig, _> = serde_json::from_value(json!({
            "revolt": "0.11.5",
            "ws": "wss://ws.example.test",
        }));

        assert!(matches!(
            config,
            Ok(StoatRootConfig { revolt, ws, .. })
                if revolt == "0.11.5" && ws == "wss://ws.example.test"
        ));
    }
}
