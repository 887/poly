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
            id: u.id.to_string(),
            display_name: u.global_name.unwrap_or(u.username),
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
            id: m.id.to_string(),
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
            AuthCredentials::EmailPassword { email, password } => {
                self.http.login(&email, &password).await?
            }
            AuthCredentials::OAuth { token } => token,
            _ => return Err(ClientError::AuthFailed("Discord requires a user token or email+password".into())),
        };
        self.http.set_token(token.clone());
        let user = self.http.get_me().await?;
        let user_id = user.id.to_string();
        self.account_id = Some(user_id.clone());
        self.account_display_name = Some(user.username.clone());
        Ok(Session {
            id: user_id.clone(),
            user: User {
                id: user_id,
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
            id: g.id.to_string(),
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
            id: g.id.to_string(),
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
        use twilight_model::channel::ChannelType as DcChType;
        Ok(self.http.get_guild_channels(server_id).await?.into_iter()
            .filter(|c| matches!(c.channel_type, DcChType::GuildText | DcChType::GuildAnnouncement))
            .map(|c| Channel {
                id: c.id.to_string(),
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
            id: ch.id.to_string(),
            name: ch.name,
            channel_type: ChannelType::Text,
            server_id: ch.guild_id.map(|id| id.to_string()).unwrap_or_default(),
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
        use twilight_model::channel::ChannelType as DcChType;
        let account_id = self.account_id();
        Ok(self.http.get_dm_channels().await?.into_iter()
            .filter(|c| c.channel_type == DcChType::Private)
            .map(|c| DmChannel {
                id: c.id.to_string(),
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

    // ── WP 1 / plan-client-ui-surface stubs ─────────────────────────────────

    async fn get_context_menu_items(
        &self, target: MenuTargetKind, _target_id: &str,
    ) -> Result<Vec<MenuItem>, ClientError> {
        match target {
            MenuTargetKind::Server => Ok(vec![
                MenuItem {
                    id: "invite-people".to_string(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-discord-menu-invite-people-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "privacy-settings".to_string(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-discord-menu-privacy-settings-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "edit-per-server-profile".to_string(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-discord-menu-edit-per-server-profile-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "server-boost".to_string(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-discord-menu-server-boost-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
            ]),
            _ => Ok(Vec::new()),
        }
    }

    async fn invoke_context_action(
        &self, action_id: &str, _target: MenuTargetKind, _target_id: &str,
    ) -> Result<ActionOutcome, ClientError> {
        match action_id {
            "invite-people" | "privacy-settings" | "edit-per-server-profile"
            | "server-boost" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown action: {other}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(Vec::new())
    }

    async fn get_setting_value(
        &self,
        _scope: SettingsScope,
        _scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        Err(ClientError::NotFound(format!("setting: {key}")))
    }

    async fn set_setting_value(
        &self,
        _scope: SettingsScope,
        _scope_id: &str,
        _key: &str,
        _value: &str,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("settings not yet implemented".into()))
    }

    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: Vec::new(),
            header_block: None,
        })
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown sidebar action: {action_id}")))
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        _channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        Err(ClientError::NotSupported("view-rows not yet implemented".into()))
    }

    async fn get_view_detail(
        &self,
        _channel_id: &str,
        _row_id: &str,
    ) -> ClientResult<ViewDetail> {
        Err(ClientError::NotSupported("view-detail not yet implemented".into()))
    }

    async fn get_composer_buttons(&self, _channel_id: &str) -> ClientResult<Vec<ComposerButton>> {
        Ok(Vec::new())
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(Vec::new())
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown composer action: {action_id}")))
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(format!("unknown message action: {action_id}")))
    }
}
