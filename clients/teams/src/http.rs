//! Microsoft Graph API HTTP client.

use std::sync::{Arc, Mutex};

use poly_client::ClientError;
use poly_host_bridge::http::HttpClient;

use crate::types::{GraphChannel, GraphChat, GraphCollection, GraphMessage, GraphTeam, GraphUser};

#[derive(Clone)]
pub struct TeamsHttpClient {
    base_url: String,
    token: Arc<Mutex<Option<String>>>,
    http: HttpClient,
}

impl TeamsHttpClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            token: Arc::new(Mutex::new(None)),
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
        let resp = self
            .http
            .get(self.url(path))
            .header("Authorization", self.auth_header())
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
            .post(self.url(path))
            .header("Authorization", self.auth_header())
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

    /// Test-server email+password login. Real Graph uses OAuth2 (Phase 3.4.7).
    pub async fn login(&self, login: &str, password: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct LoginResp {
            token: String,
        }
        let resp = self
            .http
            .post(self.url("/test/auth/login"))
            .json(&serde_json::json!({ "login": login, "password": password }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
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
        let url = self.url(&format!(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}"
        ));
        let resp = self
            .http
            .patch(url)
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({ "body": { "content": content, "contentType": "text" } }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::Network(format!("HTTP {status}")));
        }
        resp.json::<GraphMessage>().await.map_err(|e| ClientError::Internal(e.to_string()))
    }

    pub async fn delete_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> Result<(), ClientError> {
        let url = self.url(&format!(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}"
        ));
        let resp = self
            .http
            .delete(url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(ClientError::Network(format!("HTTP {status}")));
        }
        Ok(())
    }

    pub async fn get_chats(&self) -> Result<Vec<GraphChat>, ClientError> {
        let col: GraphCollection<GraphChat> = self.get("/v1.0/me/chats").await?;
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
        let url = self.url(&format!(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/setReaction"
        ));
        let resp = self
            .http
            .post(url)
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({ "reactionType": reaction_type }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        Ok(())
    }

    pub async fn unset_channel_reaction(
        &self,
        team_id: &str,
        channel_id: &str,
        message_id: &str,
        reaction_type: &str,
    ) -> Result<(), ClientError> {
        let url = self.url(&format!(
            "/v1.0/teams/{team_id}/channels/{channel_id}/messages/{message_id}/unsetReaction"
        ));
        let resp = self
            .http
            .post(url)
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({ "reactionType": reaction_type }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        Ok(())
    }

    pub async fn set_presence(&self, availability: &str) -> Result<(), ClientError> {
        let url = self.url("/v1.0/me/presence/setPresence");
        let resp = self
            .http
            .patch(url)
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({ "availability": availability }))
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Network(format!("HTTP {}", resp.status().as_u16())));
        }
        Ok(())
    }

    /// Long-poll the test server's subscription endpoint for change notifications.
    /// Returns the raw JSON events array so the caller can dispatch to `ClientEvent`.
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
