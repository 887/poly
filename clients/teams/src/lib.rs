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
pub mod auth;
#[cfg(feature = "native")]
mod http;
#[cfg(feature = "native")]
pub mod signup;
#[cfg(feature = "native")]
pub mod types;

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
use futures::stream::Stream;
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

    fn graph_message_to_poly(&self, m: types::GraphMessage) -> Message {
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

    /// Edit a channel message. Not yet on the `ClientBackend` trait — expose
    /// so test harnesses and future trait work can drive it.
    pub async fn edit_message(&self, channel_id: &str, message_id: &str, content: &str) -> ClientResult<Message> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams edit_message requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        let m = self.http.edit_channel_message(team_id, ch_id, message_id, content).await?;
        Ok(self.graph_message_to_poly(m))
    }

    /// Soft-delete a channel message.
    pub async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams delete_message requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http.delete_channel_message(team_id, ch_id, message_id).await
    }

    /// Add a reaction to a channel message.
    pub async fn react(&self, channel_id: &str, message_id: &str, reaction_type: &str) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams react requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http.set_channel_reaction(team_id, ch_id, message_id, reaction_type).await
    }

    /// Remove a reaction from a channel message.
    pub async fn unreact(&self, channel_id: &str, message_id: &str, reaction_type: &str) -> ClientResult<()> {
        let Some((team_id, ch_id)) = channel_id.split_once('/') else {
            return Err(ClientError::Internal(format!(
                "Teams unreact requires 'team_id/channel_id', got '{channel_id}'"
            )));
        };
        self.http.unset_channel_reaction(team_id, ch_id, message_id, reaction_type).await
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

/// Convert a `TeamsEvent` JSON payload (from `/test/events/poll`) to a
/// `ClientEvent`. Returns None for events we don't yet surface.
#[cfg(feature = "native")]
fn teams_event_to_client(ev: serde_json::Value) -> Option<ClientEvent> {
    let ty = ev.get("type")?.as_str()?;
    match ty {
        "MessageCreated" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let m = ev.get("message")?;
            let msg = poly_event_message_from_json(m)?;
            Some(ClientEvent::MessageReceived { channel_id: resource_id, message: msg })
        }
        "MessageUpdated" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let m = ev.get("message")?;
            let msg = poly_event_message_from_json(m)?;
            Some(ClientEvent::MessageEdited { channel_id: resource_id, message: msg })
        }
        "MessageDeleted" => {
            let resource_id = ev.get("resourceId")?.as_str()?.to_string();
            let message_id = ev.get("messageId")?.as_str()?.to_string();
            Some(ClientEvent::MessageDeleted { channel_id: resource_id, message_id })
        }
        _ => None,
    }
}

#[cfg(feature = "native")]
fn poly_event_message_from_json(m: &serde_json::Value) -> Option<Message> {
    let id = m.get("id")?.as_str()?.to_string();
    let content = m.get("body")
        .and_then(|b| b.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let timestamp = m.get("createdDateTime")
        .and_then(|t| t.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    let author_id = m.get("from")
        .and_then(|f| f.get("user"))
        .and_then(|u| u.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let author_name = m.get("from")
        .and_then(|f| f.get("user"))
        .and_then(|u| u.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let edited = m.get("lastModifiedDateTime").map(|v| !v.is_null()).unwrap_or(false);
    Some(Message {
        id,
        author: User {
            id: author_id,
            display_name: author_name,
            avatar_url: None,
            presence: PresenceStatus::Online,
            backend: BackendType::from("teams"),
        },
        content: MessageContent::Text(content),
        timestamp,
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited,
    })
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for TeamsClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            AuthCredentials::OAuth { token } => token,
            AuthCredentials::EmailPassword { email, password } => {
                self.http.login(&email, &password).await?
            }
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
        // Channel IDs are "team_id/channel_id"; chat IDs have no slash.
        let m = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.send_channel_message(team_id, ch_id, &text).await?
        } else {
            self.http.send_chat_message(channel_id, &text).await?
        };
        Ok(self.graph_message_to_poly(m))
    }

    async fn get_messages(&self, channel_id: &str, query: MessageQuery) -> ClientResult<Vec<Message>> {
        let msgs = if let Some((team_id, ch_id)) = channel_id.split_once('/') {
            self.http.get_channel_messages(team_id, ch_id, query.limit).await?
        } else {
            self.http.get_chat_messages(channel_id, query.limit).await?
        };
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

    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()> {
        let availability = match status {
            PresenceStatus::Online => "Available",
            PresenceStatus::Idle => "Away",
            PresenceStatus::DoNotDisturb => "DoNotDisturb",
            PresenceStatus::Offline | PresenceStatus::Invisible => "Offline",
        };
        self.http.set_presence(availability).await
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let http = self.http.clone();
            let (tx, rx) = tokio::sync::mpsc::channel::<ClientEvent>(128);
            tokio::spawn(async move {
                loop {
                    match http.poll_events().await {
                        Ok(events) => {
                            for ev in events {
                                if let Some(ce) = teams_event_to_client(ev)
                                    && tx.send(ce).await.is_err()
                                {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Teams poll_events error: {e:?}");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            });
            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(futures::stream::empty())
        }
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
            landing: poly_client::LandingPage::DirectMessages,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }
}
