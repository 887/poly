//! Native HTTP transport scaffolding for the Stoat backend.
//!
//! This file only manages connection/session plumbing. Endpoint-specific API
//! methods are added in later increments so each step remains small and easy to
//! resume after interruptions.

use crate::api::{
    StoatAllMemberResponse, StoatAuthenticatedSession, StoatAutumnUploadResponse,
    StoatBulkMessageResponse, StoatChannel, StoatChannelUnread, StoatLoginResponse, StoatMessage,
    StoatPasswordLoginRequest, StoatRootConfig, StoatSendMessageRequest, StoatServer, StoatUser,
};
use crate::config::StoatConfig;
use poly_client::{Attachment, ClientError, ClientResult, MessageQuery};
use reqwest::{Client, Method, RequestBuilder, multipart};
use serde_json::Value;
use std::sync::{Arc, RwLock};

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
            .map_err(|_| ClientError::Internal("Stoat session lock poisoned".to_string()))?;
        *session = None;
        Ok(())
    }

    /// Replace the full authenticated session state.
    pub fn set_session(&self, session_state: StoatSessionState) -> ClientResult<()> {
        let mut session = self
            .session
            .write()
            .map_err(|_| ClientError::Internal("Stoat session lock poisoned".to_string()))?;
        *session = Some(session_state);
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

    /// Fetch root instance configuration.
    pub async fn fetch_server_config(&self) -> ClientResult<StoatRootConfig> {
        let response = self
            .request(Method::GET, "/")
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Authenticate with email/password and populate session state.
    pub async fn login_with_password(
        &self,
        email: &str,
        password: &str,
        friendly_name: Option<&str>,
    ) -> ClientResult<StoatAuthenticatedSession> {
        let response = self
            .request(Method::POST, "/auth/session/login")
            .json(&StoatPasswordLoginRequest {
                email: email.to_string(),
                password: password.to_string(),
                friendly_name: friendly_name.map(std::string::ToString::to_string),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        let login = response
            .json::<StoatLoginResponse>()
            .await
            .map_err(Self::network_error)?
            .into_success()?;

        let user = self.fetch_self_with_token(&login.token).await?;
        let authenticated = StoatAuthenticatedSession {
            session_id: login.session_id,
            user_id: login.user_id,
            token: login.token.clone(),
            user: user.into_poly_user(),
            session_name: login.session_name,
        };

        self.set_session(StoatSessionState {
            token: authenticated.token.clone(),
            session_id: Some(authenticated.session_id.clone()),
            user_id: Some(authenticated.user_id.clone()),
            user_display_name: Some(authenticated.user.display_name.clone()),
        })?;

        Ok(authenticated)
    }

    /// Restore an already-issued session token and resolve the current user.
    pub async fn authenticate_with_token(
        &self,
        token: String,
    ) -> ClientResult<StoatAuthenticatedSession> {
        let user = self.fetch_self_with_token(&token).await?;
        let session = StoatAuthenticatedSession {
            // TODO(phase-3.1.2.2): fetch session inventory from Stoat when we
            // need an exact session identifier for token-restore flows.
            session_id: user.id.clone(),
            user_id: user.id.clone(),
            token: token.clone(),
            user: user.into_poly_user(),
            session_name: None,
        };

        self.set_session(StoatSessionState {
            token,
            session_id: Some(session.session_id.clone()),
            user_id: Some(session.user_id.clone()),
            user_display_name: Some(session.user.display_name.clone()),
        })?;

        Ok(session)
    }

    /// Fetch a Stoat server by ID.
    pub async fn fetch_server(&self, server_id: &str) -> ClientResult<StoatServer> {
        let response = self
            .authenticated_request(Method::GET, &format!("/servers/{server_id}"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch all members for a Stoat server.
    pub async fn fetch_server_members(
        &self,
        server_id: &str,
    ) -> ClientResult<StoatAllMemberResponse> {
        let response = self
            .authenticated_request(Method::GET, &format!("/servers/{server_id}/members"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch a Stoat channel by ID.
    pub async fn fetch_channel(&self, channel_id: &str) -> ClientResult<StoatChannel> {
        let response = self
            .authenticated_request(Method::GET, &format!("/channels/{channel_id}"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch a Stoat user by ID.
    pub async fn fetch_user(&self, user_id: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::GET, &format!("/users/{user_id}"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch unread metadata for the authenticated account.
    pub async fn fetch_unreads(&self) -> ClientResult<Vec<StoatChannelUnread>> {
        let response = self
            .authenticated_request(Method::GET, "/sync/unreads")?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch messages for a channel using Poly's generic message query.
    pub async fn fetch_messages(
        &self,
        channel_id: &str,
        query: &MessageQuery,
    ) -> ClientResult<StoatBulkMessageResponse> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(limit) = query.limit {
            params.push(("limit", limit.to_string()));
        }

        if let Some(around) = &query.around {
            params.push(("nearby", around.clone()));
        } else {
            if let Some(before) = &query.before {
                params.push(("before", before.clone()));
            }
            if let Some(after) = &query.after {
                params.push(("after", after.clone()));
            }

            let sort = if query.after.is_some() {
                "Oldest"
            } else {
                "Latest"
            };
            params.push(("sort", sort.to_string()));
        }

        params.push(("include_users", "true".to_string()));
        let mut path = format!("/channels/{channel_id}/messages");
        if !params.is_empty() {
            path.push('?');
            path.push_str(
                &params
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }

        let response = self
            .authenticated_request(Method::GET, &path)?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch a single Stoat message by channel and message ID.
    pub async fn fetch_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<StoatMessage> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/channels/{channel_id}/messages/{message_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Send a text/reply message to a Stoat channel.
    pub async fn send_message(
        &self,
        channel_id: &str,
        payload: &StoatSendMessageRequest,
    ) -> ClientResult<StoatMessage> {
        let response = self
            .authenticated_request(Method::POST, &format!("/channels/{channel_id}/messages"))?
            .json(payload)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Upload one outbound attachment to the Stoat Autumn file service.
    pub async fn upload_attachment(
        &self,
        autumn_base_url: &str,
        attachment: &Attachment,
    ) -> ClientResult<String> {
        let token = self.session().map(|session| session.token).ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;
        let upload_bytes = attachment.upload_bytes.clone().ok_or_else(|| {
            ClientError::NotSupported("Stoat attachment send requires raw upload bytes".to_string())
        })?;

        let part = multipart::Part::bytes(upload_bytes)
            .file_name(attachment.filename.clone())
            .mime_str(&attachment.content_type)
            .map_err(|err| {
                ClientError::Internal(format!("invalid Stoat attachment MIME type: {err}"))
            })?;

        let response = self
            .http
            .post(format!(
                "{}/attachments",
                autumn_base_url.trim_end_matches('/')
            ))
            .header(STOAT_SESSION_TOKEN_HEADER, token)
            .multipart(multipart::Form::new().part("file", part))
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response
            .json::<StoatAutumnUploadResponse>()
            .await
            .map(|upload| upload.file_id)
            .map_err(Self::network_error)
    }

    async fn fetch_self_with_token(&self, token: &str) -> ClientResult<StoatUser> {
        let response = self
            .request(Method::GET, "/users/@me")
            .header(STOAT_SESSION_TOKEN_HEADER, token)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Log out the current Stoat session.
    pub async fn logout(&self) -> ClientResult<()> {
        if !self.is_authenticated() {
            return Ok(());
        }

        let response = self
            .authenticated_request(Method::POST, "/auth/session/logout")?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }

        self.clear_session()
    }

    fn network_error(error: reqwest::Error) -> ClientError {
        ClientError::Network(error.to_string())
    }

    async fn parse_error(response: reqwest::Response) -> ClientError {
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
mod tests {
    use super::StoatHttpClient;
    use crate::api::StoatRootConfig;
    use crate::config::StoatConfig;
    use reqwest::Method;
    use serde_json::json;

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
