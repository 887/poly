//! Native HTTP transport scaffolding for the Matrix backend.
//!
//! This file only manages connection/session plumbing. Endpoint-specific API
//! methods are added in later increments so each step remains small and easy to
//! resume after interruptions.

use crate::api::{
    JoinedRoomsResponse, LoginIdentifier, LoginRequest, LoginResponse, MessagesResponse,
    ProfileResponse, PublicRoomsResponse, RoomEvent, RoomMembersResponse, SendEventResponse,
    SendMessageRequest, SpaceHierarchyResponse, SyncResponse, WhoAmIResponse,
};
use crate::config::MatrixConfig;
use poly_client::{ClientError, ClientResult};
use reqwest::{Client, Method, RequestBuilder};
use std::sync::{Arc, RwLock};

/// Matrix session state persisted across requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixSessionState {
    /// Access token returned by the Matrix login API.
    pub access_token: String,
    /// Device ID assigned by the homeserver.
    pub device_id: String,
    /// Fully-qualified Matrix user ID (e.g. `@alice:matrix.org`).
    pub user_id: String,
    /// Display name of the authenticated user when known.
    pub display_name: Option<String>,
    /// The `since` token for the next `/sync` request.
    pub sync_next_batch: Option<String>,
}

/// reqwest-backed HTTP transport for one Matrix homeserver.
#[derive(Debug, Clone)]
pub struct MatrixHttpClient {
    config: MatrixConfig,
    http: Client,
    session: Arc<RwLock<Option<MatrixSessionState>>>,
}

impl MatrixHttpClient {
    /// Create a new transport for the provided homeserver configuration.
    #[must_use]
    pub fn new(config: MatrixConfig) -> Self {
        Self {
            config,
            http: Client::new(),
            session: Arc::new(RwLock::new(None)),
        }
    }

    /// Homeserver base URL.
    #[must_use]
    pub fn homeserver_url(&self) -> &str {
        self.config.homeserver_url()
    }

    /// Stable identifier for multi-account routing.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.config.instance_id()
    }

    /// Whether a session is currently stored.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.session
            .read()
            .is_ok_and(|guard| guard.is_some())
    }

    /// Get a clone of the current session state if any.
    #[must_use]
    pub fn session(&self) -> Option<MatrixSessionState> {
        self.session.read().ok().and_then(|guard| guard.clone())
    }

    /// Store a new session state.
    pub fn set_session(&self, state: MatrixSessionState) -> ClientResult<()> {
        let mut guard = self
            .session
            .write()
            .map_err(|err| ClientError::Internal(format!("session lock poisoned: {err}")))?;
        *guard = Some(state);
        Ok(())
    }

    /// Clear the stored session (logout).
    pub fn clear_session(&self) -> ClientResult<()> {
        let mut guard = self
            .session
            .write()
            .map_err(|err| ClientError::Internal(format!("session lock poisoned: {err}")))?;
        *guard = None;
        Ok(())
    }

    /// Build an unauthenticated request to a Matrix client-server API path.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, self.config.api_url(path))
    }

    /// Build an authenticated request (adds `Authorization: Bearer <token>`).
    pub fn authenticated_request(
        &self,
        method: Method,
        path: &str,
    ) -> ClientResult<RequestBuilder> {
        let session = self
            .session()
            .ok_or_else(|| ClientError::AuthFailed("not logged in".into()))?;

        Ok(self
            .request(method, path)
            .bearer_auth(&session.access_token))
    }

    // -----------------------------------------------------------------------
    // API endpoints
    // -----------------------------------------------------------------------

    /// Authenticate with username/password via `POST /_matrix/client/v3/login`.
    pub async fn login_with_password(
        &self,
        username: &str,
        password: &str,
    ) -> ClientResult<LoginResponse> {
        let response = self
            .request(Method::POST, "/_matrix/client/v3/login")
            .json(&LoginRequest {
                login_type: "m.login.password".to_string(),
                identifier: Some(LoginIdentifier {
                    id_type: "m.id.user".to_string(),
                    user: Some(username.to_string()),
                }),
                password: Some(password.to_string()),
                token: None,
                initial_device_display_name: Some("Poly".to_string()),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        let login: LoginResponse = response.json().await.map_err(Self::network_error)?;

        self.set_session(MatrixSessionState {
            access_token: login.access_token.clone(),
            device_id: login.device_id.clone(),
            user_id: login.user_id.clone(),
            display_name: None,
            sync_next_batch: None,
        })?;

        Ok(login)
    }

    /// Validate an existing access token via `GET /_matrix/client/v3/account/whoami`
    /// and populate the session.
    pub async fn authenticate_with_token(
        &self,
        access_token: String,
    ) -> ClientResult<WhoAmIResponse> {
        let response = self
            .request(Method::GET, "/_matrix/client/v3/account/whoami")
            .bearer_auth(&access_token)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        let whoami: WhoAmIResponse = response.json().await.map_err(Self::network_error)?;

        self.set_session(MatrixSessionState {
            access_token,
            device_id: whoami.device_id.clone().unwrap_or_default(),
            user_id: whoami.user_id.clone(),
            display_name: None,
            sync_next_batch: None,
        })?;

        Ok(whoami)
    }

    /// Fetch a user profile via `GET /_matrix/client/v3/profile/{userId}`.
    pub async fn fetch_profile(&self, user_id: &str) -> ClientResult<ProfileResponse> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/_matrix/client/v3/profile/{user_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch joined rooms via `GET /_matrix/client/v3/joined_rooms`.
    pub async fn fetch_joined_rooms(&self) -> ClientResult<JoinedRoomsResponse> {
        let response = self
            .authenticated_request(Method::GET, "/_matrix/client/v3/joined_rooms")?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch space hierarchy via `GET /_matrix/client/v1/rooms/{roomId}/hierarchy`.
    pub async fn fetch_space_hierarchy(
        &self,
        room_id: &str,
    ) -> ClientResult<SpaceHierarchyResponse> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/_matrix/client/v1/rooms/{room_id}/hierarchy"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Incremental sync via `GET /_matrix/client/v3/sync`.
    pub async fn sync(
        &self,
        since: Option<&str>,
        timeout: Option<u64>,
    ) -> ClientResult<SyncResponse> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(since) = since {
            params.push(("since", since.to_string()));
        }
        if let Some(timeout) = timeout {
            params.push(("timeout", timeout.to_string()));
        }

        let path = build_path("/_matrix/client/v3/sync", &params);
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

    /// Send a message event via
    /// `PUT /_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txnId}`.
    pub async fn send_message(
        &self,
        room_id: &str,
        txn_id: &str,
        content: &SendMessageRequest,
    ) -> ClientResult<SendEventResponse> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}"),
            )?
            .json(content)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch paginated messages via
    /// `GET /_matrix/client/v3/rooms/{roomId}/messages`.
    pub async fn fetch_messages(
        &self,
        room_id: &str,
        from: &str,
        dir: &str,
        limit: Option<u64>,
    ) -> ClientResult<MessagesResponse> {
        let mut params: Vec<(&str, String)> = vec![
            ("from", from.to_string()),
            ("dir", dir.to_string()),
        ];
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }

        let path = build_path(
            &format!("/_matrix/client/v3/rooms/{room_id}/messages"),
            &params,
        );
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

    /// Fetch room members via `GET /_matrix/client/v3/rooms/{roomId}/members`.
    pub async fn fetch_room_members(
        &self,
        room_id: &str,
    ) -> ClientResult<RoomMembersResponse> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/members"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch room state via `GET /_matrix/client/v3/rooms/{roomId}/state`.
    pub async fn fetch_room_state(
        &self,
        room_id: &str,
    ) -> ClientResult<Vec<RoomEvent>> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/state"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch the public room directory via `GET /_matrix/client/v3/publicRooms`.
    pub async fn fetch_public_rooms(
        &self,
        limit: Option<u64>,
        since: Option<&str>,
    ) -> ClientResult<PublicRoomsResponse> {
        let mut params: Vec<(&str, String)> = Vec::new();
        if let Some(limit) = limit {
            params.push(("limit", limit.to_string()));
        }
        if let Some(since) = since {
            params.push(("since", since.to_string()));
        }

        let path = build_path("/_matrix/client/v3/publicRooms", &params);
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

    /// Join a room via `POST /_matrix/client/v3/join/{roomIdOrAlias}`.
    pub async fn join_room(&self, room_id_or_alias: &str) -> ClientResult<serde_json::Value> {
        let response = self
            .authenticated_request(
                Method::POST,
                &format!("/_matrix/client/v3/join/{room_id_or_alias}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Log out the current session via `POST /_matrix/client/v3/logout`.
    pub async fn logout(&self) -> ClientResult<()> {
        if !self.is_authenticated() {
            return Ok(());
        }

        let response = self
            .authenticated_request(Method::POST, "/_matrix/client/v3/logout")?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        self.clear_session()
    }

    /// Fetch account data via
    /// `GET /_matrix/client/v3/user/{userId}/account_data/{type}`.
    pub async fn fetch_account_data(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> ClientResult<serde_json::Value> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/_matrix/client/v3/user/{user_id}/account_data/{data_type}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    fn network_error(error: reqwest::Error) -> ClientError {
        ClientError::Network(error.to_string())
    }

    async fn parse_error(response: reqwest::Response) -> ClientError {
        let status = response.status();
        let payload = response.json::<serde_json::Value>().await.ok();

        let retry_after_ms = payload
            .as_ref()
            .and_then(|v| v.get("retry_after_ms"))
            .and_then(serde_json::Value::as_u64);

        let detail = payload
            .as_ref()
            .and_then(|v| v.get("error"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
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

/// Build a path with optional query parameters appended.
fn build_path(base: &str, params: &[(&str, String)]) -> String {
    if params.is_empty() {
        return base.to_string();
    }
    let qs = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{qs}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::config::MatrixConfig;
    use reqwest::Method;

    #[test]
    fn request_uses_normalized_base_url() {
        let config = MatrixConfig::new("https://matrix.example.test/").unwrap();
        let client = MatrixHttpClient::new(config);
        let url = client
            .request(Method::GET, "/_matrix/client/v3/login")
            .build()
            .unwrap()
            .url()
            .to_string();
        assert_eq!(
            url,
            "https://matrix.example.test/_matrix/client/v3/login"
        );
    }

    #[test]
    fn authenticated_request_injects_bearer_token() {
        let config = MatrixConfig::new("https://matrix.example.test").unwrap();
        let client = MatrixHttpClient::new(config);
        client
            .set_session(MatrixSessionState {
                access_token: "syt_test_token".to_string(),
                device_id: "DEVICE01".to_string(),
                user_id: "@alice:example.test".to_string(),
                display_name: None,
                sync_next_batch: None,
            })
            .unwrap();

        let request = client
            .authenticated_request(Method::GET, "/_matrix/client/v3/sync")
            .unwrap()
            .build()
            .unwrap();

        let auth = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap();
        assert_eq!(auth, "Bearer syt_test_token");
    }

    #[test]
    fn clear_session_resets_authenticated_state() {
        let config = MatrixConfig::new("https://matrix.example.test").unwrap();
        let client = MatrixHttpClient::new(config);
        client
            .set_session(MatrixSessionState {
                access_token: "tok".to_string(),
                device_id: "D".to_string(),
                user_id: "@u:e".to_string(),
                display_name: None,
                sync_next_batch: None,
            })
            .unwrap();
        assert!(client.is_authenticated());

        client.clear_session().unwrap();
        assert!(!client.is_authenticated());
    }

    #[test]
    fn set_session_preserves_state() {
        let config = MatrixConfig::new("https://matrix.example.test").unwrap();
        let client = MatrixHttpClient::new(config);
        client
            .set_session(MatrixSessionState {
                access_token: "my-token".to_string(),
                device_id: "DEV42".to_string(),
                user_id: "@bob:example.test".to_string(),
                display_name: Some("Bob".to_string()),
                sync_next_batch: Some("s123".to_string()),
            })
            .unwrap();

        let session = client.session().unwrap();
        assert_eq!(session.access_token, "my-token");
        assert_eq!(session.device_id, "DEV42");
        assert_eq!(session.user_id, "@bob:example.test");
        assert_eq!(session.display_name, Some("Bob".to_string()));
        assert_eq!(session.sync_next_batch, Some("s123".to_string()));
    }
}
