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
#[cfg(feature = "gateway")]
use tokio::sync::mpsc::UnboundedSender;
#[cfg(feature = "gateway")]
use tokio_tungstenite::tungstenite::Message as TungsteniteMsg;

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

/// Test helpers for `tests/` integration tests.
///
/// Provides access to internal mapping functions.
/// Hidden from docs; always compiled with the `native` feature so that
/// integration tests (which are compiled as separate crates) can import them.
#[cfg(feature = "native")]
#[doc(hidden)]
pub mod test_helpers {
    use super::*;
    use twilight_model::channel::ChannelType as DC;

    /// Map a raw Discord channel type integer to `poly_client::ChannelType`.
    pub fn map_discord_channel_type(raw: u8) -> ChannelType {
        let dc = match raw {
            0 => DC::GuildText,
            1 => DC::Private,
            2 => DC::GuildVoice,
            4 => DC::GuildCategory,
            5 => DC::GuildAnnouncement,
            10 => DC::AnnouncementThread,
            11 => DC::PublicThread,
            12 => DC::PrivateThread,
            13 => DC::GuildStageVoice,
            14 => DC::GuildDirectory,
            15 => DC::GuildForum,
            16 => DC::GuildMedia,
            _ => DC::GuildText,
        };
        DiscordClient::map_channel_type(dc)
    }

    /// Deserialize a JSON string as a `DiscordChannel` and map it to a `poly_client::Channel`.
    ///
    /// `fallback_server_id` is used when `guild_id` is absent from the JSON.
    /// Returns `Err` on JSON parse failure.
    pub fn channel_from_json(
        json: &str,
        fallback_server_id: &str,
    ) -> Result<Channel, serde_json::Error> {
        let dc: api::DiscordChannel = serde_json::from_str(json)?;
        let client = DiscordClient::new();
        Ok(client.discord_channel_to_poly(dc, fallback_server_id))
    }

    /// Deserialize a JSON string as a `DiscordMessage` and map it to a `poly_client::Message`.
    /// Returns `Err` on JSON parse failure.
    pub fn message_from_json(json: &str) -> Result<Message, serde_json::Error> {
        let dm: api::DiscordMessage = serde_json::from_str(json)?;
        let client = DiscordClient::new();
        Ok(client.discord_message_to_poly(dm))
    }

    /// Parse a Discord Gateway JSON string into `ClientEvent`s.
    ///
    /// Convenience wrapper for use in unit tests.
    pub fn gateway_events_from_json(
        event_name: &str,
        data_json: &str,
        fallback_server_id: &str,
    ) -> Result<Vec<ClientEvent>, serde_json::Error> {
        let data: serde_json::Value = serde_json::from_str(data_json)?;
        let client = DiscordClient::new();
        Ok(client.parse_gateway_event(event_name, &data, fallback_server_id))
    }
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
    /// Optional WebSocket gateway URL.  When `Some`, `event_stream()` connects
    /// to this URL and forwards parsed gateway events.  When `None`, the stream
    /// is `stream::pending()` (no events).
    gateway_url: Option<String>,
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
            gateway_url: None,
        }
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
            gateway_url: None,
        }
    }

    /// Create a client with a REST base URL and a WS gateway URL.
    ///
    /// `gateway_ws_url` is the WebSocket URL the client will connect to in
    /// `event_stream()`.  Example: `"ws://127.0.0.1:9999/gateway/ws"`.
    pub fn with_base_url_and_gateway(base_url: String, gateway_ws_url: String) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
            gateway_url: Some(gateway_ws_url),
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
        let thread = m.thread.map(|t| Self::discord_thread_to_thread_info(&t));
        Message {
            id: m.id.to_string(),
            author,
            content: MessageContent::Text(m.content),
            timestamp,
            attachments: vec![],
            reactions: vec![],
            reply_to: None,
            edited: m.edited_timestamp.is_some(),
            thread,
        }
    }

    /// Map a Discord `ChannelType` (twilight-model) to `poly_client::ChannelType`.
    fn map_channel_type(dc: twilight_model::channel::ChannelType) -> ChannelType {
        use twilight_model::channel::ChannelType as DC;
        match dc {
            DC::GuildText => ChannelType::Text,
            DC::GuildVoice | DC::GuildStageVoice => ChannelType::Voice,
            DC::GuildCategory => ChannelType::Text, // categories are not exposed as channels
            DC::GuildAnnouncement => ChannelType::Announcement,
            DC::AnnouncementThread | DC::PublicThread | DC::PrivateThread => ChannelType::Thread,
            DC::GuildForum | DC::GuildMedia => ChannelType::Forum,
            _ => ChannelType::Text,
        }
    }

    /// Parse `thread_metadata` from a Discord channel object into `poly_client::ThreadMetadata`.
    fn discord_thread_metadata(m: &api::DiscordThreadMetadata) -> ThreadMetadata {
        let archived_at = m.archive_timestamp.as_deref().and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });
        let created_at = m
            .create_timestamp
            .as_deref()
            .and_then(|ts| {
                chrono::DateTime::parse_from_rfc3339(ts)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .unwrap_or_else(chrono::Utc::now);
        ThreadMetadata {
            archived: m.archived,
            auto_archive_minutes: m.auto_archive_duration,
            archived_at,
            locked: m.locked,
            created_at,
        }
    }

    /// Build a `ThreadInfo` from a Discord thread channel object.
    fn discord_thread_to_thread_info(ch: &api::DiscordChannel) -> ThreadInfo {
        ThreadInfo {
            thread_id: ch.id.to_string(),
            parent_channel_id: ch.parent_id.map(|id| id.to_string()).unwrap_or_default(),
            message_count: ch.message_count.unwrap_or(0),
            member_count: ch.member_count.unwrap_or(0),
        }
    }

    /// Convert a Discord channel object to a `poly_client::Channel`.
    ///
    /// Handles both regular channels and thread/forum channels — sets
    /// `thread_metadata`, `parent_channel_id`, and `forum_tags` as appropriate.
    fn discord_channel_to_poly(&self, ch: api::DiscordChannel, server_id: &str) -> Channel {
        let channel_type = Self::map_channel_type(ch.channel_type);
        let thread_metadata = ch
            .thread_metadata
            .as_ref()
            .map(Self::discord_thread_metadata);
        let parent_channel_id = ch.parent_id.map(|id| id.to_string());
        let forum_tags = ch.available_tags.map(|tags| {
            tags.into_iter()
                .map(|t| ForumTag {
                    id: t.id.to_string(),
                    name: t.name,
                    emoji: t.emoji_name.or_else(|| t.emoji_id.map(|id| id.to_string())),
                    moderated: t.moderated,
                })
                .collect::<Vec<_>>()
        });
        Channel {
            id: ch.id.to_string(),
            name: ch.name,
            channel_type,
            server_id: ch.guild_id.map(|id| id.to_string()).unwrap_or_else(|| server_id.to_string()),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags,
            parent_channel_id,
            thread_metadata,
        }
    }

    /// Parse a Discord Gateway JSON payload into zero or more [`ClientEvent`]s.
    ///
    /// Called by the WebSocket event loop once it is connected (TODO 3.3.5).
    /// Handles the thread gateway events required by Phase 3 items 3.8 and 3.9:
    ///
    /// | Gateway event    | Emitted `ClientEvent`                          |
    /// |------------------|------------------------------------------------|
    /// | `THREAD_CREATE`  | `ChannelUpdated(thread_channel)`               |
    /// | `THREAD_UPDATE`  | `ChannelUpdated(thread_channel)`               |
    /// | `THREAD_DELETE`  | `ChannelUpdated` with a tombstone channel       |
    /// | `THREAD_LIST_SYNC` | `ChannelUpdated` for each thread in the list |
    ///
    /// Decision: we re-use `ChannelUpdated` for all thread lifecycle events
    /// rather than adding new `ClientEvent` variants.  The host treats
    /// `ChannelUpdated` as "channel state changed — re-render sidebar/thread
    /// list if you care about this channel".  Adding a new variant would
    /// require a WIT schema change and propagation through every backend's
    /// guest.rs — not warranted here because the UI reaction is identical.
    ///
    /// The `fallback_server_id` is used when `guild_id` is absent from the
    /// payload (Discord omits it on `THREAD_DELETE` events).
    #[cfg(feature = "native")]
    pub fn parse_gateway_event(
        &self,
        event_name: &str,
        data: &serde_json::Value,
        fallback_server_id: &str,
    ) -> Vec<ClientEvent> {
        match event_name {
            // ── 3.8: THREAD_CREATE / THREAD_UPDATE ────────────────────────
            "THREAD_CREATE" | "THREAD_UPDATE" => {
                match serde_json::from_value::<api::DiscordChannel>(data.clone()) {
                    Ok(ch) => {
                        let channel = self.discord_channel_to_poly(ch, fallback_server_id);
                        vec![ClientEvent::ChannelUpdated(channel)]
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "poly_discord::gateway",
                            event = event_name,
                            error = %e,
                            "failed to deserialize thread channel from gateway event"
                        );
                        vec![]
                    }
                }
            }

            // ── 3.8: THREAD_DELETE ────────────────────────────────────────
            // Discord sends a minimal object with just `id`, `guild_id`, and
            // `parent_id` on deletion — not a full channel object.
            // We emit a `ChannelUpdated` with a tombstone Thread channel so
            // subscribers can remove the thread from their caches.
            "THREAD_DELETE" => {
                let thread_id = data
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let server_id = data
                    .get("guild_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(fallback_server_id)
                    .to_string();
                let parent_channel_id = data
                    .get("parent_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if thread_id.is_empty() {
                    return vec![];
                }
                let tombstone = Channel {
                    id: thread_id,
                    name: String::new(),
                    channel_type: ChannelType::Thread,
                    server_id,
                    unread_count: 0,
                    mention_count: 0,
                    last_message_id: None,
                    forum_tags: None,
                    parent_channel_id,
                    thread_metadata: Some(ThreadMetadata {
                        archived: true,
                        locked: true,
                        auto_archive_minutes: 0,
                        archived_at: None,
                        created_at: chrono::Utc::now(),
                    }),
                };
                vec![ClientEvent::ChannelUpdated(tombstone)]
            }

            // ── 3.9: THREAD_LIST_SYNC ─────────────────────────────────────
            // Sent on READY or when the user gains access to a set of channels.
            // Payload: `{ guild_id, channel_ids?, threads: [Thread], ... }`
            "THREAD_LIST_SYNC" => {
                let threads_val = match data.get("threads").and_then(|v| v.as_array()) {
                    Some(arr) => arr.clone(),
                    None => return vec![],
                };
                let guild_id = data
                    .get("guild_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(fallback_server_id);
                let mut events = Vec::with_capacity(threads_val.len());
                for t in threads_val {
                    match serde_json::from_value::<api::DiscordChannel>(t) {
                        Ok(ch) => {
                            let channel = self.discord_channel_to_poly(ch, guild_id);
                            events.push(ClientEvent::ChannelUpdated(channel));
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "poly_discord::gateway",
                                error = %e,
                                "THREAD_LIST_SYNC: failed to deserialize thread"
                            );
                        }
                    }
                }
                events
            }

            // ── MESSAGE_CREATE / MESSAGE_UPDATE ──────────────────────────
            "MESSAGE_CREATE" => {
                match serde_json::from_value::<api::DiscordMessage>(data.clone()) {
                    Ok(m) => {
                        let channel_id = m.channel_id.to_string();
                        let message = self.discord_message_to_poly(m);
                        vec![ClientEvent::MessageReceived { channel_id, message }]
                    }
                    Err(_) => vec![],
                }
            }
            "MESSAGE_UPDATE" => {
                match serde_json::from_value::<api::DiscordMessage>(data.clone()) {
                    Ok(m) => {
                        let channel_id = m.channel_id.to_string();
                        let message = self.discord_message_to_poly(m);
                        vec![ClientEvent::MessageEdited { channel_id, message }]
                    }
                    Err(_) => vec![],
                }
            }
            "MESSAGE_DELETE" => {
                let channel_id = data
                    .get("channel_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let message_id = data
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if channel_id.is_empty() || message_id.is_empty() {
                    vec![]
                } else {
                    vec![ClientEvent::MessageDeleted { channel_id, message_id }]
                }
            }
            "TYPING_START" => {
                let channel_id = data
                    .get("channel_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let user_id = data
                    .get("user_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if channel_id.is_empty() || user_id.is_empty() {
                    vec![]
                } else {
                    vec![ClientEvent::TypingStarted {
                        channel_id,
                        user_id,
                        timestamp: chrono::Utc::now(),
                    }]
                }
            }
            "PRESENCE_UPDATE" => {
                let user_id = data
                    .get("user")
                    .and_then(|u| u.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let status_str = data
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("offline");
                use poly_client::PresenceStatus;
                let status = match status_str {
                    "online" => PresenceStatus::Online,
                    "idle" => PresenceStatus::Idle,
                    "dnd" => PresenceStatus::DoNotDisturb,
                    _ => PresenceStatus::Offline,
                };
                if user_id.is_empty() {
                    vec![]
                } else {
                    vec![ClientEvent::PresenceChanged { user_id, status }]
                }
            }

            _ => vec![],
        }
    }
}

/// Gateway WebSocket connection loop.
///
/// Connects to `gateway_url`, reads JSON frames, calls `parse_gateway_event`
/// on each dispatched event (op 0), and sends the resulting `ClientEvent`s on
/// `tx`.  Exits when the WS closes or `tx` is dropped.
///
/// Protocol decisions (Phase 6.5):
/// - Sends a minimal IDENTIFY on connect so servers can log the connection.
/// - Responds to HEARTBEAT_ACK (op 11) silently.
/// - Does NOT implement reconnect logic — stream simply ends on disconnect.
#[cfg(feature = "gateway")]
async fn gateway_connect_loop(
    gateway_url: String,
    tx: UnboundedSender<ClientEvent>,
) {
    use futures::StreamExt;
    use tokio_tungstenite::connect_async;

    let ws_stream = match connect_async(gateway_url.as_str()).await {
        Ok((ws, _)) => ws,
        Err(e) => {
            tracing::warn!(target: "poly_discord::gateway", url = %gateway_url, error = %e, "gateway connect failed");
            return;
        }
    };

    let (mut write, mut read) = futures::StreamExt::split(ws_stream);

    // Send a minimal IDENTIFY frame so the server knows we connected.
    let identify = serde_json::json!({
        "op": 2,
        "d": {
            "token": "",
            "intents": 513,
            "properties": { "os": "linux", "browser": "poly", "device": "poly" }
        }
    });
    use futures::SinkExt;
    if let Err(e) = write.send(TungsteniteMsg::Text(identify.to_string().into())).await {
        tracing::warn!(target: "poly_discord::gateway", error = %e, "failed to send IDENTIFY");
        return;
    }

    // The client that owns this stream has `&self` access; use a stub for parsing.
    let parser = DiscordClient::new();

    while let Some(msg_result) = read.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(target: "poly_discord::gateway", error = %e, "gateway WS error");
                break;
            }
        };

        let text = match msg {
            TungsteniteMsg::Text(t) => t.to_string(),
            TungsteniteMsg::Close(_) => break,
            _ => continue,
        };

        let frame: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let op = frame.get("op").and_then(|v| v.as_u64()).unwrap_or(0);

        // op 0 = DISPATCH — parse and forward.
        if op == 0 {
            let event_name = frame
                .get("t")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = frame.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let guild_id = data
                .get("guild_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let events = parser.parse_gateway_event(event_name, &data, &guild_id);
            for ev in events {
                if tx.send(ev).is_err() {
                    // Receiver dropped — stream is done.
                    return;
                }
            }
        }
        // op 11 = HEARTBEAT_ACK — no action needed.
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
            default_channel_id: g.system_channel_id,
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
            default_channel_id: g.system_channel_id,
        })
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        use twilight_model::channel::ChannelType as DcChType;
        Ok(self.http.get_guild_channels(server_id).await?.into_iter()
            .filter(|c| matches!(
                c.channel_type,
                DcChType::GuildText
                    | DcChType::GuildAnnouncement
                    | DcChType::GuildForum
                    | DcChType::GuildMedia
            ))
            .map(|c| self.discord_channel_to_poly(c, server_id))
            .collect())
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        let ch = self.http.get_channel(id).await?;
        let server_id = ch.guild_id.map(|gid| gid.to_string()).unwrap_or_default();
        Ok(self.discord_channel_to_poly(ch, &server_id))
    }

    async fn get_forum_posts(
        &self,
        forum_channel_id: &str,
        sort: ForumSortOrder,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>> {
        // Fetch the forum channel to get the guild ID.
        let forum_ch = self.http.get_channel(forum_channel_id).await?;
        let guild_id = forum_ch
            .guild_id
            .map(|id| id.to_string())
            .ok_or_else(|| ClientError::Internal("forum channel missing guild_id".into()))?;

        let cap = limit.unwrap_or(50).min(100) as usize;

        // Fetch all active threads in the guild, filter to this forum.
        let active = self.http.get_active_threads(&guild_id).await?;
        let mut threads: Vec<api::DiscordChannel> = active
            .threads
            .into_iter()
            .filter(|t| {
                t.parent_id
                    .map(|pid| pid.to_string() == forum_channel_id)
                    .unwrap_or(false)
            })
            .collect();

        // Sort per the requested order.
        match sort {
            ForumSortOrder::LatestActivity => {
                // last_message_id is a snowflake — lexicographic sort is chronological.
                // Since we don't have last_message_id on the thread object yet, we fall
                // back to insertion order (Discord returns newest-activity first anyway).
            }
            ForumSortOrder::CreationDate => {
                // Sort by thread creation timestamp, newest first.
                threads.sort_by(|a, b| {
                    let ts_a = a.thread_metadata.as_ref().and_then(|m| m.create_timestamp.as_deref())
                        .unwrap_or("");
                    let ts_b = b.thread_metadata.as_ref().and_then(|m| m.create_timestamp.as_deref())
                        .unwrap_or("");
                    ts_b.cmp(ts_a) // descending
                });
            }
        }

        threads.truncate(cap);

        let mut posts = Vec::with_capacity(threads.len());
        for t in threads {
            let thread_id = t.id.to_string();
            // Fetch the starter message (oldest message) for each thread.
            // Discord returns messages in reverse-chronological order; `after=0`
            // returns the first message ever posted (after snowflake 0).
            let starter_message_id = self
                .http
                .get_thread_messages(&thread_id, Some(1), Some("0"))
                .await
                .ok()
                .and_then(|msgs| msgs.into_iter().next())
                .map(|m| m.id.to_string());
            let applied_tags = t
                .applied_tags
                .as_ref()
                .map(|tags| tags.iter().map(|id| id.to_string()).collect())
                .unwrap_or_default();
            posts.push(ForumPost {
                thread: Self::discord_thread_to_thread_info(&t),
                applied_tags,
                starter_message_id,
            });
        }

        Ok(posts)
    }

    async fn get_active_threads(&self, server_id: &str) -> ClientResult<Vec<ThreadInfo>> {
        let resp = self.http.get_active_threads(server_id).await?;
        Ok(resp.threads.into_iter().map(|t| Self::discord_thread_to_thread_info(&t)).collect())
    }

    async fn get_archived_threads(
        &self,
        parent_channel_id: &str,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ThreadInfo>> {
        let resp = self.http.get_archived_threads_public(parent_channel_id, limit).await?;
        Ok(resp.threads.into_iter().map(|t| Self::discord_thread_to_thread_info(&t)).collect())
    }

    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        self.http.trigger_typing(channel_id).await
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
        #[cfg(feature = "gateway")]
        {
            if let Some(url) = &self.gateway_url {
                let url = url.clone();
                // Spawn a task that connects to the gateway WS and streams events.
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ClientEvent>();
                tokio::spawn(gateway_connect_loop(url, tx));
                return Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(rx));
            }
        }
        // When the `gateway` feature is disabled (WASM plugin target, plain
        // native core consumer), we can't open a WebSocket — return a pending
        // stream. Events arrive via other paths (WIT plugin host, REST poll).
        let _ = &self.gateway_url;
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
