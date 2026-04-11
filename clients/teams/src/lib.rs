//! # poly-teams
//!
//! Microsoft Teams messenger client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using Microsoft Graph API.
//! Uses Bearer token auth against `/v1.0/` endpoints.
//!
//! ## Build Modes
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.

#[cfg(feature = "native")]
mod api;
#[cfg(feature = "native")]
mod http;
#[cfg(feature = "native")]
pub mod signup;

/// WIT bindings for the WASM plugin (WASI targets only).
#[cfg(target_os = "wasi")]
mod wit_bindings;
/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

/// Return Fluent translations for the given locale.
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use http::TeamsHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;

/// Microsoft Teams client.
///
/// Uses Microsoft Graph API v1.0. Teams (guilds) map to poly `Server`s;
/// Graph channels map to poly `Channel`s. Token auth via Bearer header.
///
/// ## Channel ID format
///
/// Graph requires both team_id and channel_id to address messages.
/// We encode these as `"<team_id>/<channel_id>"` in `Channel.server_id` and
/// `Channel.id` respectively, and decode on use.
#[cfg(feature = "native")]
pub struct TeamsClient {
    http: TeamsHttpClient,
    account_id: Option<String>,
    account_display_name: Option<String>,
}

#[cfg(feature = "native")]
impl TeamsClient {
    pub fn new() -> Self {
        Self::with_base_url("https://graph.microsoft.com".to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: TeamsHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
        }
    }

    fn account_id(&self) -> String {
        self.account_id.clone().unwrap_or_default()
    }

    fn account_display_name(&self) -> String {
        self.account_display_name.clone().unwrap_or_default()
    }

    fn graph_message_to_poly(&self, m: api::GraphMessage) -> Message {
        let author = if let Some(from) = m.from {
            if let Some(u) = from.user {
                User {
                    id: u.id,
                    display_name: u.display_name.unwrap_or_default(),
                    avatar_url: None,
                    presence: PresenceStatus::Online,
                    backend: BackendType::from("teams"),
                }
            } else {
                self.unknown_user()
            }
        } else {
            self.unknown_user()
        };
        let timestamp = chrono::DateTime::parse_from_rfc3339(&m.created_date_time)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        Message {
            id: m.id,
            author,
            content: MessageContent::Text(m.body.content),
            timestamp,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            edited: false,
        }
    }

    fn unknown_user(&self) -> User {
        User {
            id: String::new(),
            display_name: "Unknown".to_string(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: BackendType::from("teams"),
        }
    }
}

#[cfg(feature = "native")]
impl Default for TeamsClient {
    fn default() -> Self { Self::new() }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for TeamsClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            AuthCredentials::OAuth { token } => token,
            AuthCredentials::EmailPassword { password, .. } => password,
            _ => return Err(ClientError::AuthFailed("Teams requires a Bearer token".into())),
        };
        self.http.set_token(token.clone());
        let user = self.http.get_me().await?;
        self.account_id = Some(user.id.clone());
        self.account_display_name = Some(user.display_name.clone());
        Ok(Session {
            id: user.id.clone(),
            user: User {
                id: user.id.clone(),
                display_name: user.display_name.clone(),
                avatar_url: None,
                presence: PresenceStatus::Online,
                backend: BackendType::from("teams"),
            },
            token,
            backend: BackendType::from("teams"),
            icon_emoji: Some("💼".to_string()),
            instance_id: self.http.base_url().to_string(),
            backend_url: Some(self.http.base_url().to_string()),
        })
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.account_id = None;
        self.account_display_name = None;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.account_id.is_some()
    }

    fn plugin_manifest(&self) -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![
                "graph.microsoft.com".to_string(),
                "login.microsoftonline.com".to_string(),
            ],
            description: "Microsoft Teams backend. Connects to Microsoft Graph with a \
                          Bearer token. Dev-only: not shipped in release builds because \
                          Teams' enterprise licensing blocks third-party app-store distribution."
                .to_string(),
            homepage: Some("https://teams.microsoft.com".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        Ok(self.http.get_joined_teams().await?.into_iter().map(|t| Server {
            id: t.id,
            name: t.display_name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from("teams"),
            unread_count: 0,
            mention_count: 0,
            account_id: account_id.clone(),
            account_display_name: account_name.clone(),
        }).collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let t = self.http.get_team(id).await?;
        Ok(Server {
            id: t.id,
            name: t.display_name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from("teams"),
            unread_count: 0,
            mention_count: 0,
            account_id,
            account_display_name: account_name,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(self.http.get_team_channels(server_id).await?.into_iter().map(|ch| Channel {
            id: ch.id,
            name: ch.display_name,
            channel_type: ChannelType::Text,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
        }).collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        // id is expected as "team_id/channel_id"
        let (team_id, channel_id) = id.split_once('/').ok_or_else(|| {
            ClientError::Internal(format!("Teams channel id must be 'team_id/channel_id', got '{id}'"))
        })?;
        let channels = self.http.get_team_channels(team_id).await?;
        channels
            .into_iter()
            .find(|c| c.id == channel_id)
            .map(|ch| Channel {
                id: ch.id,
                name: ch.display_name,
                channel_type: ChannelType::Text,
                server_id: team_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
            })
            .ok_or_else(|| ClientError::NotFound(format!("Channel {id}")))
    }

    async fn send_message(&self, channel_id: &str, content: MessageContent) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };
        // channel_id is "team_id/channel_id"
        let (team_id, ch_id) = channel_id.split_once('/').ok_or_else(|| {
            ClientError::Internal(format!("Teams channel id must be 'team_id/channel_id', got '{channel_id}'"))
        })?;
        let m = self.http.send_channel_message(team_id, ch_id, &text).await?;
        Ok(self.graph_message_to_poly(m))
    }

    async fn get_messages(&self, channel_id: &str, query: MessageQuery) -> ClientResult<Vec<Message>> {
        let (team_id, ch_id) = channel_id.split_once('/').ok_or_else(|| {
            ClientError::Internal(format!("Teams channel id must be 'team_id/channel_id', got '{channel_id}'"))
        })?;
        let msgs = self.http.get_channel_messages(team_id, ch_id, query.limit).await?;
        Ok(msgs.into_iter().map(|m| self.graph_message_to_poly(m)).collect())
    }

    async fn get_user(&self, _id: &str) -> ClientResult<User> {
        Err(ClientError::NotFound("Teams user lookup not supported".into()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(vec![])
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let account_id = self.account_id();
        Ok(self.http.get_chats().await?.into_iter().map(|chat| DmChannel {
            id: chat.id,
            user: self.unknown_user(),
            last_message: None,
            unread_count: 0,
            backend: BackendType::from("teams"),
            account_id: account_id.clone(),
        }).collect())
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    async fn get_voice_participants(&self, _channel_id: &str) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(vec![])
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        Box::pin(stream::pending())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from("teams")
    }

    fn backend_name(&self) -> &str {
        "Teams"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            typing_indicators: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }
}
