//! Microsoft Graph API HTTP client.
//!
//! ## Rate-limiting
//!
//! All outbound requests run through [`send_with_retry`], which honors Graph's
//! `Retry-After` on 429 and applies exponential backoff on 5xx. Up to
//! [`MAX_ATTEMPTS`] tries; on the final attempt we return whatever we got so
//! the caller can surface the error.

use std::sync::{Arc, Mutex};

/// Default User-Agent for Teams (Graph) API requests.
pub const DEFAULT_CLIENT_VERSION: &str = "poly-teams/0.0.0";
use std::time::Duration;

use poly_client::ClientError;
use poly_host_bridge::http::{HttpClient, RequestBuilder, Response};

use crate::types::{GraphChannel, GraphChat, GraphChatMember, GraphCollection, GraphMember, GraphMessage, GraphTeam, GraphUser};

/// Max HTTP attempts per logical request (initial + 2 retries).
const MAX_ATTEMPTS: u32 = 3;
/// Fallback backoff when a 429 response omits `Retry-After`.
const DEFAULT_RETRY_AFTER_SECS: u64 = 1;
/// Hard cap on any single backoff — don't let a server hold us forever.
const MAX_BACKOFF_SECS: u64 = 30;

/// Run `make_req` and retry on 429/5xx. The closure rebuilds the request each
/// attempt because `RequestBuilder` isn't `Clone`.
async fn send_with_retry<F>(make_req: F) -> Result<Response, ClientError>
where
    F: Fn() -> RequestBuilder,
{
    let mut attempt: u32 = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let resp = make_req()
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let retryable = status == 429 || (500..600).contains(&status);
        if !retryable || attempt >= MAX_ATTEMPTS {
            return Ok(resp);
        }
        let delay = if status == 429 {
            resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
        } else {
            1u64 << attempt.saturating_sub(1) // 1, 2, 4…
        };
        let delay = delay.min(MAX_BACKOFF_SECS);
        tracing::debug!(
            status,
            attempt,
            delay_secs = delay,
            "teams: retrying after transient failure"
        );
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}

#[derive(Clone)]
pub struct TeamsHttpClient {
    base_url: String,
    token: Arc<Mutex<Option<String>>>,
    http: HttpClient,
    user_agent: Arc<Mutex<String>>,
}

impl TeamsHttpClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            token: Arc::new(Mutex::new(None)),
            http: HttpClient::new(),
            user_agent: Arc::new(Mutex::new(DEFAULT_CLIENT_VERSION.to_string())),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn set_token(&self, token: String) {
        if let Ok(mut lock) = self.token.lock() {
            *lock = Some(token);
        }
    }


    /// Update the User-Agent string.
    pub fn set_user_agent(&self, ua: String) {
        if let Ok(mut lock) = self.user_agent.lock() {
            *lock = ua;
        }
    }

    fn ua(&self) -> String {
        self.user_agent
            .lock()
            .ok().map_or_else(|| DEFAULT_CLIENT_VERSION.to_string(), |g| g.clone())
    }

    fn auth_header(&self) -> String {
        self.token
            .lock()
            .ok()
            .and_then(|lock| lock.clone())
            .map(|t| format!("Bearer {t}"))
            .unwrap_or_default()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let url = self.url(path);
        let ua = self.ua();
        let resp = send_with_retry(|| {
            self.http
                .get(url.clone())
                .header("Authorization", self.auth_header())
                .header("User-Agent", ua.clone())
        })
        .await?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let url = self.url(path);
        let ua = self.ua();
        let body_bytes =
            serde_json::to_vec(body).map_err(|e| ClientError::Internal(e.to_string()))?;
        let resp = send_with_retry(|| {
            self.http
                .post(url.clone())
                .header("Authorization", self.auth_header())
                .header("Content-Type", "application/json")
                .header("User-Agent", ua.clone())
                .body(body_bytes.clone())
        })
        .await?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    async fn patch_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let url = self.url(path);
        let ua = self.ua();
        let body_bytes =
            serde_json::to_vec(body).map_err(|e| ClientError::Internal(e.to_string()))?;
        let resp = send_with_retry(|| {
            self.http
                .patch(url.clone())
                .header("Authorization", self.auth_header())
                .header("Content-Type", "application/json")
                .header("User-Agent", ua.clone())
                .body(body_bytes.clone())
        })
        .await?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    async fn delete_unit(&self, path: &str) -> Result<(), ClientError> {
        let url = self.url(path);
        let ua = self.ua();
        let resp = send_with_retry(|| {
            self.http
                .delete(url.clone())
                .header("Authorization", self.auth_header())
                .header("User-Agent", ua.clone())
        })
        .await?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        Ok(())
    }

    async fn post_json_unit<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), ClientError> {
        let url = self.url(path);
        let ua = self.ua();
        let body_bytes =
            serde_json::to_vec(body).map_err(|e| ClientError::Internal(e.to_string()))?;
        let resp = send_with_retry(|| {
            self.http
                .post(url.clone())
                .header("Authorization", self.auth_header())
                .header("Content-Type", "application/json")
                .header("User-Agent", ua.clone())
                .body(body_bytes.clone())
        })
        .await?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        Ok(())
    }

    async fn patch_json_unit<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<(), ClientError> {
        let url = self.url(path);
        let ua = self.ua();
        let body_bytes =
            serde_json::to_vec(body).map_err(|e| ClientError::Internal(e.to_string()))?;
        let resp = send_with_retry(|| {
            self.http
                .patch(url.clone())
                .header("Authorization", self.auth_header())
                .header("Content-Type", "application/json")
                .header("User-Agent", ua.clone())
                .body(body_bytes.clone())
        })
        .await?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        Ok(())
    }

    /// Test-server email+password login. Real Graph uses OAuth2 (Phase 3.4.7).
    pub async fn login(&self, login: &str, password: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct LoginResp {
            token: String,
        }
        let url = self.url("/test/auth/login");
        let body = serde_json::json!({ "login": login, "password": password });
        let body_bytes =
            serde_json::to_vec(&body).map_err(|e| ClientError::Internal(e.to_string()))?;
        let resp = send_with_retry(|| {
            self.http
                .post(url.clone())
                .header("Content-Type", "application/json")
                .body(body_bytes.clone())
        })
        .await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::AuthFailed(format!("login failed: HTTP {status}")));
        }
        let parsed: LoginResp = resp
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;
        Ok(parsed.token)
    }

    pub async fn get_me(&self) -> Result<GraphUser, ClientError> {
        self.get("/v1.0/me").await
    }

    pub async fn get_joined_teams(&self) -> Result<Vec<GraphTeam>, ClientError> {
        let col: GraphCollection<GraphTeam> = self.get("/v1.0/me/joinedTeams").await?;
        Ok(col.value)
    }

    pub async fn get_team(&self, team_id: &str) -> Result<GraphTeam, ClientError> {
        self.get(&format!("/v1.0/teams/{team_id}")).await
    }

    pub async fn get_team_channels(&self, team_id: &str) -> Result<Vec<GraphChannel>, ClientError> {
        let col: GraphCollection<GraphChannel> = self.get(&format!("/v1.0/teams/{team_id}/channels")).await?;
        Ok(col.value)
    }

    pub async fn get_channel_messages(
        &self,
        team_id: &str,
        channel_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<GraphMessage>, ClientError> {
        let top = limit.unwrap_or(50);
        let col: GraphCollection<GraphMessage> = self
            .get(&format!("/v1.0/teams/{team_id}/channels/{channel_id}/messages?$top={top}"))
            .await?;
        Ok(col.value)
    }

    pub async fn send_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        content: &str,
    ) -> Result<GraphMessage, ClientError> {
        self.post_json(
            &format!("/v1.0/teams/{team_id}/channels/{channel_id}/messages"),
            &serde_json::json!({ "body": { "content": content, "contentType": "text" } }),
        )
        .await
    }

    pub async fn edit_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        content: &str,
    ) -> Result<GraphMessage, ClientError> {
        self.patch_json(
            &format!("/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}"),
            &serde_json::json!({ "body": { "content": content, "contentType": "text" } }),
        )
        .await
    }

    pub async fn delete_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), ClientError> {
        self.delete_unit(&format!(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}"
        ))
        .await
    }

    pub async fn get_chats(&self) -> Result<Vec<GraphChat>, ClientError> {
        // $expand=members fetches member list (with displayName) inline so
        // get_dm_channels can resolve contact display names without extra round-trips.
        let col: GraphCollection<GraphChat> = self.get("/v1.0/me/chats?$expand=members").await?;
        Ok(col.value)
    }

    pub async fn get_chat_messages(
        &self,
        chat_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<GraphMessage>, ClientError> {
        let top = limit.unwrap_or(50);
        let col: GraphCollection<GraphMessage> = self
            .get(&format!("/v1.0/chats/{chat_id}/messages?$top={top}"))
            .await?;
        Ok(col.value)
    }

    pub async fn send_chat_message(
        &self,
        chat_id: &str,
        content: &str,
    ) -> Result<GraphMessage, ClientError> {
        self.post_json(
            &format!("/v1.0/chats/{chat_id}/messages"),
            &serde_json::json!({ "body": { "content": content, "contentType": "text" } }),
        )
        .await
    }

    pub async fn set_channel_reaction(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        reaction_type: &str,
    ) -> Result<(), ClientError> {
        self.post_json_unit(
            &format!(
                "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/setReaction"
            ),
            &serde_json::json!({ "reactionType": reaction_type }),
        )
        .await
    }

    pub async fn unset_channel_reaction(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        reaction_type: &str,
    ) -> Result<(), ClientError> {
        self.post_json_unit(
            &format!(
                "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/unsetReaction"
            ),
            &serde_json::json!({ "reactionType": reaction_type }),
        )
        .await
    }

    pub async fn set_presence(&self, availability: &str) -> Result<(), ClientError> {
        self.patch_json_unit(
            "/v1.0/me/presence/setPresence",
            &serde_json::json!({ "availability": availability }),
        )
        .await
    }

    // ── Moderation ────────────────────────────────────────────────────────────

    /// GET /v1.0/teams/{team_id}/members — returns team membership list.
    pub async fn get_team_members(&self, team_id: &str) -> Result<Vec<GraphMember>, ClientError> {
        let col: GraphCollection<GraphMember> =
            self.get(&format!("/v1.0/teams/{team_id}/members")).await?;
        Ok(col.value)
    }

    /// DELETE /v1.0/teams/{team_id}/members/{membership_id} — kick a member.
    pub async fn delete_team_member(
        &self,
        team_id: &str,
        membership_id: &str,
    ) -> Result<(), ClientError> {
        self.delete_unit(&format!(
            "/v1.0/teams/{team_id}/members/{membership_id}"
        ))
        .await
    }

    /// POST /v1.0/teams/{team_id}/channels/{channel_id}/messages/{msg_id}/softDelete
    pub async fn soft_delete_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), ClientError> {
        self.post_json_unit(
            &format!(
                "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/softDelete"
            ),
            &serde_json::json!({}),
        )
        .await
    }

    /// PATCH /v1.0/teams/{team_id}/channels/{channel_id} — update channel name/description.
    /// Ignores unsupported fields (slow_mode_secs, nsfw, position).
    pub async fn patch_channel(
        &self,
        team_id: &str,
        channel_id: &str,
        display_name: Option<&str>,
        description: Option<&str>,
    ) -> Result<(), ClientError> {
        let mut body = serde_json::Map::new();
        if let Some(name) = display_name {
            body.insert("displayName".into(), serde_json::Value::String(name.into()));
        }
        if let Some(desc) = description {
            body.insert("description".into(), serde_json::Value::String(desc.into()));
        }
        self.patch_json_unit(
            &format!("/v1.0/teams/{team_id}/channels/{channel_id}"),
            &serde_json::Value::Object(body),
        )
        .await
    }

    // ── DM / Chat management ──────────────────────────────────────────────────

    /// POST /v1.0/chats — create a 1:1 or group chat.
    ///
    /// `chat_type` is `"oneOnOne"` for DMs or `"group"` for group chats.
    /// `members` is a list of OData-typed member objects:
    /// `{ "@odata.type": "#microsoft.graph.aadUserConversationMember", "user@odata.bind": "…", "roles": ["owner"|""] }`.
    pub async fn create_chat(
        &self,
        chat_type: &str,
        members: &[serde_json::Value],
    ) -> Result<GraphChat, ClientError> {
        self.post_json(
            "/v1.0/chats",
            &serde_json::json!({
                "chatType": chat_type,
                "members": members,
            }),
        )
        .await
    }

    /// POST /v1.0/chats/{chat_id}/members — add a member to a group chat.
    pub async fn add_chat_member(
        &self,
        chat_id: &str,
        user_id: &str,
    ) -> Result<(), ClientError> {
        self.post_json_unit(
            &format!("/v1.0/chats/{chat_id}/members"),
            &serde_json::json!({
                "@odata.type": "#microsoft.graph.aadUserConversationMember",
                "user@odata.bind": format!("https://graph.microsoft.com/v1.0/users('{user_id}')"),
                "roles": [],
            }),
        )
        .await
    }

    /// GET /v1.0/chats/{chat_id}/members — list members of a chat.
    pub async fn get_chat_members(&self, chat_id: &str) -> Result<Vec<GraphChatMember>, ClientError> {
        let col: GraphCollection<GraphChatMember> =
            self.get(&format!("/v1.0/chats/{chat_id}/members")).await?;
        Ok(col.value)
    }

    /// DELETE /v1.0/chats/{chat_id}/members/{membership_id} — remove a member from a group chat.
    pub async fn remove_chat_member(
        &self,
        chat_id: &str,
        membership_id: &str,
    ) -> Result<(), ClientError> {
        self.delete_unit(&format!("/v1.0/chats/{chat_id}/members/{membership_id}"))
            .await
    }

    /// PATCH /v1.0/chats/{chat_id} — update topic (display name) of a group chat.
    pub async fn patch_chat_topic(
        &self,
        chat_id: &str,
        topic: &str,
    ) -> Result<(), ClientError> {
        self.patch_json_unit(
            &format!("/v1.0/chats/{chat_id}"),
            &serde_json::json!({ "topic": topic }),
        )
        .await
    }

    /// Long-poll the test server's subscription endpoint for change notifications.
    /// Returns the raw JSON events array so the caller can dispatch to `ClientEvent`.
    ///
    /// Long-polls skip retry on 5xx — the caller loop reconnects on its own schedule.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn poll_events(&self) -> Result<Vec<serde_json::Value>, ClientError> {
        #[derive(serde::Deserialize)]
        struct PollResp {
            events: Vec<serde_json::Value>,
        }
        let resp = self
            .http
            .get(self.url("/test/events/poll"))
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        let parsed: PollResp = resp
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;
        Ok(parsed.events)
    }
}
