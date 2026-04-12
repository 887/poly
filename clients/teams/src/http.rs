//! Microsoft Graph API HTTP client.

use std::sync::Mutex;

use poly_client::ClientError;
use poly_host_bridge::http::HttpClient;

use crate::api::{GraphChannel, GraphChat, GraphCollection, GraphMessage, GraphTeam, GraphUser};

pub struct TeamsHttpClient {
    base_url: String,
    token: Mutex<Option<String>>,
    http: HttpClient,
}

impl TeamsHttpClient {
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

    pub async fn get_chats(&self) -> Result<Vec<GraphChat>, ClientError> {
        let col: GraphCollection<GraphChat> = self.get("/v1.0/me/chats").await?;
        Ok(col.value)
    }
}
