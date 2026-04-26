#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]
//! # poly-stoat
//!
//! Stoat (formerly Revolt) messenger client for Poly.
//!
//! Implements [`poly_client::ClientBackend`] using the Stoat REST and
//! WebSocket APIs from `developers.stoat.chat`.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.
//!
//! DECISION(D21): WASM Plugin Backends.

// TODO(phase-3.1): Implement Stoat client

#[cfg(feature = "native")]
mod api;

#[cfg(feature = "native")]
mod config;

#[cfg(feature = "native")]
mod http;

#[cfg(feature = "native")]
pub mod signup;

/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

#[cfg(feature = "native")]
pub use api::StoatRootConfig;
#[cfg(feature = "native")]
use api::{
    StoatBanCreate, StoatBulkMessageResponse, StoatChannelEdit, StoatChannelUnread,
    StoatMemberEdit, StoatRelationshipStatus, StoatSendMessageRequest, reply_preview_from_message,
};
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
pub use config::{OFFICIAL_STOAT_BASE_URL, StoatAuthInput, StoatConfig, StoatConfigError};
#[cfg(feature = "native")]
use futures::{
    future,
    stream::{self, Stream},
};
#[cfg(feature = "native")]
use http::StoatHttpClient;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use poly_host_bridge::http::{Method, RequestBuilder};
#[cfg(feature = "native")]
use std::collections::{HashMap, HashSet};
#[cfg(feature = "native")]
use std::pin::Pin;
#[cfg(feature = "native")]
use std::sync::Mutex;
#[cfg(feature = "native")]
use uuid::Uuid;

/// Return the raw FTL translation source for the Stoat client plugin.
///
/// Mirrors the WIT `plugin-metadata.get-translations(locale)` export used by
/// the WASM plugin host.
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// In-memory state for context-menu toggle actions (F10).
/// Persistent storage is F9 — out of scope here.
#[cfg(feature = "native")]
#[derive(Debug, Default)]
struct StoatMenuState {
    muted_channels: HashSet<String>,
    muted_servers: HashSet<String>,
    blocked_users: HashSet<String>,
    friends: HashSet<String>,
    closed_dms: HashSet<String>,
    muted_dms: HashSet<String>,
}

/// Stoat (Revolt) messenger client.
#[cfg(feature = "native")]
pub struct StoatClient {
    http: StoatHttpClient,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    settings_storage: SettingsStorageCell,
    /// F10 — in-memory state for context-menu toggle actions.
    menu_state: Mutex<StoatMenuState>,
}

#[cfg(feature = "native")]
impl StoatClient {
    /// Create a new Stoat client pointed at the official Stoat API.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(StoatConfig::official())
    }

    /// Create a Stoat client for a custom instance.
    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self, StoatConfigError> {
        StoatConfig::new(base_url).map(Self::with_config)
    }

    /// Create a Stoat client from pre-validated configuration.
    #[must_use]
    pub fn with_config(config: StoatConfig) -> Self {
        Self {
            http: StoatHttpClient::new(config),
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(StoatMenuState::default()),
        }
    }

    /// Normalized REST API base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.http.base_url()
    }

    /// Bonfire websocket URL derived from the configured API root.
    #[must_use]
    pub fn websocket_url(&self) -> String {
        self.http.websocket_url()
    }

    /// Stable instance identifier derived from the configured base URL.
    #[must_use]
    pub fn instance_id(&self) -> String {
        self.http.instance_id()
    }

    /// Inspect the currently loaded session token, if any.
    #[must_use]
    pub fn session_token(&self) -> Option<String> {
        self.http.session().map(|session| session.token)
    }

    /// Load a previously persisted Stoat session token into the transport.
    pub fn load_session_token(&self, token: String) -> ClientResult<()> {
        self.http.set_session_token(token)
    }

    /// Build a REST request against the configured Stoat API root.
    pub fn request_builder(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, path)
    }

    /// Build an authenticated request using the currently loaded Stoat token.
    pub fn authenticated_request_builder(
        &self,
        method: Method,
        path: &str,
    ) -> ClientResult<RequestBuilder> {
        self.http.authenticated_request(method, path)
    }

    /// Fetch Stoat instance configuration from `GET /`.
    pub async fn fetch_server_config(&self) -> ClientResult<StoatRootConfig> {
        self.http.fetch_server_config().await
    }

    /// Send a Stoat friend request by `username#discriminator`.
    pub async fn send_friend_request(&self, username: &str) -> ClientResult<User> {
        let (user, root_config) = future::try_join(
            self.http.send_friend_request(username),
            self.http.fetch_server_config(),
        )
        .await?;

        Ok(user.into_poly_user_with_autumn(root_config.autumn_base_url()))
    }

    fn build_session(&self, authenticated: api::StoatAuthenticatedSession) -> Session {
        Session {
            id: authenticated.session_id,
            user: authenticated.user,
            token: authenticated.token,
            backend: BackendType::from("stoat"),
            icon_emoji: Some("🦦".to_string()),
            instance_id: self.instance_id(),
            backend_url: Some(self.base_url().to_string()),
        }
    }

    fn current_account_metadata(&self) -> ClientResult<(String, String)> {
        let session = self.http.session().ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;

        let account_id = session.user_id.ok_or_else(|| {
            ClientError::Internal("Stoat session is missing the authenticated user id".to_string())
        })?;

        let account_display_name = session
            .user_display_name
            .unwrap_or_else(|| account_id.clone());

        Ok((account_id, account_display_name))
    }

    fn index_unreads(unreads: Vec<StoatChannelUnread>) -> HashMap<String, StoatChannelUnread> {
        unreads
            .into_iter()
            .map(|unread| (unread.key.channel.clone(), unread))
            .collect()
    }

    fn unread_count_for_channel(
        unread_index: &HashMap<String, StoatChannelUnread>,
        channel_id: &str,
    ) -> u32 {
        unread_index
            .get(channel_id)
            .map(StoatChannelUnread::approximate_unread_count)
            .unwrap_or(0)
    }

    fn current_user_id(&self) -> Option<String> {
        self.http.session().and_then(|session| session.user_id)
    }

    fn map_messages_response(
        &self,
        response: StoatBulkMessageResponse,
        autumn_base_url: Option<&str>,
    ) -> Vec<Message> {
        let current_user_id = self.current_user_id();
        let (raw_messages, bundled_users, bundled_members) = response.into_parts();

        let mut messages_with_replies: Vec<(Message, Option<String>)> = raw_messages
            .into_iter()
            .map(|raw| {
                let reply_id = raw.primary_reply_id().map(str::to_string);
                let message = raw.into_poly_message(
                    &bundled_users,
                    &bundled_members,
                    current_user_id.as_deref(),
                    autumn_base_url,
                );
                (message, reply_id)
            })
            .collect();

        let preview_index: HashMap<String, MessageReplyPreview> = messages_with_replies
            .iter()
            .map(|(message, _)| (message.id.clone(), reply_preview_from_message(message)))
            .collect();

        let mut messages: Vec<Message> = messages_with_replies
            .drain(..)
            .map(|(mut message, reply_id)| {
                message.reply_to =
                    reply_id.and_then(|reply_id| preview_index.get(&reply_id).cloned());
                message
            })
            .collect();

        messages.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.id.cmp(&right.id))
        });

        messages
    }

    async fn fetch_last_message_preview(
        &self,
        channel_id: &str,
        last_message_id: Option<&str>,
        autumn_base_url: Option<&str>,
    ) -> ClientResult<Option<Message>> {
        if last_message_id.is_none() {
            return Ok(None);
        }

        let response = self
            .http
            .fetch_messages(
                channel_id,
                &MessageQuery {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await?;

        Ok(self
            .map_messages_response(response, autumn_base_url)
            .into_iter()
            .last())
    }

    async fn map_dm_like_channel(
        &self,
        channel: api::StoatChannel,
        unread_count: u32,
        autumn_base_url: Option<&str>,
        account_id: &str,
        self_user: Option<&api::StoatUser>,
    ) -> ClientResult<DmChannel> {
        let last_message = self
            .fetch_last_message_preview(
                &channel.id,
                channel.last_message_id.as_deref(),
                autumn_base_url,
            )
            .await?;

        let user = if channel.is_saved_messages() {
            self_user
                .cloned()
                .ok_or_else(|| {
                    ClientError::Internal(
                        "Stoat Saved Messages mapping requires the current user profile"
                            .to_string(),
                    )
                })?
                .into_poly_user_with_autumn(autumn_base_url)
        } else {
            let current_user_id = self.current_user_id().ok_or_else(|| {
                ClientError::AuthFailed("Stoat client is not authenticated".to_string())
            })?;
            let other_user_id = channel
                .recipients
                .clone()
                .unwrap_or_default()
                .into_iter()
                .find(|user_id| user_id != &current_user_id)
                .ok_or_else(|| {
                    ClientError::NotSupported(format!(
                        "Stoat DM channel {} is missing the other participant",
                        channel.id
                    ))
                })?;

            self.http
                .fetch_user(&other_user_id)
                .await?
                .into_poly_user_with_autumn(autumn_base_url)
        };

        Ok(DmChannel {
            id: channel.id,
            user,
            last_message,
            unread_count,
            backend: BackendType::from("stoat"),
            account_id: account_id.to_string(),
        })
    }

    /// Open or create a Stoat direct-message-like channel for the target user.
    ///
    /// When `user_id` refers to the authenticated user, Stoat returns the
    /// Saved Messages channel. Because Poly's current `DmChannel` model always
    /// carries a `user`, Saved Messages is represented as a self-DM using the
    /// authenticated user's own profile.
    pub async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        let (channel, unreads, root_config, self_user) = future::try_join4(
            self.http.open_direct_message_channel(user_id),
            self.http.fetch_unreads(),
            self.http.fetch_server_config(),
            self.http.fetch_self(),
        )
        .await?;
        let unread_index = Self::index_unreads(unreads);
        let unread_count = Self::unread_count_for_channel(&unread_index, &channel.id);
        let account_id = self.current_account_metadata()?.0;

        self.map_dm_like_channel(
            channel,
            unread_count,
            root_config.autumn_base_url(),
            &account_id,
            Some(&self_user),
        )
        .await
    }

    /// Convenience wrapper for the authenticated user's Saved Messages channel.
    pub async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        let self_user_id = self.current_account_metadata()?.0;
        self.open_direct_message_channel(&self_user_id).await
    }

    async fn send_message_internal(
        &self,
        channel_id: &str,
        content: MessageContent,
        reply_to_message_id: Option<&str>,
    ) -> ClientResult<Message> {
        let root_config = self.http.fetch_server_config().await?;
        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);

        let (text, attachment_ids) = match &content {
            MessageContent::Text(text) => (text.clone(), Vec::new()),
            MessageContent::WithAttachments { text, attachments } => {
                let attachment_ids =
                    if attachments.is_empty() {
                        Vec::new()
                    } else {
                        let autumn_base_url = autumn_base_url.as_deref().ok_or_else(|| {
                            ClientError::NotSupported(
                                "Stoat instance does not expose Autumn for attachment upload"
                                    .to_string(),
                            )
                        })?;

                        future::try_join_all(attachments.iter().map(|attachment| {
                            self.http.upload_attachment(autumn_base_url, attachment)
                        }))
                        .await?
                    };

                (text.clone(), attachment_ids)
            }
        };

        let request = StoatSendMessageRequest::new(
            text,
            attachment_ids,
            reply_to_message_id.map(std::string::ToString::to_string),
            Uuid::new_v4().simple().to_string(),
        );

        let reply_lookup = async {
            if let Some(reply_id) = reply_to_message_id {
                self.http
                    .fetch_message(channel_id, reply_id)
                    .await
                    .map(Some)
            } else {
                Ok(None)
            }
        };

        let (raw_message, reply_message) =
            future::try_join(self.http.send_message(channel_id, &request), reply_lookup).await?;

        let current_user_id = self.current_user_id();
        let bundled_users = HashMap::new();
        let bundled_members = HashMap::new();

        let mut message = raw_message.into_poly_message(
            &bundled_users,
            &bundled_members,
            current_user_id.as_deref(),
            autumn_base_url.as_deref(),
        );

        if let Some(reply_message) = reply_message {
            let reply_preview_source = reply_message.into_poly_message(
                &bundled_users,
                &bundled_members,
                current_user_id.as_deref(),
                autumn_base_url.as_deref(),
            );
            message.reply_to = Some(reply_preview_from_message(&reply_preview_source));
        }

        Ok(message)
    }
}

#[cfg(feature = "native")]
impl Default for StoatClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for StoatClient {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        let authenticated = match StoatAuthInput::try_from(credentials)? {
            StoatAuthInput::SessionToken(token) => self.http.authenticate_with_token(token).await?,
            StoatAuthInput::EmailPassword { email, password } => {
                self.http
                    .login_with_password(&email, &password, Some("Poly"))
                    .await?
            }
        };

        Ok(self.build_session(authenticated))
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.http.logout().await
    }

    fn is_authenticated(&self) -> bool {
        self.http.is_authenticated()
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        let (servers, unreads, root_config) = future::try_join3(
            self.http.fetch_my_servers(),
            self.http.fetch_unreads(),
            self.http.fetch_server_config(),
        )
        .await?;

        let (account_id, account_display_name) = self.current_account_metadata()?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url();

        Ok(servers
            .into_iter()
            .map(|s| {
                let (unread_count, mention_count) = s
                    .channels
                    .iter()
                    .filter_map(|channel_id| unread_index.get(channel_id))
                    .fold((0_u32, 0_u32), |(unreads_acc, mentions_acc), unread| {
                        (
                            unreads_acc.saturating_add(unread.approximate_unread_count()),
                            mentions_acc.saturating_add(unread.mention_count()),
                        )
                    });
                s.into_poly_server(
                    account_id.clone(),
                    account_display_name.clone(),
                    unread_count,
                    mention_count,
                    autumn_base_url,
                )
            })
            .collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let (server, unreads, root_config) = future::try_join3(
            self.http.fetch_server(id),
            self.http.fetch_unreads(),
            self.http.fetch_server_config(),
        )
        .await?;
        let (account_id, account_display_name) = self.current_account_metadata()?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url();

        let (unread_count, mention_count) = server
            .channels
            .iter()
            .filter_map(|channel_id| unread_index.get(channel_id))
            .fold((0_u32, 0_u32), |(unreads_acc, mentions_acc), unread| {
                (
                    unreads_acc.saturating_add(unread.approximate_unread_count()),
                    mentions_acc.saturating_add(unread.mention_count()),
                )
            });

        Ok(server.into_poly_server(
            account_id,
            account_display_name,
            unread_count,
            mention_count,
            autumn_base_url,
        ))
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        let (server, unreads) =
            future::try_join(self.http.fetch_server(server_id), self.http.fetch_unreads()).await?;
        let unread_index = Self::index_unreads(unreads);

        let channels = future::try_join_all(
            server
                .channels
                .iter()
                .map(|channel_id| self.http.fetch_channel(channel_id)),
        )
        .await?;

        channels
            .into_iter()
            .map(|channel| {
                let unread = unread_index.get(&channel.id);
                let unread_count = unread
                    .map(StoatChannelUnread::approximate_unread_count)
                    .unwrap_or(0);
                let mention_count = unread.map(StoatChannelUnread::mention_count).unwrap_or(0);

                channel.into_poly_server_channel(unread_count, mention_count)
            })
            .collect()
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let (channel, unreads) =
            future::try_join(self.http.fetch_channel(id), self.http.fetch_unreads()).await?;
        let unread_index = Self::index_unreads(unreads);
        let unread = unread_index.get(&channel.id);
        let unread_count = unread
            .map(StoatChannelUnread::approximate_unread_count)
            .unwrap_or(0);
        let mention_count = unread.map(StoatChannelUnread::mention_count).unwrap_or(0);

        channel.into_poly_server_channel(unread_count, mention_count)
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        self.send_message_internal(channel_id, content, None).await
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        self.send_message_internal(channel_id, content, Some(reply_to_message_id))
            .await
    }

    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        // Stub: Stoat has a typing-indicator WebSocket event but the HTTP
        // wiring is not yet plumbed through StoatClient.
        // TODO: wire real endpoint in http.rs.
        tracing::warn!("send_typing stub for stoat (channel_id={channel_id})");
        Ok(())
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let (response, root_config) = future::try_join(
            self.http.fetch_messages(channel_id, &query),
            self.http.fetch_server_config(),
        )
        .await?;
        Ok(self.map_messages_response(response, root_config.autumn_base_url()))
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let (user, root_config) =
            future::try_join(self.http.fetch_user(id), self.http.fetch_server_config()).await?;
        Ok(user.into_poly_user_with_autumn(root_config.autumn_base_url()))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        let (self_user, root_config) =
            future::try_join(self.http.fetch_self(), self.http.fetch_server_config()).await?;
        let autumn_base_url = root_config.autumn_base_url();

        let mut friends: Vec<User> = future::try_join_all(
            self_user
                .relations
                .into_iter()
                .filter(|relation| relation.status == StoatRelationshipStatus::Friend)
                .map(|relation| async move { self.http.fetch_user(&relation.user_id).await }),
        )
        .await?
        .into_iter()
        .map(|user| user.into_poly_user_with_autumn(autumn_base_url))
        .collect();

        friends.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.id.cmp(&right.id))
        });

        Ok(friends)
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        let (channel, root_config) = future::try_join(
            self.http.fetch_channel(channel_id),
            self.http.fetch_server_config(),
        )
        .await?;
        let autumn_base_url = root_config.autumn_base_url();

        if let Some(server_id) = channel.server.clone() {
            let members_response = self.http.fetch_server_members(&server_id).await?;
            let user_index: HashMap<String, api::StoatUser> = members_response
                .users
                .into_iter()
                .map(|user| (user.id.clone(), user))
                .collect();

            return Ok(members_response
                .members
                .into_iter()
                .filter(|member| member.key.server == server_id)
                .filter_map(|member| {
                    let mut user = user_index
                        .get(&member.key.user)
                        .cloned()?
                        .into_poly_user_with_autumn(autumn_base_url);

                    if let Some(nickname) = member.nickname {
                        user.display_name = nickname;
                    }
                    if let Some(avatar_url) = member
                        .avatar
                        .and_then(|avatar| avatar.download_url(autumn_base_url))
                    {
                        user.avatar_url = Some(avatar_url);
                    }

                    Some(user)
                })
                .collect());
        }

        if channel.is_group() {
            return Ok(self
                .http
                .fetch_group_members(channel_id)
                .await?
                .into_iter()
                .map(|user| user.into_poly_user_with_autumn(autumn_base_url))
                .collect());
        }

        if channel.is_direct_message() || channel.is_saved_messages() {
            let recipient_ids = channel
                .recipients
                .clone()
                .unwrap_or_else(|| channel.user.into_iter().collect());

            return future::try_join_all(
                recipient_ids
                    .iter()
                    .map(|user_id| self.http.fetch_user(user_id)),
            )
            .await
            .map(|users| {
                users
                    .into_iter()
                    .map(|user| user.into_poly_user_with_autumn(autumn_base_url))
                    .collect()
            });
        }

        Err(ClientError::NotSupported(format!(
            "Stoat channel {channel_id} does not expose member lists"
        )))
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        let (channels, root_config) = future::try_join(
            self.http.fetch_direct_message_channels(),
            self.http.fetch_server_config(),
        )
        .await?;
        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);
        let account_id = self.current_account_metadata()?.0;

        future::try_join_all(
            channels
                .into_iter()
                .filter(|channel| channel.is_group())
                .map(|channel| {
                    let autumn_base_url = autumn_base_url.clone();
                    let account_id = account_id.clone();

                    async move {
                        let members = self.http.fetch_group_members(&channel.id).await?;
                        let last_message = self
                            .fetch_last_message_preview(
                                &channel.id,
                                channel.last_message_id.as_deref(),
                                autumn_base_url.as_deref(),
                            )
                            .await?;

                        Ok(Group {
                            id: channel.id,
                            members: members
                                .into_iter()
                                .map(|user| {
                                    user.into_poly_user_with_autumn(autumn_base_url.as_deref())
                                })
                                .collect(),
                            name: channel.name,
                            last_message,
                            backend: BackendType::from("stoat"),
                            account_id: account_id.clone(),
                        })
                    }
                }),
        )
        .await
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        let ((channels, unreads, root_config), self_user) = future::try_join(
            future::try_join3(
                self.http.fetch_direct_message_channels(),
                self.http.fetch_unreads(),
                self.http.fetch_server_config(),
            ),
            self.http.fetch_self(),
        )
        .await?;
        let unread_index = Self::index_unreads(unreads);
        let autumn_base_url = root_config.autumn_base_url().map(str::to_string);
        let account_id = self.current_account_metadata()?.0;

        future::try_join_all(
            channels
                .into_iter()
                .filter(|channel| channel.is_direct_message() || channel.is_saved_messages())
                .map(|channel| {
                    let unread_index = unread_index.clone();
                    let autumn_base_url = autumn_base_url.clone();
                    let account_id = account_id.clone();
                    let self_user = self_user.clone();

                    async move {
                        let unread_count =
                            Self::unread_count_for_channel(&unread_index, &channel.id);
                        self.map_dm_like_channel(
                            channel,
                            unread_count,
                            autumn_base_url.as_deref(),
                            &account_id,
                            Some(&self_user),
                        )
                        .await
                    }
                }),
        )
        .await
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        let self_user = self.http.fetch_self().await?;
        let account_id = self.current_account_metadata()?.0;

        let mut notifications = future::try_join_all(
            self_user
                .relations
                .into_iter()
                .filter(|relation| relation.status == StoatRelationshipStatus::Incoming)
                .map(|relation| {
                    let account_id = account_id.clone();
                    async move {
                        let user = self.http.fetch_user(&relation.user_id).await?;
                        Ok(Notification {
                            id: format!("stoat-friend-request-{}", user.id),
                            kind: NotificationKind::FriendRequest {
                                from_user_id: user.id.clone(),
                            },
                            backend: BackendType::from("stoat"),
                            account_id: account_id.clone(),
                            timestamp: chrono::Utc::now(),
                            read: false,
                            preview: format!(
                                "{} sent you a friend request",
                                user.display_name.unwrap_or(user.username)
                            ),
                        })
                    }
                }),
        )
        .await?;

        notifications.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
        Ok(notifications)
    }

    async fn respond_to_friend_request(&self, user_id: &str, accept: bool) -> ClientResult<()> {
        if accept {
            let _user = self.http.accept_friend_request(user_id).await?;
        } else {
            let _user = self.http.remove_friend(user_id).await?;
        }

        Ok(())
    }

    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        self.http.remove_group_member(group_id, user_id).await
    }

    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        self.http.add_group_member(group_id, user_id).await
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        StoatClient::open_direct_message_channel(self, user_id).await
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        StoatClient::open_saved_messages_channel(self).await
    }

    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus> {
        let user = self.http.fetch_user(user_id).await?;
        Ok(user.into_poly_user().presence)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }

    // ── Moderation (B-ST) ────────────────────────────────────────────────────

    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        let (server, member_me) = future::try_join(
            self.http.fetch_server(server_id),
            self.http.fetch_my_member(server_id),
        )
        .await?;

        let current_user_id = self.current_account_metadata()?.0;

        // Server owner has all permissions.
        if server.owner == current_user_id {
            return Ok(MemberPermissions {
                manage_server: true,
                manage_channels: true,
                manage_roles: true,
                kick_members: true,
                ban_members: true,
                manage_messages: true,
                timeout_members: true,
                display_role: "Owner".to_string(),
                power_level: None,
            });
        }

        // Compute merged permission bitfield from all assigned roles.
        let roles_map: std::collections::HashMap<String, u64> = server
            .categories // roles live on the server, not categories; use member roles from member_me
            .iter()
            .flat_map(|_| std::iter::empty::<(String, u64)>())
            .collect();
        let _ = roles_map; // placeholder — Stoat roles are on StoatServer but not yet mapped

        // For now we compute from the known bit values directly using a naive
        // approach: if any role grants a bit we set the flag.  Phase B-ST-1 only
        // needs to handle the common case of "no explicit roles" (all false) plus
        // the owner case (all true).  A full role-walking implementation requires
        // the roles to be carried on StoatServer, which is a separate increment.
        // The member_me.roles list tells us role IDs but we don't have role
        // permission bits without a separate GET /servers/{id}/roles call.
        // For now return the safe empty set and mark as non-owner member.
        let has_roles = !member_me.roles.is_empty();
        let _ = has_roles;

        Ok(MemberPermissions {
            manage_server: false,
            manage_channels: false,
            manage_roles: false,
            kick_members: false,
            ban_members: false,
            manage_messages: false,
            timeout_members: false,
            display_role: if member_me.roles.is_empty() {
                "Member".to_string()
            } else {
                "Role Member".to_string()
            },
            power_level: None,
        })
    }

    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        self.http.kick_member(server_id, member_id).await
    }

    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        self.http
            .ban_member(
                server_id,
                member_id,
                &StoatBanCreate {
                    reason: reason.map(str::to_string),
                    delete_message_seconds: delete_message_history_secs,
                },
            )
            .await
    }

    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http.unban_member(server_id, member_id).await
    }

    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        self.http
            .edit_member(
                server_id,
                member_id,
                &StoatMemberEdit {
                    timeout: Some(until.to_rfc3339()),
                    remove: None,
                },
            )
            .await
    }

    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http
            .edit_member(
                server_id,
                member_id,
                &StoatMemberEdit {
                    timeout: None,
                    remove: Some(vec!["Timeout".to_string()]),
                },
            )
            .await
    }

    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let response = self.http.get_bans(server_id).await?;

        // Index users by ID for O(1) lookup.
        let user_index: std::collections::HashMap<String, api::StoatUser> = response
            .users
            .into_iter()
            .map(|user| (user.id.clone(), user))
            .collect();

        Ok(response
            .bans
            .into_iter()
            .map(|ban| {
                let user = user_index.get(&ban.id.user);
                BannedMember {
                    user_id: ban.id.user.clone(),
                    display_name: user
                        .and_then(|u| u.display_name.clone())
                        .unwrap_or_else(|| ban.id.user.clone()),
                    avatar_url: None, // Autumn URL not resolvable without root config here
                    reason: ban.reason,
                    expires_at: None, // Stoat bans are permanent
                    banned_at: None,
                }
            })
            .collect())
    }

    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        self.http.delete_message(channel_id, message_id).await
    }

    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        if update.position.is_some() {
            tracing::warn!(
                "update_channel: Stoat does not support channel position reordering; ignoring position field"
            );
        }

        let edit = StoatChannelEdit {
            name: update.name,
            description: update.topic,
            slowmode: update.slow_mode_secs,
            nsfw: update.nsfw,
        };

        self.http.edit_channel(channel_id, &edit).await
    }

    // reorder_channels: Stoat doesn't expose a reorder endpoint → default NotSupported

    // get_moderation_log: Stoat has no audit log endpoint → default NotSupported

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        // TODO(phase-3): Implement voice participant fetching for Stoat
        Ok(vec![])
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use tokio::sync::mpsc;
            use tokio_tungstenite::tungstenite::Message as WsMessage;

            let ws_url = match self.http.ws_url() {
                Some(url) => url,
                None => return Box::pin(stream::empty()),
            };
            let token = match self.http.session().map(|s| s.token) {
                Some(t) => t,
                None => return Box::pin(stream::empty()),
            };

            let (tx, rx) = mpsc::channel::<ClientEvent>(128);

            tokio::spawn(async move {
                let (mut ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        tracing::warn!("Bonfire WS connect failed: {e}");
                        return;
                    }
                };

                // Authenticate
                let auth_msg = serde_json::json!({"type": "Authenticate", "token": token});
                {
                    use futures::SinkExt;
                    if ws_stream
                        .send(WsMessage::Text(auth_msg.to_string().into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }

                use futures::StreamExt;
                while let Some(msg) = ws_stream.next().await {
                    match msg {
                        Ok(WsMessage::Text(text)) => {
                            if let Ok(event_json) =
                                serde_json::from_str::<serde_json::Value>(&text)
                                && let Some(ev) = parse_bonfire_event(event_json)
                                && tx.send(ev).await.is_err()
                            {
                                break;
                            }
                        }
                        Ok(WsMessage::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
            });

            Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
        }
        #[cfg(target_arch = "wasm32")]
        {
            // WebSocket transport is native-only; WASM builds return an empty stream.
            Box::pin(stream::empty())
        }
    }

    fn backend_type(&self) -> BackendType {
        BackendType::from("stoat")
    }

    fn backend_name(&self) -> &str {
        "Stoat"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            voice: VoiceSupport::None,
            landing: poly_client::LandingPage::DirectMessages,
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: false,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        fn normal(id: &str, label_key: &str, slot: MenuSlot) -> MenuItem {
            MenuItem {
                id: id.to_string(),
                parent_id: None,
                slot,
                label_key: label_key.to_string(),
                icon: None,
                item_variant: MenuItemVariant::Normal,
                shortcut: None,
                block: None,
            }
        }

        fn destructive(id: &str, label_key: &str, slot: MenuSlot) -> MenuItem {
            MenuItem {
                id: id.to_string(),
                parent_id: None,
                slot,
                label_key: label_key.to_string(),
                icon: None,
                item_variant: MenuItemVariant::Destructive,
                shortcut: None,
                block: None,
            }
        }

        let (is_channel_muted, is_server_muted, is_user_blocked, is_friend, is_dm_muted) =
            self.menu_state
                .lock()
                .map(|state| {
                    (
                        state.muted_channels.contains(target_id),
                        state.muted_servers.contains(target_id),
                        state.blocked_users.contains(target_id),
                        state.friends.contains(target_id),
                        state.muted_dms.contains(target_id),
                    )
                })
                .unwrap_or((false, false, false, false, false));

        match target {
            MenuTargetKind::Channel => {
                let mute_item = if is_channel_muted {
                    normal("unmute-channel", "plugin-stoat-menu-unmute-channel-label", MenuSlot::AfterFavorites)
                } else {
                    normal("mute-channel", "plugin-stoat-menu-mute-channel-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    mute_item,
                    normal("mark-channel-read", "plugin-stoat-menu-mark-channel-read-label", MenuSlot::AfterFavorites),
                ])
            }
            MenuTargetKind::Server => {
                let mute_item = if is_server_muted {
                    normal("unmute-server", "plugin-stoat-menu-unmute-server-label", MenuSlot::AfterFavorites)
                } else {
                    normal("mute-server", "plugin-stoat-menu-mute-server-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    normal("invite-people", "plugin-stoat-menu-invite-people-label", MenuSlot::AfterFavorites),
                    normal("privacy-settings", "plugin-stoat-menu-privacy-settings-label", MenuSlot::AfterFavorites),
                    normal("edit-per-server-profile", "plugin-stoat-menu-edit-per-server-profile-label", MenuSlot::AfterFavorites),
                    normal("manage-bots", "plugin-stoat-menu-manage-bots-label", MenuSlot::AfterFavorites),
                    mute_item,
                    destructive("leave-server", "plugin-stoat-menu-leave-server-label", MenuSlot::BeforeLeave),
                ])
            }
            MenuTargetKind::User => {
                let block_item = if is_user_blocked {
                    normal("unblock-user", "plugin-stoat-menu-unblock-user-label", MenuSlot::BeforeLeave)
                } else {
                    destructive("block-user", "plugin-stoat-menu-block-user-label", MenuSlot::BeforeLeave)
                };
                let friend_item = if is_friend {
                    normal("remove-friend", "plugin-stoat-menu-remove-friend-label", MenuSlot::AfterFavorites)
                } else {
                    normal("add-friend", "plugin-stoat-menu-add-friend-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    normal("open-dm", "plugin-stoat-menu-open-dm-label", MenuSlot::AfterFavorites),
                    friend_item,
                    block_item,
                ])
            }
            MenuTargetKind::Message => Ok(vec![
                normal("react-message", "plugin-stoat-menu-react-message-label", MenuSlot::Top),
                normal("copy-message-link", "plugin-stoat-menu-copy-message-link-label", MenuSlot::AfterFavorites),
                destructive("delete-message", "plugin-stoat-menu-delete-message-label", MenuSlot::BeforeLeave),
            ]),
            MenuTargetKind::Dm => {
                let mute_item = if is_dm_muted {
                    normal("unmute-dm", "plugin-stoat-menu-unmute-dm-label", MenuSlot::AfterFavorites)
                } else {
                    normal("mute-dm", "plugin-stoat-menu-mute-dm-label", MenuSlot::AfterFavorites)
                };
                Ok(vec![
                    destructive("close-dm", "plugin-stoat-menu-close-dm-label", MenuSlot::BeforeLeave),
                    mute_item,
                ])
            }
            MenuTargetKind::Category => Ok(vec![]),
        }
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        match action_id {
            "mute-channel" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_channels.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unmute-channel" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_channels.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "mark-channel-read" => Ok(ActionOutcome::Completed),
            "mute-server" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_servers.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unmute-server" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_servers.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "invite-people" | "privacy-settings" | "edit-per-server-profile" | "manage-bots" => {
                Ok(ActionOutcome::Noop)
            }
            "leave-server" => Ok(ActionOutcome::Completed),
            "block-user" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.blocked_users.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unblock-user" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.blocked_users.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "add-friend" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.friends.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "remove-friend" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.friends.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            "open-dm" => Ok(ActionOutcome::Noop),
            "react-message" => Ok(ActionOutcome::Noop),
            "copy-message-link" => Ok(ActionOutcome::Noop),
            "delete-message" => Ok(ActionOutcome::Completed),
            "close-dm" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.closed_dms.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "mute-dm" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_dms.insert(target_id.to_string());
                }
                Ok(ActionOutcome::Completed)
            }
            "unmute-dm" => {
                if let Ok(mut state) = self.menu_state.lock() {
                    state.muted_dms.remove(target_id);
                }
                Ok(ActionOutcome::Completed)
            }
            other => Err(ClientError::NotFound(format!("unknown stoat action: {other}"))),
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
                        key: "avatar-url".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
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

    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-stoat-overview-title".to_string()),
                subtitle_key: Some("plugin-stoat-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }

    async fn get_channel_view(&self, _channel_id: &str) -> ClientResult<ViewDescriptor> {
        Err(ClientError::NotSupported("channel-view not yet implemented".into()))
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        // Empty channel_id is the account-overview sentinel emitted by
        // `AccountOverviewView` (routes.rs line ~149). Map each joined
        // server to a card row with member count + unread indicators.
        if !channel_id.is_empty() {
            return Err(ClientError::NotSupported("view-rows not yet implemented".into()));
        }

        let servers = self.get_servers().await?;

        // Fan out member-count fetches in parallel; degrade gracefully
        // on individual failures so one unauthorized server doesn't
        // blank the entire overview.
        let member_counts: Vec<Option<usize>> = {
            let futs: Vec<_> = servers
                .iter()
                .map(|s| self.http.fetch_server_members(&s.id))
                .collect();
            future::join_all(futs)
                .await
                .into_iter()
                .map(|r| r.ok().map(|resp| resp.members.len()))
                .collect()
        };

        let rows = servers
            .into_iter()
            .zip(member_counts)
            .map(|(s, member_count_opt)| {
                let meta = {
                    let members_str = match member_count_opt {
                        Some(n) => format!("{n} members"),
                        None => "? members".to_string(),
                    };
                    let unread_part = if s.unread_count > 0 {
                        format!(" · {} unread", s.unread_count)
                    } else {
                        String::new()
                    };
                    let mention_part = if s.mention_count > 0 {
                        format!(" · @{}", s.mention_count)
                    } else {
                        String::new()
                    };
                    format!("{members_str}{unread_part}{mention_part}")
                };
                ViewRow {
                    id: s.id.clone(),
                    primary_text: s.name.clone(),
                    secondary_text: s.description.clone(),
                    meta_text: Some(meta),
                    icon: s.icon_url.clone(),
                    badge: None,
                    context_menu_target_kind: MenuTargetKind::Server,
                }
            })
            .collect();

        Ok(ViewRowsPage { rows, next_cursor: None })
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
            id: "emoji-picker".to_string(),
            label_key: "plugin-stoat-composer-emoji-label".to_string(),
            icon: "😀".to_string(),
            position: ComposerSlot::RightOfInput,
        }])
    }

    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![MenuItem {
            id: "report".to_string(),
            parent_id: None,
            slot: MenuSlot::AfterFavorites,
            label_key: "plugin-stoat-message-action-report-label".to_string(),
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
            "emoji-picker" => Ok(ActionOutcome::Noop),
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
            "report" => Ok(ActionOutcome::Noop),
            other => Err(ClientError::NotFound(format!("unknown message action: {other}"))),
        }
    }
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn parse_bonfire_event(json: serde_json::Value) -> Option<ClientEvent> {
    match json.get("type")?.as_str()? {
        "Message" => {
            let channel_id = json.get("channel")?.as_str()?.to_string();
            let msg_json = json.get("message")?;
            let id = msg_json.get("_id")?.as_str()?.to_string();
            let content = msg_json.get("content")?.as_str()?.to_string();
            let author_id = msg_json.get("author")?.as_str()?.to_string();
            let message = poly_client::Message {
                id,
                author: poly_client::User {
                    id: author_id,
                    display_name: String::new(),
                    avatar_url: None,
                    presence: poly_client::PresenceStatus::Online,
                    backend: BackendType::from("stoat"),
                },
                content: poly_client::MessageContent::Text(content),
                timestamp: chrono::Utc::now(),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
                thread: None,
            };
            Some(ClientEvent::MessageReceived { channel_id, message })
        }
        "ChannelStartTyping" => {
            let channel_id = json.get("id")?.as_str()?.to_string();
            let user_id = json.get("user")?.as_str()?.to_string();
            Some(ClientEvent::TypingStarted {
                channel_id,
                user_id,
                timestamp: chrono::Utc::now(),
            })
        }
        _ => None,
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::{OFFICIAL_STOAT_BASE_URL, StoatClient};
    use crate::http::StoatSessionState;
    use axum::{
        Json, Router,
        extract::State,
        http::HeaderMap,
        response::IntoResponse,
        routing::{get, post},
    };
    use poly_client::{BackendType, ClientBackend, PresenceStatus};
    use reqwest::Method;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct TestServerState {
        captured_requests: Arc<Mutex<Vec<serde_json::Value>>>,
        captured_tokens: Arc<Mutex<Vec<String>>>,
        captured_uploads: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    async fn launch_test_server(
        state: TestServerState,
    ) -> Result<(String, tokio::task::JoinHandle<()>), Box<dyn std::error::Error>> {
        async fn send_friend_request() -> impl IntoResponse {
            Json(json!({
                "_id": "user_2",
                "username": "otterpal",
                "discriminator": "0002",
                "display_name": "Otter Pal",
                "online": true
            }))
        }

        async fn upload_attachment(
            State(state): State<TestServerState>,
            headers: HeaderMap,
        ) -> impl IntoResponse {
            if let Some(token) = headers
                .get("x-session-token")
                .and_then(|value| value.to_str().ok())
                .map(std::string::ToString::to_string)
                && let Ok(mut tokens) = state.captured_tokens.lock()
            {
                tokens.push(token);
            }

            if let Ok(mut uploads) = state.captured_uploads.lock() {
                uploads.push(json!({ "ok": true }));
            }

            Json(json!({ "id": "uploaded-file-1" }))
        }

        async fn send_message(
            State(state): State<TestServerState>,
            headers: HeaderMap,
            Json(payload): Json<serde_json::Value>,
        ) -> impl IntoResponse {
            let response_content = payload
                .get("content")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let response_replies = payload.get("replies").cloned();

            if let Ok(mut requests) = state.captured_requests.lock() {
                requests.push(payload);
            }

            if let Some(token) = headers
                .get("x-session-token")
                .and_then(|value| value.to_str().ok())
                .map(std::string::ToString::to_string)
                && let Ok(mut tokens) = state.captured_tokens.lock()
            {
                tokens.push(token);
            }

            Json(json!({
                "_id": "01HZZZZZZZZZZZZZZZZZZZZZZZ",
                "channel": "channel_1",
                "author": "user_1",
                "content": response_content,
                "user": {
                    "_id": "user_1",
                    "username": "stoaty",
                    "discriminator": "0001",
                    "display_name": "Stoaty",
                    "online": true
                },
                "replies": response_replies.map(|replies| {
                    replies
                        .as_array()
                        .map(|entries| {
                            entries
                                .iter()
                                .filter_map(|entry| entry.get("id").cloned())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            }))
        }

        async fn fetch_message() -> impl IntoResponse {
            Json(json!({
                "_id": "01HYYYYYYYYYYYYYYYYYYYYYYY",
                "channel": "channel_1",
                "author": "user_2",
                "content": "Original reply target",
                "user": {
                    "_id": "user_2",
                    "username": "other",
                    "discriminator": "0002",
                    "display_name": "Other User",
                    "online": false
                }
            }))
        }

        let addr_holder = Arc::new(Mutex::new(String::new()));
        let root_addr_holder = addr_holder.clone();

        let app = Router::new()
            .route(
                "/",
                get(move || {
                    let root_addr_holder = root_addr_holder.clone();
                    async move {
                        let addr = root_addr_holder
                            .lock()
                            .ok()
                            .map(|value| value.clone())
                            .unwrap_or_default();
                        Json(json!({
                            "revolt": "0.11.5",
                            "ws": "wss://ws.example.test",
                            "features": {
                                "autumn": {
                                    "enabled": true,
                                    "url": format!("http://{addr}/autumn")
                                }
                            }
                        }))
                    }
                }),
            )
            .route("/users/friend", post(send_friend_request))
            .route("/autumn/attachments", post(upload_attachment))
            .route("/channels/{channel_id}/messages", post(send_message))
            .route(
                "/channels/{channel_id}/messages/{message_id}",
                get(fetch_message),
            )
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        if let Ok(mut stored_addr) = addr_holder.lock() {
            *stored_addr = addr.to_string();
        }
        let handle = tokio::spawn(async move {
            let _ignored = axum::serve(listener, app).await;
        });

        Ok((format!("http://{addr}"), handle))
    }

    #[test]
    fn default_client_uses_official_instance() {
        let client = StoatClient::new();
        assert_eq!(client.base_url(), OFFICIAL_STOAT_BASE_URL);
        assert_eq!(
            client.websocket_url(),
            "wss://api.stoat.chat/ws".to_string()
        );
    }

    #[test]
    fn custom_client_exposes_instance_metadata() {
        let client = StoatClient::with_base_url("http://127.0.0.1:7001/api");
        assert_eq!(
            client.map(|stoat| {
                (
                    stoat.base_url().to_string(),
                    stoat.websocket_url(),
                    stoat.instance_id(),
                )
            }),
            Ok((
                "http://127.0.0.1:7001/api".to_string(),
                "ws://127.0.0.1:7001/api/ws".to_string(),
                "127.0.0.1:7001~api".to_string(),
            ))
        );
    }

    #[test]
    fn request_builder_uses_configured_base_url() {
        let client = StoatClient::with_base_url("https://chat.example.test/api");
        assert_eq!(
            client.map_err(|error| error.to_string()).map(|stoat| {
                stoat
                    .request_builder(Method::GET, "/servers")
                    .url_ref()
                    .to_string()
            }),
            Ok("https://chat.example.test/api/servers".to_string())
        );
    }

    #[test]
    fn server_config_deserializes_through_public_type() {
        let config: Result<super::StoatRootConfig, _> = serde_json::from_value(json!({
            "revolt": "0.11.5",
            "ws": "wss://ws.example.test",
        }));

        assert!(matches!(
            config,
            Ok(super::StoatRootConfig { revolt, ws, .. })
                if revolt == "0.11.5" && ws == "wss://ws.example.test"
        ));
    }

    #[test]
    fn build_session_uses_stoat_backend_identity() {
        let session = StoatClient::with_base_url("https://chat.example.test/api").map(|client| {
            client.build_session(super::api::StoatAuthenticatedSession {
                session_id: "session_1".to_string(),
                user_id: "user_1".to_string(),
                token: "token_1".to_string(),
                user: poly_client::User {
                    id: "user_1".to_string(),
                    display_name: "Stoaty".to_string(),
                    avatar_url: None,
                    presence: PresenceStatus::Online,
                    backend: BackendType::from("stoat"),
                },
                session_name: Some("Poly".to_string()),
            })
        });

        let session = session.expect("authenticate should succeed");
        assert_eq!(session.backend, BackendType::from("stoat"));
        assert_eq!(session.instance_id, "chat.example.test~api");
        assert_eq!(session.backend_url, Some("https://chat.example.test/api".to_string()));
    }

    #[tokio::test]
    async fn send_message_posts_text_payload_and_maps_response()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state.clone()).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let sent = client
            .send_message(
                "channel_1",
                poly_client::MessageContent::Text("Hello Stoat".to_string()),
            )
            .await?;

        server_handle.abort();

        let requests = state
            .captured_requests
            .lock()
            .map_err(|_| "captured request lock poisoned")?;
        let first_request = requests.first().ok_or("missing captured request")?;
        let nonce_present = first_request
            .get("nonce")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|nonce| !nonce.is_empty());

        assert!(nonce_present);
        assert_eq!(first_request.get("content"), Some(&json!("Hello Stoat")));
        assert_eq!(sent.author.display_name, "Stoaty");
        assert_eq!(
            sent.content,
            poly_client::MessageContent::Text("Hello Stoat".to_string())
        );

        let tokens = state
            .captured_tokens
            .lock()
            .map_err(|_| "captured token lock poisoned")?;
        assert_eq!(tokens.first().map(String::as_str), Some("token_123"));

        Ok(())
    }

    #[tokio::test]
    async fn send_reply_message_includes_reply_intent_and_preview()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state.clone()).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let sent = poly_client::ClientBackend::send_reply_message(
            &client,
            "channel_1",
            "01HYYYYYYYYYYYYYYYYYYYYYYY",
            poly_client::MessageContent::Text("Reply text".to_string()),
        )
        .await?;

        server_handle.abort();

        let requests = state
            .captured_requests
            .lock()
            .map_err(|_| "captured request lock poisoned")?;
        let first_request = requests.first().ok_or("missing captured request")?;
        assert_eq!(first_request.get("content"), Some(&json!("Reply text")));
        assert_eq!(
            first_request.get("replies"),
            Some(&json!([{
                "id": "01HYYYYYYYYYYYYYYYYYYYYYYY",
                "mention": false,
                "fail_if_not_exists": true
            }]))
        );

        assert!(matches!(
            sent.reply_to,
            Some(poly_client::MessageReplyPreview { ref message_id, ref author_display_name, ref snippet, .. })
                if message_id == "01HYYYYYYYYYYYYYYYYYYYYYYY"
                    && author_display_name == "Other User"
                    && snippet == "Original reply target"
        ));

        Ok(())
    }

    #[tokio::test]
    async fn send_message_with_attachments_uploads_to_autumn_and_sends_attachment_ids()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state.clone()).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let result = client
            .send_message(
                "channel_1",
                poly_client::MessageContent::WithAttachments {
                    text: "Hello Stoat".to_string(),
                    attachments: vec![poly_client::Attachment {
                        id: "attachment_1".to_string(),
                        filename: "hello.txt".to_string(),
                        content_type: "text/plain".to_string(),
                        url: String::new(),
                        size: 5,
                        upload_bytes: Some(b"hello".to_vec()),
                    }],
                },
            )
            .await;

        server_handle.abort();

        let sent = result?;
        assert_eq!(sent.author.display_name, "Stoaty");

        let uploads = state
            .captured_uploads
            .lock()
            .map_err(|_| "captured upload lock poisoned")?;
        assert_eq!(uploads.len(), 1);

        let requests = state
            .captured_requests
            .lock()
            .map_err(|_| "captured request lock poisoned")?;
        let first_request = requests.first().ok_or("missing captured request")?;
        assert_eq!(first_request.get("content"), Some(&json!("Hello Stoat")));
        assert_eq!(
            first_request.get("attachments"),
            Some(&json!(["uploaded-file-1"]))
        );

        Ok(())
    }

    #[tokio::test]
    async fn send_message_rejects_missing_attachment_upload_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let result = client
            .send_message(
                "channel_1",
                poly_client::MessageContent::WithAttachments {
                    text: "Hello Stoat".to_string(),
                    attachments: vec![poly_client::Attachment {
                        id: "attachment_1".to_string(),
                        filename: "hello.txt".to_string(),
                        content_type: "text/plain".to_string(),
                        url: "https://example.test/hello.txt".to_string(),
                        size: 5,
                        upload_bytes: None,
                    }],
                },
            )
            .await;

        server_handle.abort();

        assert!(matches!(
            result,
            Err(poly_client::ClientError::NotSupported(message))
                if message == "Stoat attachment send requires raw upload bytes"
        ));

        Ok(())
    }

    #[tokio::test]
    async fn send_friend_request_maps_returned_user() -> Result<(), Box<dyn std::error::Error>> {
        let state = TestServerState::default();
        let (base_url, server_handle) = launch_test_server(state).await?;
        let client = StoatClient::with_base_url(base_url)?;

        client.http.set_session(StoatSessionState {
            token: "token_123".to_string(),
            session_id: Some("session_1".to_string()),
            user_id: Some("user_1".to_string()),
            user_display_name: Some("Stoaty".to_string()),
        })?;

        let user = client.send_friend_request("otterpal#0002").await?;

        server_handle.abort();

        assert_eq!(user.id, "user_2");
        assert_eq!(user.display_name, "Otter Pal");
        assert_eq!(user.backend, BackendType::from("stoat"));

        Ok(())
    }
}
