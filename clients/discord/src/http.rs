//! Discord REST API v10 HTTP client.

use std::sync::Mutex;

use poly_client::ClientError;
use poly_host_bridge::http::HttpClient;

use crate::api::{
    DiscordActiveThreadsResponse, DiscordArchivedThreadsResponse, DiscordChannel, DiscordGuild,
    DiscordMessage, DiscordUser,
};

pub struct DiscordHttpClient {
    base_url: String,
    token: Mutex<Option<String>>,
    http: HttpClient,
}

impl DiscordHttpClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            token: Mutex::new(None),
            http: HttpClient::new(),
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

    fn token_header(&self) -> String {
        self.token
            .lock()
            .ok()
            .and_then(|lock| lock.clone())
            .map(|t| format!("Bot {t}"))
            .unwrap_or_default()
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let resp = self
            .http
            .get(self.api_url(path))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::Network(format!("HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    async fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ClientError> {
        let resp = self
            .http
            .post(self.api_url(path))
            .header("Authorization", self.token_header())
            .json(body)
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::Network(format!("HTTP {status}")));
        }
        resp.json::<T>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    /// Spacebar/Fosscord-compatible password login.
    /// Real Discord doesn't expose this without captcha+MFA; we use it for
    /// self-hosted Spacebar instances and the local test server.
    pub async fn login(&self, login: &str, password: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct LoginResp {
            token: String,
        }
        let resp = self
            .http
            .post(self.api_url("/api/v10/auth/login"))
            .json(&serde_json::json!({ "login": login, "password": password }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::AuthFailed(format!(
                "login failed: HTTP {status}"
            )));
        }
        let parsed: LoginResp = resp
            .json()
            .await
            .map_err(|e| ClientError::Internal(e.to_string()))?;
        Ok(parsed.token)
    }

    pub async fn get_me(&self) -> Result<DiscordUser, ClientError> {
        self.get("/api/v10/users/@me").await
    }

    pub async fn get_guilds(&self) -> Result<Vec<DiscordGuild>, ClientError> {
        self.get("/api/v10/users/@me/guilds").await
    }

    pub async fn get_guild(&self, guild_id: &str) -> Result<DiscordGuild, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}")).await
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<DiscordChannel, ClientError> {
        self.get(&format!("/api/v10/channels/{channel_id}")).await
    }

    pub async fn get_guild_channels(&self, guild_id: &str) -> Result<Vec<DiscordChannel>, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/channels")).await
    }

    pub async fn get_dm_channels(&self) -> Result<Vec<DiscordChannel>, ClientError> {
        self.get("/api/v10/users/@me/channels").await
    }

    pub async fn get_messages(
        &self,
        channel_id: &str,
        limit: Option<u32>,
        before: Option<&str>,
    ) -> Result<Vec<DiscordMessage>, ClientError> {
        let limit = limit.unwrap_or(50);
        let mut path = format!("/api/v10/channels/{channel_id}/messages?limit={limit}");
        if let Some(b) = before {
            path.push_str(&format!("&before={b}"));
        }
        self.get(&path).await
    }

    pub async fn send_message(&self, channel_id: &str, content: &str) -> Result<DiscordMessage, ClientError> {
        self.post_json(
            &format!("/api/v10/channels/{channel_id}/messages"),
            &serde_json::json!({ "content": content }),
        ).await
    }

    pub async fn get_user(&self, user_id: &str) -> Result<DiscordUser, ClientError> {
        self.get(&format!("/api/v10/users/{user_id}")).await
    }

    /// `GET /guilds/{guild_id}/threads/active` — all active (non-archived) threads
    /// in the guild. May return `has_more = true` if there are over 100 threads;
    /// for now we fetch one page (Discord doesn't paginate this endpoint, but
    /// `has_more` signals a cap was applied).
    pub async fn get_active_threads(
        &self,
        guild_id: &str,
    ) -> Result<DiscordActiveThreadsResponse, ClientError> {
        self.get(&format!("/api/v10/guilds/{guild_id}/threads/active")).await
    }

    /// `GET /channels/{channel_id}/threads/archived/public` — archived public threads
    /// for a parent channel (text or forum).
    pub async fn get_archived_threads_public(
        &self,
        channel_id: &str,
        limit: Option<u32>,
    ) -> Result<DiscordArchivedThreadsResponse, ClientError> {
        let limit = limit.unwrap_or(50).min(100);
        self.get(&format!(
            "/api/v10/channels/{channel_id}/threads/archived/public?limit={limit}"
        ))
        .await
    }

    /// POST /api/v10/channels/{channel_id}/typing — trigger typing indicator.
    /// Discord returns 204 No Content on success.
    pub async fn trigger_typing(&self, channel_id: &str) -> Result<(), ClientError> {
        let path = format!("/api/v10/channels/{channel_id}/typing");
        let resp = self
            .http
            .post(self.api_url(&path))
            .header("Authorization", self.token_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        if status == 204 || resp.status().is_success() {
            Ok(())
        } else {
            Err(ClientError::Network(format!("trigger_typing returned HTTP {status}")))
        }
    }

    /// Fetch messages from a thread channel. Uses the same messages endpoint —
    /// Discord thread IDs are valid channel IDs.
    pub async fn get_thread_messages(
        &self,
        thread_id: &str,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> Result<Vec<DiscordMessage>, ClientError> {
        let limit = limit.unwrap_or(1).min(100);
        let mut path = format!("/api/v10/channels/{thread_id}/messages?limit={limit}");
        if let Some(a) = after {
            path.push_str(&format!("&after={a}"));
        }
        self.get(&path).await
    }
}
