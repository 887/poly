//! # poly-demo
//!
//! Demo/mock messenger client for Poly UI testing.
//!
//! Generates fake servers, channels, users, messages, and events
//! so the full UI can be developed and tested without connecting
//! to any real messenger backend.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements [`poly_client::ClientBackend`]
//!   for direct linking into `poly-core`. This is the traditional path.
//! - **WASM plugin** (`--no-default-features`, target `wasm32-wasip2`): Exports
//!   the WIT `messenger-client` interface via `wit-bindgen`. Loaded at runtime
//!   by the plugin host in `poly-core`.
//!
//! DECISION(D21): WASM Plugin Backends.

/// Public data module — demo data generators for testing.
pub mod data;

/// WASM plugin guest implementation.
///
/// When compiled to `wasm32-wasip2`, this module exports the WIT
/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// `messenger-client` interface using `wit-bindgen`.
/// Only on WASI targets (not `wasm32-unknown-unknown` used by the web frontend).
#[cfg(target_os = "wasi")]
mod guest;

// ─── Native plugin metadata ─────────────────────────────────────────
//
// Mirrors the WIT `plugin-metadata.get-translations` interface for native
// (non-WASM) builds. The plugin-host calls `get-translations(locale)` on
// WASM components at instantiation time; for native backends poly-core calls
// this free function instead. The FTL paths are owned by this crate, not core.

/// Return the raw FTL translation source for the demo plugin.
///
/// Mirrors the WIT `plugin-metadata.get-translations(locale) → string` export.
/// Returns an empty string for unsupported locales so the host falls back to
/// English (the same contract as the WIT interface).
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

// ─── Native ClientBackend implementations ──────────────────────────
// These are available when the `native` feature is enabled (default).
// They implement the async `ClientBackend` trait from poly-client.

#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use chrono::{Duration, Utc};
#[cfg(feature = "native")]
use futures::stream::Stream;
#[cfg(feature = "native")]
use poly_client::*;
#[cfg(feature = "native")]
use std::pin::Pin;

/// Demo messenger client for UI testing.
///
/// Generates randomized but realistic-looking data for all
/// messenger operations. No network calls are made.
// DECISION(D12): Demo client created in Phase 2 alongside UI.
#[cfg(feature = "native")]
pub struct DemoClient {
    authenticated: bool,
    session: Option<Session>,
}

#[cfg(feature = "native")]
impl DemoClient {
    /// Create a new demo client.
    pub fn new() -> Self {
        Self {
            authenticated: false,
            session: None,
        }
    }
}

#[cfg(feature = "native")]
impl Default for DemoClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for DemoClient {
    async fn authenticate(&mut self, _credentials: AuthCredentials) -> ClientResult<Session> {
        let session = data::demo_session();
        self.session = Some(session.clone());
        self.authenticated = true;
        Ok(session)
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        self.authenticated = false;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Ok(data::demo_servers())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        data::demo_servers()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("Server {id}")))
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(data::demo_channels(server_id))
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        // Search all servers for the channel
        for server in data::demo_servers() {
            for channel in data::demo_channels(&server.id) {
                if channel.id == id {
                    return Ok(channel);
                }
            }
        }
        Err(ClientError::NotFound(format!("Channel {id}")))
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        Ok(data::demo_sent_message(channel_id, content))
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        Ok(data::demo_sent_reply_message(
            channel_id,
            reply_to_message_id,
            content,
        ))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Ok(data::demo_messages_query(channel_id, &query))
    }

    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Ok(data::demo_search_messages(&query))
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
        Ok(data::demo_pinned_messages(channel_id))
    }

    async fn get_channel_commands(&self, channel_id: &str) -> ClientResult<Vec<ChatCommand>> {
        Ok(data::demo_channel_commands(channel_id))
    }

    async fn get_available_emojis(&self, channel_id: &str) -> ClientResult<Vec<CustomEmoji>> {
        Ok(data::demo_available_emojis(channel_id))
    }

    async fn get_available_stickers(&self, channel_id: &str) -> ClientResult<Vec<StickerItem>> {
        Ok(data::demo_available_stickers(channel_id))
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        data::demo_users()
            .into_iter()
            .find(|u| u.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("User {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(data::demo_users().into_iter().take(8).collect())
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(data::demo_users())
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(data::demo_groups_v2())
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        // Demo client: UI updates local state; no real backend call needed.
        Ok(())
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(data::demo_dm_channels())
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        data::demo_dm_channels()
            .into_iter()
            .find(|dm| dm.user.id == user_id)
            .ok_or_else(|| ClientError::NotFound(format!("DM user {user_id}")))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        let session = self.session.clone().unwrap_or_else(data::demo_session);
        Ok(DmChannel {
            id: "dm-demo-saved-self".to_string(),
            user: session.user,
            last_message: None,
            unread_count: 0,
            backend: BackendType::Demo,
            account_id: data::DEMO_ACCOUNT_ID.to_string(),
        })
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(data::demo_notifications())
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        // Demo client: accept/deny is handled by host-side state updates after a successful
        // backend response. Return success so the notifications UI can exercise that flow.
        Ok(())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Online)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }

    async fn get_voice_participants(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(data::demo_voice_participants(channel_id))
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        #[cfg(target_arch = "wasm32")]
        {
            // The demo dataset is already preloaded in web/Electron builds.
            // Returning an empty live stream keeps demo mode functional there
            // without relying on unsupported/native timer behavior.
            Box::pin(futures::stream::empty())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let users = data::demo_users();
            // Server text channels that receive simulated messages.
            let server_channels = vec![
                "ch-general",
                "ch-off-topic",
                "ch-rust",
                "ch-dioxus",
                "ch-minecraft",
                "ch-valorant",
                "ch-recommendations",
            ];
            // DM channels that receive simulated messages (dm-{user_id}).
            let dm_channels = vec!["dm-user-alice", "dm-user-bob", "dm-user-charlie"];
            let server_messages = vec![
                "That's a great point!",
                "I'll look into it. \u{1f527}",
                "Has anyone else seen this?",
                "Working on a fix now...",
                "brb",
                "lol nice one",
                "Can confirm, same issue here.",
                "\u{1f44d}",
                "Just pushed the fix!",
                "Who's up for a game tonight?",
                "This is so cool!",
                "Let's sync tomorrow morning.",
            ];
            let dm_messages = vec![
                "Hey, are you around?",
                "Did you see the latest update?",
                "Let's catch up soon!",
                "Thanks for the help earlier \u{1f64f}",
                "Check this out!",
                "I'll send you the file in a bit.",
                "Haha yeah exactly \u{1f61d}",
                "Makes sense, let's do it!",
            ];

            // Emit a simulated event every 4-8 seconds (staggered cycle).
            let stream = futures::stream::unfold(0u64, move |counter| {
                let users = users.clone();
                let server_channels = server_channels.clone();
                let dm_channels = dm_channels.clone();
                let server_messages = server_messages.clone();
                let dm_messages = dm_messages.clone();
                async move {
                    if users.is_empty() || server_channels.is_empty() {
                        return None;
                    }

                    // Stagger timing: 4s, 6s, 8s, 5s, 7s, 3s cycle
                    let delays = [4u64, 6, 8, 5, 7, 3];
                    let delay_secs = delays
                        .get((counter as usize) % delays.len())
                        .copied()
                        .unwrap_or(5);
                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

                    let user_idx = (counter as usize) % users.len();
                    let user = users.get(user_idx)?;

                    // Rotate: server msg, typing, DM msg, server msg, presence
                    let event = match counter % 5 {
                        // Server channel message
                        0 | 3 => {
                            let ch_idx = (counter as usize) % server_channels.len();
                            let channel_id = (*server_channels.get(ch_idx)?).to_string();
                            let msg_idx = (counter as usize / 5) % server_messages.len();
                            let text = server_messages.get(msg_idx).copied().unwrap_or("...");
                            ClientEvent::MessageReceived {
                                channel_id,
                                message: Message {
                                    id: format!("msg-stream-{counter}"),
                                    author: user.clone(),
                                    content: MessageContent::Text(text.to_string()),
                                    timestamp: chrono::Utc::now(),
                                    attachments: vec![],
                                    reactions: vec![],
                                    reply_to: None,
                                    edited: false,
                                },
                            }
                        }
                        // Typing indicator in a server channel
                        1 => {
                            let ch_idx = (counter as usize) % server_channels.len();
                            let channel_id = (*server_channels.get(ch_idx)?).to_string();
                            ClientEvent::TypingStarted {
                                channel_id,
                                user_id: user.id.clone(),
                                timestamp: chrono::Utc::now(),
                            }
                        }
                        // DM channel message (simulates another user messaging you)
                        2 => {
                            let dm_idx = (counter as usize / 2) % dm_channels.len();
                            let channel_id = (*dm_channels.get(dm_idx)?).to_string();
                            let dm_user_idx = (counter as usize + 1) % users.len();
                            let dm_user = users.get(dm_user_idx)?;
                            let msg_idx = (counter as usize / 3) % dm_messages.len();
                            let text = dm_messages.get(msg_idx).copied().unwrap_or("hey!");
                            ClientEvent::MessageReceived {
                                channel_id,
                                message: Message {
                                    id: format!("msg-stream-dm-{counter}"),
                                    author: dm_user.clone(),
                                    content: MessageContent::Text(text.to_string()),
                                    timestamp: chrono::Utc::now(),
                                    attachments: vec![],
                                    reactions: vec![],
                                    reply_to: None,
                                    edited: false,
                                },
                            }
                        }
                        // Presence change
                        _ => {
                            let statuses = [
                                PresenceStatus::Online,
                                PresenceStatus::Idle,
                                PresenceStatus::DoNotDisturb,
                                PresenceStatus::Online,
                            ];
                            let s_idx = (counter as usize / 3) % statuses.len();
                            let status = statuses
                                .get(s_idx)
                                .cloned()
                                .unwrap_or(PresenceStatus::Online);
                            ClientEvent::PresenceChanged {
                                user_id: user.id.clone(),
                                status,
                            }
                        }
                    };

                    Some((event, counter + 1))
                }
            });

            Box::pin(stream)
        }
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Demo
    }

    fn backend_name(&self) -> &str {
        "Demo"
    }
}

/// Second demo messenger client — the "dog" account (demo2 / 🐶).
///
/// Provides a second set of demo data (4 different servers, separate
/// notifications, different communities) so the multi-account UI can be
/// tested realistically with two simultaneous demo accounts.
#[cfg(feature = "native")]
pub struct DemoClient2 {
    authenticated: bool,
    session: Option<Session>,
}

#[cfg(feature = "native")]
impl DemoClient2 {
    /// Create a new demo2 client.
    pub fn new() -> Self {
        Self {
            authenticated: false,
            session: None,
        }
    }
}

#[cfg(feature = "native")]
impl Default for DemoClient2 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl ClientBackend for DemoClient2 {
    async fn authenticate(&mut self, _credentials: AuthCredentials) -> ClientResult<Session> {
        let session = data::demo2_session();
        self.session = Some(session.clone());
        self.authenticated = true;
        Ok(session)
    }

    async fn logout(&mut self) -> ClientResult<()> {
        self.session = None;
        self.authenticated = false;
        Ok(())
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        Ok(data::demo2_servers())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        data::demo2_servers()
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("Server {id}")))
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        Ok(data::demo2_channels(server_id))
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        for server in data::demo2_servers() {
            for channel in data::demo2_channels(&server.id) {
                if channel.id == id {
                    return Ok(channel);
                }
            }
        }
        Err(ClientError::NotFound(format!("Channel {id}")))
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        Ok(data::demo_sent_message(channel_id, content))
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        Ok(data::demo_sent_reply_message(
            channel_id,
            reply_to_message_id,
            content,
        ))
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Ok(data::demo2_messages_query(channel_id, &query))
    }

    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Ok(data::demo2_search_messages(&query))
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
        Ok(data::demo2_pinned_messages(channel_id))
    }

    async fn get_channel_commands(&self, channel_id: &str) -> ClientResult<Vec<ChatCommand>> {
        Ok(data::demo_channel_commands(channel_id))
    }

    async fn get_available_emojis(&self, channel_id: &str) -> ClientResult<Vec<CustomEmoji>> {
        Ok(data::demo_available_emojis(channel_id))
    }

    async fn get_available_stickers(&self, channel_id: &str) -> ClientResult<Vec<StickerItem>> {
        Ok(data::demo_available_stickers(channel_id))
    }

    async fn get_user(&self, id: &str) -> ClientResult<User> {
        data::demo_users()
            .into_iter()
            .find(|u| u.id == id)
            .ok_or_else(|| ClientError::NotFound(format!("User {id}")))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        // Dog account has a different friend circle
        Ok(data::demo_users().into_iter().skip(2).take(6).collect())
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(data::demo_users().into_iter().take(6).collect())
    }

    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        Ok(data::demo2_groups())
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        // A subset of DMs from a different perspective
        let mut dms: Vec<DmChannel> = data::demo_dm_channels()
            .into_iter()
            .take(3)
            .map(|mut dm| {
                dm.account_id = data::DEMO2_ACCOUNT_ID.to_string();
                dm
            })
            .collect();

        // Add cross-account DM: dog sees cat
        dms.push(DmChannel {
            id: "dm-demo-cat".to_string(),
            user: User {
                id: "demo-cat-user".to_string(),
                display_name: "🐱 Cat (demo)".to_string(),
                avatar_url: Some(data::DEMO_CAT_AVATAR.to_string()),
                presence: PresenceStatus::Online,
                backend: BackendType::Demo,
            },
            last_message: Some(Message {
                id: "msg-dm-cat-latest".to_string(),
                author: User {
                    id: "demo-cat-user".to_string(),
                    display_name: "🐱 Cat (demo)".to_string(),
                    avatar_url: Some(data::DEMO_CAT_AVATAR.to_string()),
                    presence: PresenceStatus::Online,
                    backend: BackendType::Demo,
                },
                content: MessageContent::Text(
                    "fair! 😹 but you have to admit the feature flag organization is *clean* even if it's stolen from my 2023 design"
                        .to_string(),
                ),
                timestamp: Utc::now() - Duration::hours(3),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
            }),
            unread_count: 1,
            backend: BackendType::Demo,
            account_id: data::DEMO2_ACCOUNT_ID.to_string(),
        });

        Ok(dms)
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        self.get_dm_channels()
            .await?
            .into_iter()
            .find(|dm| dm.user.id == user_id)
            .ok_or_else(|| ClientError::NotFound(format!("DM user {user_id}")))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        let session = self.session.clone().unwrap_or_else(data::demo2_session);
        Ok(DmChannel {
            id: "dm-demo2-saved-self".to_string(),
            user: session.user,
            last_message: None,
            unread_count: 0,
            backend: BackendType::Demo,
            account_id: data::DEMO2_ACCOUNT_ID.to_string(),
        })
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(data::demo2_notifications())
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        // Demo client: accept/deny is handled by host-side state updates after a successful
        // backend response. Return success so the notifications UI can exercise that flow.
        Ok(())
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Online)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }

    async fn get_voice_participants(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(vec![])
    }

    fn event_stream(&self) -> Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // Demo2 emits no live events for simplicity
        Box::pin(futures::stream::empty())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Demo
    }

    fn backend_name(&self) -> &str {
        "Demo (Dog)"
    }
}
