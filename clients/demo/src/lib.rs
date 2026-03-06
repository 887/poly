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
/// `messenger-client` interface using `wit-bindgen`.
/// Only on WASI targets (not `wasm32-unknown-unknown` used by the web frontend).
#[cfg(target_os = "wasi")]
mod guest;

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
#[async_trait]
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

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        if channel_id.starts_with("dm-") {
            Ok(data::demo_dm_messages(channel_id))
        } else if channel_id.starts_with("group-") {
            Ok(data::demo_group_messages(channel_id))
        } else {
            // Try the rich supplement first (covers sparse channels); fall back to base.
            let rich = data::demo2_messages_rich(channel_id);
            if rich.is_empty() {
                Ok(data::demo_messages(channel_id))
            } else {
                Ok(rich)
            }
        }
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

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        Ok(data::demo_dm_channels())
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(data::demo_notifications())
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
        let users = data::demo_users();
        let channels = vec![
            "ch-general",
            "ch-off-topic",
            "ch-rust",
            "ch-dioxus",
            "ch-minecraft",
        ];
        let typing_messages = vec![
            "That's a great point!",
            "I'll look into it.",
            "Has anyone else seen this?",
            "Working on a fix now \u{1f527}",
            "brb",
            "lol nice",
            "Can confirm, same issue here.",
            "\u{1f44d}",
        ];

        // Emit a fake event every 4–8 seconds (alternating cycle).
        let stream = futures::stream::unfold(0u64, move |counter| {
            let users = users.clone();
            let channels = channels.clone();
            let typing_messages = typing_messages.clone();
            async move {
                if users.is_empty() || channels.is_empty() || typing_messages.is_empty() {
                    return None;
                }

                // Stagger timing: 4s, 6s, 8s, 5s, 7s cycle.
                let delays = [4u64, 6, 8, 5, 7];
                let delay_secs = delays
                    .get((counter as usize) % delays.len())
                    .copied()
                    .unwrap_or(5);
                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;

                let user_idx = (counter as usize) % users.len();
                let ch_idx = (counter as usize) % channels.len();
                let user = users.get(user_idx)?;
                let channel_id = (*channels.get(ch_idx)?).to_string();

                let event = match counter % 4 {
                    // Message event.
                    0 | 2 => {
                        let msg_idx = (counter as usize / 4) % typing_messages.len();
                        let text = typing_messages.get(msg_idx).copied().unwrap_or("...");
                        ClientEvent::MessageReceived {
                            channel_id,
                            message: Message {
                                id: format!("msg-live-{counter}"),
                                author: user.clone(),
                                content: MessageContent::Text(text.to_string()),
                                timestamp: chrono::Utc::now(),
                                attachments: vec![],
                                reactions: vec![],
                                edited: false,
                            },
                        }
                    }
                    // Typing event.
                    1 => ClientEvent::TypingStarted {
                        channel_id,
                        user_id: user.id.clone(),
                        timestamp: chrono::Utc::now(),
                    },
                    // Presence change.
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
#[async_trait]
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

    async fn get_messages(
        &self,
        channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        if channel_id.starts_with("dm-") {
            Ok(data::demo_dm_messages(channel_id))
        } else if channel_id.starts_with("group2-") {
            Ok(data::demo_group_messages(channel_id))
        } else {
            // Try rich supplement first; fall back to demo2 base data.
            let rich = data::demo2_messages_rich(channel_id);
            if rich.is_empty() {
                Ok(data::demo2_messages(channel_id))
            } else {
                Ok(rich)
            }
        }
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
                edited: false,
            }),
            unread_count: 1,
            backend: BackendType::Demo,
            account_id: data::DEMO2_ACCOUNT_ID.to_string(),
        });

        Ok(dms)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(data::demo2_notifications())
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
