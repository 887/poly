//! Native HTTP transport scaffolding for the Stoat backend.
//!
//! This file only manages connection/session plumbing. Endpoint-specific API
//! methods are added in later increments so each step remains small and easy to
//! resume after interruptions.

use crate::api::{
    StoatAllMemberResponse, StoatAuthenticatedSession, StoatAutumnUploadResponse,
    StoatBanCreate, StoatBansResponse, StoatBulkMessageResponse, StoatChannel, StoatChannelEdit,
    StoatChannelUnread, StoatLoginResponse, StoatMemberEdit, StoatMessage, StoatPasswordLoginRequest,
    StoatRootConfig, StoatSendFriendRequest, StoatSendMessageRequest, StoatServer, StoatUser,
};
use crate::config::StoatConfig;
use poly_client::{Attachment, ClientError, ClientResult, MessageQuery};
use poly_host_bridge::http::{HttpClient, HttpError, Method, RequestBuilder, Response};
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
    http: HttpClient,
    session: Arc<RwLock<Option<StoatSessionState>>>,
    /// WebSocket URL obtained from the server's root config (GET /).
    /// Set after successful authentication.
    ws_url: Arc<RwLock<Option<String>>>,
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
    /// Populated after successful authentication. Only consumed by the
    /// native event-stream path in `lib.rs`, so the getter is gated off
    /// on wasm32 to avoid a dead-code warning.
    #[cfg(not(target_arch = "wasm32"))]
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

        let (user, root_config) = tokio::try_join!(
            self.fetch_self_with_token(&login.token),
            self.fetch_server_config(),
        )?;
        self.set_ws_url(root_config.ws.clone());
        let authenticated = StoatAuthenticatedSession {
            session_id: login.session_id,
            user_id: login.user_id,
            token: login.token.clone(),
            user: user.into_poly_user_with_autumn(root_config.autumn_base_url()),
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
        let (user, root_config) = futures::future::try_join(
            self.fetch_self_with_token(&token),
            self.fetch_server_config(),
        )
        .await?;
        self.set_ws_url(root_config.ws.clone());
        let session = StoatAuthenticatedSession {
            // TODO(phase-3.1.2.2): fetch session inventory from Stoat when we
            // need an exact session identifier for token-restore flows.
            session_id: user.id.clone(),
            user_id: user.id.clone(),
            token: token.clone(),
            user: user.into_poly_user_with_autumn(root_config.autumn_base_url()),
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

    /// Fetch the authenticated user's full Stoat profile.
    pub async fn fetch_self(&self) -> ClientResult<StoatUser> {
        let token = self.session().map(|session| session.token).ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;

        self.fetch_self_with_token(&token).await
    }

    /// Fetch a Stoat server by ID.
    /// Fetch all servers the authenticated user belongs to.
    ///
    /// Uses `GET /users/@me/servers` — a non-standard extension supported by
    /// Poly test servers. Falls back to `NotSupported` if the endpoint is
    /// not available.
    pub async fn fetch_my_servers(&self) -> ClientResult<Vec<StoatServer>> {
        let response = self
            .authenticated_request(Method::GET, "/users/@me/servers")?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(ClientError::NotSupported(
                "Server listing endpoint not available".to_string(),
            ));
        }

        response.json().await.map_err(Self::network_error)
    }

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

    /// Fetch the authenticated account's DM and group channels.
    pub async fn fetch_direct_message_channels(&self) -> ClientResult<Vec<StoatChannel>> {
        let response = self
            .authenticated_request(Method::GET, "/users/dms")?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Open or create a direct-message-like channel with the target user.
    ///
    /// Stoat returns a normal one-to-one DM for another user, and returns the
    /// personal Saved Messages channel when the target is the authenticated
    /// user themself.
    pub async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<StoatChannel> {
        let response = self
            .authenticated_request(Method::GET, &format!("/users/{user_id}/dm"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch all members in a Stoat group DM.
    pub async fn fetch_group_members(&self, channel_id: &str) -> ClientResult<Vec<StoatUser>> {
        let response = self
            .authenticated_request(Method::GET, &format!("/channels/{channel_id}/members"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Add a member to a Stoat group DM.
    pub async fn add_group_member(&self, group_id: &str, member_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/channels/{group_id}/recipients/{member_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        Ok(())
    }

    /// Remove a member from a Stoat group DM.
    pub async fn remove_group_member(&self, group_id: &str, member_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/channels/{group_id}/recipients/{member_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        Ok(())
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

    /// Send a Stoat friend request by username/discriminator.
    pub async fn send_friend_request(&self, username: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::POST, "/users/friend")?
            .json(&StoatSendFriendRequest {
                username: username.to_string(),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Accept a pending Stoat friend request.
    pub async fn accept_friend_request(&self, user_id: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::PUT, &format!("/users/{user_id}/friend"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Deny a pending Stoat friend request or remove an existing friend.
    pub async fn remove_friend(&self, user_id: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::DELETE, &format!("/users/{user_id}/friend"))?
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

        let boundary = format!(
            "----polystoatboundary{}",
            uuid::Uuid::new_v4().simple()
        );
        let body = encode_multipart_file(
            &boundary,
            "file",
            &attachment.filename,
            &attachment.content_type,
            &upload_bytes,
        );

        let response = self
            .http
            .post(format!(
                "{}/attachments",
                autumn_base_url.trim_end_matches('/')
            ))
            .header(STOAT_SESSION_TOKEN_HEADER, token)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
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

    // ── Moderation (B-ST) ────────────────────────────────────────────────────

    /// Fetch the calling member's own record for a server.
    ///
    /// Used to compute `MemberPermissions` by merging each assigned role's
    /// permission bits (with the server owner getting all bits set).
    pub async fn fetch_my_member(
        &self,
        server_id: &str,
    ) -> ClientResult<crate::api::StoatServerMemberMe> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/servers/{server_id}/members/@me"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Kick a member from a server (`DELETE /servers/{server_id}/members/{member_id}`).
    pub async fn kick_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/servers/{server_id}/members/{member_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Permanently ban a user from a server (`PUT /servers/{server_id}/bans/{user_id}`).
    pub async fn ban_member(
        &self,
        server_id: &str,
        user_id: &str,
        ban: &StoatBanCreate,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/servers/{server_id}/bans/{user_id}"),
            )?
            .json(ban)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Lift a ban from a user (`DELETE /servers/{server_id}/bans/{user_id}`).
    pub async fn unban_member(&self, server_id: &str, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/servers/{server_id}/bans/{user_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Get the list of banned users for a server (`GET /servers/{server_id}/bans`).
    pub async fn get_bans(&self, server_id: &str) -> ClientResult<StoatBansResponse> {
        let response = self
            .authenticated_request(Method::GET, &format!("/servers/{server_id}/bans"))?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Edit a server member's record (`PATCH /servers/{server_id}/members/{member_id}`).
    ///
    /// Used for both timeout (`{timeout: ISO8601}`) and untimeout (`{remove: ["Timeout"]}`).
    pub async fn edit_member(
        &self,
        server_id: &str,
        member_id: &str,
        edit: &StoatMemberEdit,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PATCH,
                &format!("/servers/{server_id}/members/{member_id}"),
            )?
            .json(edit)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Delete a message from a channel (`DELETE /channels/{channel_id}/messages/{message_id}`).
    pub async fn delete_message(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/channels/{channel_id}/messages/{message_id}"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Update channel settings (`PATCH /channels/{channel_id}`).
    pub async fn edit_channel(
        &self,
        channel_id: &str,
        edit: &StoatChannelEdit,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::PATCH, &format!("/channels/{channel_id}"))?
            .json(edit)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
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

    fn network_error(error: HttpError) -> ClientError {
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
    let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 256);
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
