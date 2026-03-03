//! HTTP client for poly-server REST API.
//!
//! Wraps `reqwest::Client` with automatic `Authorization: Bearer` header injection
//! and Ed25519 challenge-response authentication.

use ed25519_dalek::{Signer, SigningKey};
use reqwest::Client;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::error::{PolyServerError, Result};
use super::models::*;

/// Configuration for connecting to a poly-server instance.
#[derive(Debug, Clone)]
pub struct PolyServerConfig {
    /// Base URL (e.g. `http://127.0.0.1:7080`).
    pub base_url: String,
    /// Raw 32-byte Ed25519 signing key.
    pub private_key_bytes: [u8; 32],
}

/// Authenticated session state.
#[derive(Debug, Clone)]
pub struct SessionState {
    /// JWT token for API requests.
    pub token: String,
    /// User ID assigned by the server (e.g. `user:abc123`).
    pub user_id: String,
    /// Device ID for this session.
    pub device_id: String,
}

/// HTTP client for a single poly-server instance.
///
/// Handles authentication (Ed25519 challenge-response), token management,
/// and provides typed methods for every API endpoint.
pub struct PolyServerHttpClient {
    /// Base URL of the poly-server instance.
    base_url: String,
    /// Ed25519 signing key for authentication.
    signing_key: SigningKey,
    /// reqwest HTTP client.
    http: Client,
    /// Current session (populated after signup/signin).
    session: Arc<RwLock<Option<SessionState>>>,
}

impl std::fmt::Debug for PolyServerHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolyServerHttpClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl PolyServerHttpClient {
    /// Create a new HTTP client for the given server.
    pub fn new(config: PolyServerConfig) -> Self {
        let signing_key = SigningKey::from_bytes(&config.private_key_bytes);
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            signing_key,
            http: Client::new(),
            session: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the hex-encoded public key.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Get the current session, if authenticated.
    pub async fn session(&self) -> Option<SessionState> {
        self.session.read().await.clone()
    }

    /// Check if the client is currently authenticated.
    pub async fn is_authenticated(&self) -> bool {
        self.session.read().await.is_some()
    }

    /// Get a clone of the session lock for sharing with WS client.
    pub fn session_lock(&self) -> Arc<RwLock<Option<SessionState>>> {
        Arc::clone(&self.session)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Build a request with auth token if available.
    async fn auth_get(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let session = self.session.read().await;
        let Some(ref s) = *session else {
            return Err(PolyServerError::NotAuthenticated);
        };
        Ok(self.http.get(url).bearer_auth(&s.token))
    }

    async fn auth_post(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let session = self.session.read().await;
        let Some(ref s) = *session else {
            return Err(PolyServerError::NotAuthenticated);
        };
        Ok(self.http.post(url).bearer_auth(&s.token))
    }

    async fn auth_patch(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let session = self.session.read().await;
        let Some(ref s) = *session else {
            return Err(PolyServerError::NotAuthenticated);
        };
        Ok(self.http.patch(url).bearer_auth(&s.token))
    }

    async fn auth_delete(&self, url: &str) -> Result<reqwest::RequestBuilder> {
        let session = self.session.read().await;
        let Some(ref s) = *session else {
            return Err(PolyServerError::NotAuthenticated);
        };
        Ok(self.http.delete(url).bearer_auth(&s.token))
    }

    /// Parse a server error response.
    async fn parse_error(resp: reqwest::Response) -> PolyServerError {
        let status = resp.status().as_u16();
        let message = resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
            .unwrap_or_else(|| format!("HTTP {status}"));
        PolyServerError::Server { status, message }
    }

    // ── Auth ─────────────────────────────────────────────────────────────────

    /// `GET /server-info` — probe server (no auth required).
    pub async fn server_info(&self) -> Result<ServerInfo> {
        let resp = self.http.get(self.url("/server-info")).send().await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `POST /auth/signup` — register with Ed25519 public key.
    pub async fn signup(&self, username: &str, display_name: Option<&str>) -> Result<AuthResponse> {
        let pk_hex = self.public_key_hex();
        let body = json!({
            "public_key": pk_hex,
            "username": username,
            "display_name": display_name.unwrap_or(username),
        });
        let resp = self
            .http
            .post(self.url("/auth/signup"))
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let auth: AuthResponse = resp.json().await?;
        info!("Signed up as {} (user_id={})", username, auth.user_id);
        *self.session.write().await = Some(SessionState {
            token: auth.token.clone(),
            user_id: auth.user_id.clone(),
            device_id: auth.device_id.clone(),
        });
        Ok(auth)
    }

    /// `POST /auth/challenge` + `POST /auth/verify` — Ed25519 challenge-response signin.
    pub async fn signin(&self) -> Result<AuthResponse> {
        let pk_hex = self.public_key_hex();

        // Step 1: Request challenge nonce.
        let challenge_resp = self
            .http
            .post(self.url("/auth/challenge"))
            .json(&json!({ "public_key": pk_hex }))
            .send()
            .await?;
        if !challenge_resp.status().is_success() {
            return Err(Self::parse_error(challenge_resp).await);
        }
        let challenge: ChallengeResponse = challenge_resp.json().await?;
        debug!("Got challenge nonce (expires {})", challenge.expires_at);

        // Step 2: Sign the challenge bytes with our Ed25519 key.
        let challenge_bytes = hex::decode(&challenge.challenge)?;
        let signature = self.signing_key.sign(&challenge_bytes);
        let sig_hex = hex::encode(signature.to_bytes());

        // Step 3: Submit signature for verification.
        let verify_resp = self
            .http
            .post(self.url("/auth/verify"))
            .json(&json!({
                "public_key": pk_hex,
                "challenge": challenge.challenge,
                "signature": sig_hex,
            }))
            .send()
            .await?;
        if !verify_resp.status().is_success() {
            return Err(Self::parse_error(verify_resp).await);
        }
        let auth: AuthResponse = verify_resp.json().await?;
        info!("Signed in (user_id={})", auth.user_id);
        *self.session.write().await = Some(SessionState {
            token: auth.token.clone(),
            user_id: auth.user_id.clone(),
            device_id: auth.device_id.clone(),
        });
        Ok(auth)
    }

    /// `POST /auth/signout` — revoke current device session.
    pub async fn signout(&self) -> Result<()> {
        let resp = self
            .auth_post(&self.url("/auth/signout"))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        *self.session.write().await = None;
        info!("Signed out");
        Ok(())
    }

    /// `GET /auth/devices` — list current user's devices.
    pub async fn get_devices(&self) -> Result<Vec<Device>> {
        let resp = self
            .auth_get(&self.url("/auth/devices"))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `DELETE /auth/devices/:id` — revoke a specific device.
    pub async fn revoke_device(&self, device_id: &str) -> Result<()> {
        let resp = self
            .auth_delete(&self.url(&format!("/auth/devices/{device_id}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(())
    }

    // ── Users ────────────────────────────────────────────────────────────────

    /// `GET /users/me` — get current user profile.
    pub async fn get_me(&self) -> Result<UserProfile> {
        let resp = self.auth_get(&self.url("/users/me")).await?.send().await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `PATCH /users/me` — update current user profile.
    pub async fn update_me(
        &self,
        display_name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<UserProfile> {
        let mut map = serde_json::Map::new();
        if let Some(dn) = display_name {
            map.insert("display_name".into(), json!(dn));
        }
        if let Some(url) = avatar_url {
            map.insert("avatar_url".into(), json!(url));
        }
        let body = serde_json::Value::Object(map);
        let resp = self
            .auth_patch(&self.url("/users/me"))
            .await?
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `GET /users/:id` — get a user profile.
    pub async fn get_user(&self, user_id: &str) -> Result<UserProfile> {
        let resp = self
            .auth_get(&self.url(&format!("/users/{user_id}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    // ── Friends ──────────────────────────────────────────────────────────────

    /// `GET /users/me/friends` — list friends.
    pub async fn get_friends(&self) -> Result<Vec<FriendRequest>> {
        let resp = self
            .auth_get(&self.url("/users/me/friends"))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `POST /users/me/friends` — send friend request (by username).
    pub async fn send_friend_request(&self, username: &str) -> Result<FriendRequest> {
        let resp = self
            .auth_post(&self.url("/users/me/friends"))
            .await?
            .json(&json!({ "username": username }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    // ── Servers ──────────────────────────────────────────────────────────────

    /// `GET /servers` — list servers the user is a member of.
    ///
    /// The server returns `[{ "server": { ... } }]` because the SurrealDB
    /// query `SELECT server.* FROM membership FETCH server` nests each server.
    pub async fn get_servers(&self) -> Result<Vec<WireServer>> {
        let resp = self.auth_get(&self.url("/servers")).await?.send().await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let wrappers: Vec<super::models::ServerWrapper> = resp.json().await?;
        Ok(wrappers.into_iter().map(|w| w.server).collect())
    }

    /// `POST /servers` — create a new server.
    pub async fn create_server(&self, name: &str) -> Result<WireServer> {
        let resp = self
            .auth_post(&self.url("/servers"))
            .await?
            .json(&json!({ "name": name }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `GET /servers/:id` — get server detail (members, channels, categories).
    ///
    /// Members come wrapped: `{ "user": { ... } }`. We unwrap them into
    /// flat `UserProfile` values via `ServerDetail::from_value`.
    pub async fn get_server(&self, server_id: &str) -> Result<ServerDetail> {
        let resp = self
            .auth_get(&self.url(&format!("/servers/{server_id}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        let val: serde_json::Value = resp.json().await?;
        ServerDetail::from_value(val).map_err(super::error::PolyServerError::Json)
    }

    /// `POST /servers/:id/invite` — create an invite code.
    ///
    /// Returns a JSON object with a `code` field: `{ "code": "abc123" }`.
    pub async fn create_invite(
        &self,
        server_id: &str,
        max_uses: Option<i64>,
        expires_in_secs: Option<i64>,
    ) -> Result<Value> {
        let mut map = serde_json::Map::new();
        if let Some(mu) = max_uses {
            map.insert("max_uses".into(), json!(mu));
        }
        if let Some(exp) = expires_in_secs {
            map.insert("expires_in_secs".into(), json!(exp));
        }
        let body = serde_json::Value::Object(map);
        let resp = self
            .auth_post(&self.url(&format!("/servers/{server_id}/invite")))
            .await?
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `POST /servers/join/:code` — join a server via invite code.
    pub async fn join_server(&self, invite_code: &str) -> Result<Value> {
        let resp = self
            .auth_post(&self.url(&format!("/servers/join/{invite_code}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `DELETE /servers/:id/members/me` — leave a server.
    pub async fn leave_server(&self, server_id: &str) -> Result<()> {
        let resp = self
            .auth_delete(&self.url(&format!("/servers/{server_id}/members/me")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(())
    }

    // ── Channels ─────────────────────────────────────────────────────────────

    /// `GET /servers/:id/channels` — list channels in a server.
    pub async fn get_channels(&self, server_id: &str) -> Result<Vec<WireChannel>> {
        let resp = self
            .auth_get(&self.url(&format!("/servers/{server_id}/channels")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `POST /servers/:id/channels` — create a channel.
    pub async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        kind: &str,
        category_id: Option<&str>,
    ) -> Result<WireChannel> {
        let mut map = serde_json::Map::new();
        map.insert("name".into(), json!(name));
        map.insert("kind".into(), json!(kind));
        if let Some(cat) = category_id {
            map.insert("category".into(), json!(cat));
        }
        let body = serde_json::Value::Object(map);
        let resp = self
            .auth_post(&self.url(&format!("/servers/{server_id}/channels")))
            .await?
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `GET /channels/@dms` — list DM channels.
    pub async fn get_dm_channels(&self) -> Result<Vec<WireChannel>> {
        let resp = self
            .auth_get(&self.url("/channels/@dms"))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `POST /channels/@dms` — create/open a DM with a user.
    pub async fn create_dm(&self, user_id: &str) -> Result<WireChannel> {
        let resp = self
            .auth_post(&self.url("/channels/@dms"))
            .await?
            .json(&json!({ "user_id": user_id }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `GET /channels/:id/participants` — list channel participants.
    pub async fn get_participants(&self, channel_id: &str) -> Result<Vec<Participant>> {
        let resp = self
            .auth_get(&self.url(&format!("/channels/{channel_id}/participants")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    /// `GET /channels/:id/messages` — list messages with cursor-based pagination.
    pub async fn get_messages(
        &self,
        channel_id: &str,
        limit: Option<u32>,
        before: Option<&str>,
    ) -> Result<Vec<WireMessage>> {
        let mut url = self.url(&format!("/channels/{channel_id}/messages"));
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={l}"));
        }
        if let Some(b) = before {
            params.push(format!("before={b}"));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }
        let resp = self.auth_get(&url).await?.send().await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `POST /channels/:id/messages` — send a message.
    pub async fn send_message(
        &self,
        channel_id: &str,
        content: &str,
        reply_to: Option<&str>,
        attachments: Option<&[String]>,
    ) -> Result<WireMessage> {
        let mut map = serde_json::Map::new();
        map.insert("content".into(), json!(content));
        if let Some(rt) = reply_to {
            map.insert("reply_to".into(), json!(rt));
        }
        if let Some(att) = attachments {
            map.insert("attachments".into(), json!(att));
        }
        let body = serde_json::Value::Object(map);
        let resp = self
            .auth_post(&self.url(&format!("/channels/{channel_id}/messages")))
            .await?
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `PATCH /messages/:id` — edit a message.
    pub async fn edit_message(&self, message_id: &str, content: &str) -> Result<WireMessage> {
        let resp = self
            .auth_patch(&self.url(&format!("/messages/{message_id}")))
            .await?
            .json(&json!({ "content": content }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    /// `DELETE /messages/:id` — soft-delete a message.
    pub async fn delete_message(&self, message_id: &str) -> Result<()> {
        let resp = self
            .auth_delete(&self.url(&format!("/messages/{message_id}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(())
    }

    // ── Reactions ────────────────────────────────────────────────────────────

    /// `POST /messages/:id/reactions/:emoji` — add a reaction.
    pub async fn add_reaction(&self, message_id: &str, emoji: &str) -> Result<()> {
        let resp = self
            .auth_post(&self.url(&format!("/messages/{message_id}/reactions/{emoji}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(())
    }

    /// `DELETE /messages/:id/reactions/:emoji` — remove a reaction.
    pub async fn remove_reaction(&self, message_id: &str, emoji: &str) -> Result<()> {
        let resp = self
            .auth_delete(&self.url(&format!("/messages/{message_id}/reactions/{emoji}")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(())
    }

    /// `GET /messages/:id/reactions` — list reactions on a message.
    pub async fn get_reactions(&self, message_id: &str) -> Result<Vec<WireReaction>> {
        let resp = self
            .auth_get(&self.url(&format!("/messages/{message_id}/reactions")))
            .await?
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Self::parse_error(resp).await);
        }
        Ok(resp.json().await?)
    }

    // ── Attachments ──────────────────────────────────────────────────────────

    /// `GET /attachments/:id` — get attachment download URL.
    ///
    /// Returns the full URL (not the file bytes). Use reqwest to download.
    pub fn attachment_url(&self, attachment_id: &str) -> String {
        self.url(&format!("/attachments/{attachment_id}"))
    }
}
