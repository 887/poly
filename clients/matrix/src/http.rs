//! Native HTTP transport scaffolding for the Matrix backend.
//!
//! This file only manages connection/session plumbing. Endpoint-specific API
//! methods are added in later increments so each step remains small and easy to
//! resume after interruptions.

use crate::api::{
    BanRequest, IgnoredUserListContent, InviteRequest, JoinedRoomsResponse, KickRequest,
    LoginIdentifier, LoginRequest, LoginResponse, MessagesResponse, PowerLevelsContent,
    ProfileResponse, PushRuleRequest, RedactRequest, RoomAvatarRequest, RoomEvent,
    RoomMembersResponse, RoomNameRequest, RoomTopicRequest, SendEventResponse, SendMessageRequest,
    SpaceHierarchyResponse, SyncResponse, UnbanRequest, WhoAmIResponse,
};
use crate::config::MatrixConfig;
use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::{HttpClient, Method, RequestBuilder, Response};
use std::sync::{Arc, RwLock};

/// Default User-Agent for Matrix API requests.
pub const DEFAULT_CLIENT_VERSION: &str = "poly-matrix/0.0.0";

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
    /// HTTP thumbnail URL for the authenticated user's avatar (resolved from
    /// the `mxc://` URI in their profile). Populated lazily after the
    /// profile fetch in `MatrixClient::authenticate`. `None` until then,
    /// or if the user has no avatar.
    pub avatar_url: Option<String>,
    /// The `since` token for the next `/sync` request.
    pub sync_next_batch: Option<String>,
}

/// reqwest-backed HTTP transport for one Matrix homeserver.
#[derive(Debug, Clone)]
pub struct MatrixHttpClient {
    config: MatrixConfig,
    http: HttpClient,
    session: Arc<RwLock<Option<MatrixSessionState>>>,
    user_agent: Arc<RwLock<String>>,
}

impl MatrixHttpClient {
    /// Create a new transport for the provided homeserver configuration.
    #[must_use]
    pub fn new(config: MatrixConfig) -> Self {
        Self {
            config,
            http: HttpClient::new(),
            session: Arc::new(RwLock::new(None)),
            user_agent: Arc::new(RwLock::new(DEFAULT_CLIENT_VERSION.to_string())),
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

    /// Update the cached profile (display name + resolved avatar URL) on the
    /// existing session. Called after the post-auth `/profile/{userId}` fetch
    /// so subsequent send-message echoes can populate the author block
    /// without an extra round-trip.
    pub fn update_session_profile(
        &self,
        display_name: Option<String>,
        avatar_url: Option<String>,
    ) -> ClientResult<()> {
        let mut guard = self
            .session
            .write()
            .map_err(|err| ClientError::Internal(format!("session lock poisoned: {err}")))?;
        if let Some(state) = guard.as_mut() {
            state.display_name = display_name;
            state.avatar_url = avatar_url;
        }
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


    /// Update the User-Agent string.
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut guard) = self.user_agent.write() {
            *guard = ua;
        }
    }

    fn ua(&self) -> String {
        self.user_agent
            .read()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_else(|| DEFAULT_CLIENT_VERSION.to_string())
    }

    /// Build an unauthenticated request to a Matrix client-server API path.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, self.config.api_url(path))
            .header("User-Agent", self.ua())
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
            avatar_url: None,
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
            avatar_url: None,
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

    // -----------------------------------------------------------------------
    // Moderation endpoints (B-MX)
    // -----------------------------------------------------------------------

    /// Kick a member via `POST /_matrix/client/v3/rooms/{roomId}/kick`.
    pub async fn kick_member(
        &self,
        room_id: &str,
        user_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/kick"),
            )?
            .json(&KickRequest {
                user_id: user_id.to_string(),
                reason: reason.map(str::to_string),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Ban a member via `POST /_matrix/client/v3/rooms/{roomId}/ban`.
    pub async fn ban_member(
        &self,
        room_id: &str,
        user_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/ban"),
            )?
            .json(&BanRequest {
                user_id: user_id.to_string(),
                reason: reason.map(str::to_string),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Unban a member via `POST /_matrix/client/v3/rooms/{roomId}/unban`.
    pub async fn unban_member(&self, room_id: &str, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::POST,
                &format!("/_matrix/client/v3/rooms/{room_id}/unban"),
            )?
            .json(&UnbanRequest {
                user_id: user_id.to_string(),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Redact an event via `PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}`.
    pub async fn redact_event(
        &self,
        room_id: &str,
        event_id: &str,
        txn_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!(
                    "/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}"
                ),
            )?
            .json(&RedactRequest {
                reason: reason.map(str::to_string),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Fetch the `m.room.power_levels` state event content for a room.
    ///
    /// Returns default power levels if the event is absent (new rooms before
    /// the first explicit power_levels event use Matrix spec defaults).
    pub async fn fetch_power_levels(&self, room_id: &str) -> ClientResult<PowerLevelsContent> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels"),
            )?
            .send()
            .await
            .map_err(Self::network_error)?;

        if response.status().as_u16() == 404 {
            // Room has no explicit power_levels state event — use spec defaults.
            return Ok(PowerLevelsContent::default());
        }

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(Self::network_error)
    }

    /// Fetch banned members via `GET /_matrix/client/v3/rooms/{roomId}/members?membership=ban`.
    pub async fn fetch_banned_members(&self, room_id: &str) -> ClientResult<RoomMembersResponse> {
        let path = format!("/_matrix/client/v3/rooms/{room_id}/members?membership=ban");
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

    /// Update a room's name via `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.name`.
    pub async fn set_room_name(&self, room_id: &str, name: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.name"),
            )?
            .json(&RoomNameRequest {
                name: name.to_string(),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Update a room's topic via `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.topic`.
    pub async fn set_room_topic(&self, room_id: &str, topic: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.topic"),
            )?
            .json(&RoomTopicRequest {
                topic: topic.to_string(),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Ignored users (account data)
    // -----------------------------------------------------------------------

    /// Fetch `m.ignored_user_list` account data.
    ///
    /// Returns a default (empty) struct if the event does not exist yet.
    pub async fn fetch_ignored_user_list(
        &self,
        user_id: &str,
    ) -> ClientResult<IgnoredUserListContent> {
        let path = format!(
            "/_matrix/client/v3/user/{user_id}/account_data/m.ignored_user_list"
        );
        let response = self
            .authenticated_request(Method::GET, &path)?
            .send()
            .await
            .map_err(Self::network_error)?;

        if response.status().as_u16() == 404 {
            return Ok(IgnoredUserListContent::default());
        }
        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        response.json().await.map_err(Self::network_error)
    }

    /// Write `m.ignored_user_list` account data via
    /// `PUT /_matrix/client/v3/user/{userId}/account_data/m.ignored_user_list`.
    pub async fn put_ignored_user_list(
        &self,
        user_id: &str,
        content: &IgnoredUserListContent,
    ) -> ClientResult<()> {
        let path = format!(
            "/_matrix/client/v3/user/{user_id}/account_data/m.ignored_user_list"
        );
        let response = self
            .authenticated_request(Method::PUT, &path)?
            .json(content)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Push rules (mute / unmute conversation)
    // -----------------------------------------------------------------------

    /// Install a room-level push rule that suppresses notifications
    /// (`dont_notify`) via
    /// `PUT /_matrix/client/v3/pushrules/global/room/{roomId}`.
    ///
    /// Matrix push rules have no native expiry; the `until` parameter from
    /// `mute_conversation` is informational only and cannot be honoured.
    pub async fn put_room_push_rule_mute(&self, room_id: &str) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/pushrules/global/room/{room_id}");
        let response = self
            .authenticated_request(Method::PUT, &path)?
            .json(&PushRuleRequest {
                actions: vec![serde_json::Value::String("dont_notify".to_string())],
                conditions: vec![],
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Remove a room-level push rule via
    /// `DELETE /_matrix/client/v3/pushrules/global/room/{roomId}`.
    pub async fn delete_room_push_rule(&self, room_id: &str) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/pushrules/global/room/{room_id}");
        let response = self
            .authenticated_request(Method::DELETE, &path)?
            .send()
            .await
            .map_err(Self::network_error)?;

        // 404 is acceptable — the rule did not exist; treat as success.
        if response.status().as_u16() == 404 {
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Room membership lifecycle
    // -----------------------------------------------------------------------

    /// Leave a room via `POST /_matrix/client/v3/rooms/{roomId}/leave`.
    pub async fn leave_room(&self, room_id: &str) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/rooms/{room_id}/leave");
        let response = self
            .authenticated_request(Method::POST, &path)?
            .json(&serde_json::Value::Object(serde_json::Map::new()))
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Forget a room via `POST /_matrix/client/v3/rooms/{roomId}/forget`.
    ///
    /// The caller must leave the room first; forgetting a joined room will be
    /// rejected by the homeserver with 400.
    pub async fn forget_room(&self, room_id: &str) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/rooms/{room_id}/forget");
        let response = self
            .authenticated_request(Method::POST, &path)?
            .json(&serde_json::Value::Object(serde_json::Map::new()))
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Invite a user to a room via
    /// `POST /_matrix/client/v3/rooms/{roomId}/invite`.
    pub async fn invite_to_room(&self, room_id: &str, user_id: &str) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/rooms/{room_id}/invite");
        let response = self
            .authenticated_request(Method::POST, &path)?
            .json(&InviteRequest {
                user_id: user_id.to_string(),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Room avatar
    // -----------------------------------------------------------------------

    /// Set a room's avatar via
    /// `PUT /_matrix/client/v3/rooms/{roomId}/state/m.room.avatar/`.
    pub async fn set_room_avatar(&self, room_id: &str, mxc_url: &str) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/rooms/{room_id}/state/m.room.avatar/");
        let response = self
            .authenticated_request(Method::PUT, &path)?
            .json(&RoomAvatarRequest {
                url: mxc_url.to_string(),
            })
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // m.direct account data
    // -----------------------------------------------------------------------

    /// Fetch `m.direct` account data as a raw JSON value.
    ///
    /// Returns an empty JSON object if the event does not exist.
    pub async fn fetch_m_direct(&self, user_id: &str) -> ClientResult<serde_json::Value> {
        let path = format!("/_matrix/client/v3/user/{user_id}/account_data/m.direct");
        let response = self
            .authenticated_request(Method::GET, &path)?
            .send()
            .await
            .map_err(Self::network_error)?;

        if response.status().as_u16() == 404 {
            return Ok(serde_json::Value::Object(serde_json::Map::new()));
        }
        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        response.json().await.map_err(Self::network_error)
    }

    /// Write `m.direct` account data via
    /// `PUT /_matrix/client/v3/user/{userId}/account_data/m.direct`.
    pub async fn put_m_direct(
        &self,
        user_id: &str,
        content: &serde_json::Value,
    ) -> ClientResult<()> {
        let path = format!("/_matrix/client/v3/user/{user_id}/account_data/m.direct");
        let response = self
            .authenticated_request(Method::PUT, &path)?
            .json(content)
            .send()
            .await
            .map_err(Self::network_error)?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
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

    /// Get the current sync batch token.
    #[must_use]
    pub fn sync_next_batch(&self) -> Option<String> {
        self.session
            .read()
            .ok()
            .and_then(|s| s.as_ref().and_then(|s| s.sync_next_batch.clone()))
    }

    /// Update the sync batch token after a successful sync.
    pub fn set_sync_next_batch(&self, token: String) {
        if let Ok(mut guard) = self.session.write()
            && let Some(state) = guard.as_mut()
        {
            state.sync_next_batch = Some(token);
        }
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    fn network_error(error: poly_host_bridge::http::HttpError) -> ClientError {
        ClientError::Network(error.to_string())
    }

    async fn parse_error(response: Response) -> ClientError {
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

    #[test]
    fn request_uses_normalized_base_url() {
        let config = MatrixConfig::new("https://matrix.example.test/").unwrap();
        let client = MatrixHttpClient::new(config);
        let builder = client.request(Method::GET, "/_matrix/client/v3/login");
        assert_eq!(
            builder.url_ref(),
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
                avatar_url: None,
                sync_next_batch: None,
            })
            .unwrap();

        let builder = client
            .authenticated_request(Method::GET, "/_matrix/client/v3/sync")
            .unwrap();
        let auth = builder.header_value("authorization").unwrap();
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
                avatar_url: None,
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
                avatar_url: None,
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
