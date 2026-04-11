//! # poly-discord
//!
//! Discord messenger client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] against the Discord REST API v10.
//! Uses user tokens for direct API access.
//!
//! **NOTE:** Discord's ToS prohibits unofficial client automation; this
//! implementation is for research/testing purposes only.
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
use http::DiscordHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;

/// Discord messenger client.
#[cfg(feature = "native")]
pub struct DiscordClient {
    http: DiscordHttpClient,
    /// Cached account metadata (set on successful authenticate).
    account_id: Option<String>,
    account_display_name: Option<String>,
}

#[cfg(feature = "native")]
impl DiscordClient {
    pub fn new() -> Self {
        Self {
            http: DiscordHttpClient::new("https://discord.com".to_string()),
            account_id: None,
            account_display_name: None,
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
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

    fn discord_user_to_poly(&self, u: api::DiscordUser) -> User {
        User {
            id: u.id,
            display_name: u.username,
            avatar_url: None,
            presence: PresenceStatus::Online,
            backend: BackendType::from("discord"),
        }
    }

    fn discord_message_to_poly(&self, m: api::DiscordMessage) -> Message {
        let author = self.discord_user_to_poly(m.author);
        let timestamp = chrono::DateTime::parse_from_rfc3339(&m.timestamp)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        Message {
            id: m.id,
            author,
            content: MessageContent::Text(m.content),
            timestamp,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            edited: m.edited_timestamp.is_some(),
        }
    }
}

#[cfg(feature = "native")]
impl Default for DiscordClient {
    fn default() -> Self { Self::new() }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for DiscordClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let token = match credentials {
            AuthCredentials::Token(t) => t,
            AuthCredentials::EmailPassword { password, .. } => password,
            AuthCredentials::OAuth { token } => token,
            _ => return Err(ClientError::AuthFailed("Discord requires a user token".into())),
        };
        self.http.set_token(token.clone());
        let user = self.http.get_me().await?;
        self.account_id = Some(user.id.clone());
        self.account_display_name = Some(user.username.clone());
        Ok(Session {
            id: user.id.clone(),
            user: User {
                id: user.id.clone(),
                display_name: user.username.clone(),
                avatar_url: None,
                presence: PresenceStatus::Online,
                backend: BackendType::from("discord"),
            },
            token,
            backend: BackendType::from("discord"),
            icon_emoji: Some("💬".to_string()),
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
            http_hosts: vec!["discord.com".to_string(), "cdn.discordapp.com".to_string()],
            description: "Discord chat backend. Connects to discord.com with a user token. \
                          Dev-only: not shipped in release builds because Discord's ToS \
                          forbids third-party clients on the app store."
                .to_string(),
            homepage: Some("https://discord.com".to_string()),
        }
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        Ok(self.http.get_guilds().await?.into_iter().map(|g| Server {
            id: g.id,
            name: g.name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from("discord"),
            unread_count: 0,
            mention_count: 0,
            account_id: account_id.clone(),
            account_display_name: account_name.clone(),
        }).collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let g = self.http.get_guild(id).await?;
        Ok(Server {
            id: g.id,
            name: g.name,
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: BackendType::from("discord"),
            unread_count: 0,
            mention_count: 0,
            account_id,
            account_display_name: account_name,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(self.http.get_guild_channels(server_id).await?.into_iter()
            .filter(|c| c.channel_type == 0 || c.channel_type == 5)
            .map(|c| Channel {
                id: c.id,
                name: c.name,
                channel_type: ChannelType::Text,
                server_id: server_id.to_string(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
            })
            .collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let ch = self.http.get_channel(id).await?;
        Ok(Channel {
            id: ch.id,
            name: ch.name,
            channel_type: ChannelType::Text,
            server_id: ch.guild_id.unwrap_or_default(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
        })
    }

    async fn send_message(&self, channel_id: &str, content: MessageContent) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };
        let m = self.http.send_message(channel_id, &text).await?;
        Ok(self.discord_message_to_poly(m))
    }

    async fn get_messages(&self, channel_id: &str, query: MessageQuery) -> ClientResult<Vec<Message>> {
        let msgs = self.http.get_messages(channel_id, query.limit, query.before.as_deref()).await?;
        Ok(msgs.into_iter().map(|m| self.discord_message_to_poly(m)).collect())
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let u = self.http.get_user(id).await?;
        Ok(self.discord_user_to_poly(u))
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
        Ok(self.http.get_dm_channels().await?.into_iter()
            .filter(|c| c.channel_type == 1)
            .map(|c| DmChannel {
                id: c.id,
                user: User {
                    id: String::new(),
                    display_name: c.name,
                    avatar_url: None,
                    presence: PresenceStatus::Offline,
                    backend: BackendType::from("discord"),
                },
                last_message: None,
                unread_count: 0,
                backend: BackendType::from("discord"),
                account_id: account_id.clone(),
            })
            .collect())
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
        BackendType::from("discord")
    }

    fn backend_name(&self) -> &str {
        "Discord"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::FULL_SOCIAL_CHAT
    }
}
