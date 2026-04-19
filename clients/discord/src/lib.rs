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
use std::collections::HashSet;
#[cfg(feature = "native")]
use std::pin::Pin;
#[cfg(feature = "native")]
use std::sync::Mutex;

/// F10 — in-memory mutable state for context-menu item state-awareness.
///
/// Tracks mute/block/friend state per id so that `get_context_menu_items`
/// can return Mute vs Unmute, Block vs Unblock, etc. Persistent storage is
/// F9 and is out of scope here; this is intentionally in-memory only.
#[cfg(feature = "native")]
#[derive(Default)]
struct DiscordMenuState {
    /// Channel IDs the user has locally muted.
    muted_channels: HashSet<String>,
    /// Guild (server) IDs the user has locally muted.
    muted_servers: HashSet<String>,
    /// User IDs the local user has blocked.
    blocked_users: HashSet<String>,
    /// User IDs the local user has added as friends.
    friend_ids: HashSet<String>,
    /// DM channel IDs the local user has muted.
    muted_dms: HashSet<String>,
}

/// Discord messenger client.
#[cfg(feature = "native")]
pub struct DiscordClient {
    http: DiscordHttpClient,
    /// Cached account metadata (set on successful authenticate).
    account_id: Option<String>,
    account_display_name: Option<String>,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
    /// F10 — in-memory state for state-aware context-menu items.
    /// Uses Mutex because `get_context_menu_items` takes `&self` but
    /// actions like mute/unmute must mutate state, and `ClientBackend`
    /// requires `Send + Sync`.
    menu_state: Mutex<DiscordMenuState>,
}

#[cfg(feature = "native")]
impl DiscordClient {
    pub fn new() -> Self {
        Self {
            http: DiscordHttpClient::new("https://discord.com".to_string()),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
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
            thread: None,
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
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
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
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
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

    // ── WP 1 / F10 — state-aware context menus ──────────────────────────────

    async fn get_context_menu_items(
        &self, target: MenuTargetKind, target_id: &str,
    ) -> Result<Vec<MenuItem>, ClientError> {
        match target {
            MenuTargetKind::Server => {
                // State-aware: Mute Server / Unmute Server, plus static items.
                let muted = self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_servers.contains(target_id);
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
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
                    mute_item,
                    MenuItem {
                        id: "leave-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-leave-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Channel => {
                // State-aware: Mute/Unmute Channel, Mark Read.
                let muted = self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_channels.contains(target_id);
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-channel".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-channel-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-channel".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-channel-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    mute_item,
                    MenuItem {
                        id: "mark-channel-read".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mark-channel-read-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::User => {
                // State-aware: Block/Unblock, Add Friend/Remove Friend, Open DM.
                let blocked = self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).blocked_users.contains(target_id);
                let is_friend = self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).friend_ids.contains(target_id);
                let block_item = if blocked {
                    MenuItem {
                        id: "unblock-user".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-unblock-user-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "block-user".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-block-user-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    }
                };
                let friend_item = if is_friend {
                    MenuItem {
                        id: "remove-friend".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-remove-friend-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "add-friend".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-add-friend-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    MenuItem {
                        id: "open-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-open-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    friend_item,
                    block_item,
                ])
            }
            MenuTargetKind::Message => {
                // Copy Link is always available; Delete is destructive.
                Ok(vec![
                    MenuItem {
                        id: "copy-message-link".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-copy-message-link-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "delete-message".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-delete-message-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Dm => {
                // State-aware: Mute/Unmute DM, Close DM.
                let muted = self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_dms.contains(target_id);
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    mute_item,
                    MenuItem {
                        id: "close-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-close-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Category => Ok(Vec::new()),
        }
    }

    async fn invoke_context_action(
        &self, action_id: &str, _target: MenuTargetKind, target_id: &str,
    ) -> Result<ActionOutcome, ClientError> {
        match action_id {
            // Server actions
            "invite-people" | "privacy-settings" | "edit-per-server-profile"
            | "server-boost" | "leave-server" => Ok(ActionOutcome::Noop),
            "mute-server" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_servers.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-server" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_servers.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // Channel actions
            "mute-channel" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_channels.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-channel" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_channels.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            "mark-channel-read" => Ok(ActionOutcome::Noop),
            // User actions
            "open-dm" => Ok(ActionOutcome::Noop),
            "add-friend" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).friend_ids.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "remove-friend" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).friend_ids.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            "block-user" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).blocked_users.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unblock-user" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).blocked_users.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // Message actions
            "copy-message-link" | "delete-message" => Ok(ActionOutcome::Noop),
            // DM actions
            "mute-dm" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_dms.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-dm" => {
                self.menu_state.lock().unwrap_or_else(|p| p.into_inner()).muted_dms.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            "close-dm" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown action: {other}"))),
        }
    }

    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound("no pending actions".into()))
    }

    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "profile".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "nickname".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "server-avatar-url".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                ],
                info_block: None,
            },
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "notification-rules".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "mentions-only".to_string(),
                        kind: SettingKind::Toggle,
                        default_value: "false".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "mute-category".to_string(),
                        kind: SettingKind::Toggle,
                        default_value: "false".to_string(),
                        extra: String::new(),
                    },
                ],
                info_block: None,
            },
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "privacy".to_string(),
                icon: None,
                fields: vec![SettingDescriptor {
                    key: "allow-dms-from-server-members".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "true".to_string(),
                    extra: String::new(),
                }],
                info_block: None,
            },
        ])
    }

    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        // Pack C P18: in-memory storage stub. TODO: migrate to
        // host-api.kv_get once exposed to plugins for true persistence.
        if let Some(value) = self.settings_storage.get(scope, scope_id, key) {
            return Ok(value);
        }
        for section in self.get_settings_sections().await? {
            for field in section.fields {
                if field.key == key {
                    return Ok(field.default_value);
                }
            }
        }
        Err(ClientError::NotFound(format!("setting: {key}")))
    }

    async fn set_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()> {
        // Pack C P18: in-memory storage stub. TODO: migrate to
        // host-api.kv_set once exposed to plugins for true persistence.
        self.settings_storage.set(scope, scope_id, key, value)
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
        Ok(vec![ComposerButton {
            id: "stickers".to_string(),
            label_key: "plugin-discord-composer-stickers-label".to_string(),
            icon: "🎨".to_string(),
            position: ComposerSlot::RightOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![MenuItem {
            id: "pin-message".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-discord-message-action-pin-message-label".to_string(),
            icon: None,
            item_variant: MenuItemVariant::Normal,
            shortcut: None,
            block: None,
        }])
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "stickers" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown composer action: {other}"))),
        }
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "pin-message" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}
