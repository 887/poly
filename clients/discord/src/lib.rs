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

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "discord";

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
#[must_use]
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
    #[must_use]
    pub fn map_discord_channel_type(raw: u8) -> ChannelType {
        let dc = match raw {
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
            // 0 (GuildText) and any other unknown raw → fall back to GuildText
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
    /// Stored version override (None = use DEFAULT_CLIENT_VERSION).
    version_override: Mutex<Option<String>>,
}

#[cfg(feature = "native")]
impl DiscordClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            http: DiscordHttpClient::new("https://discord.com".to_string()),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
            gateway_url: None,
            version_override: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
            gateway_url: None,
            version_override: Mutex::new(None),
        }
    }

    /// Create a client with a REST base URL and a WS gateway URL.
    ///
    /// `gateway_ws_url` is the WebSocket URL the client will connect to in
    /// `event_stream()`.  Example: `"ws://127.0.0.1:9999/gateway/ws"`.
    #[must_use]
    pub fn with_base_url_and_gateway(base_url: String, gateway_ws_url: String) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
            gateway_url: Some(gateway_ws_url),
            version_override: Mutex::new(None),
        }
    }

    fn account_id(&self) -> String {
        self.account_id.clone().unwrap_or_default()
    }

    fn account_display_name(&self) -> String {
        self.account_display_name.clone().unwrap_or_default()
    }

    fn discord_user_to_poly(&self, u: api::DiscordUser) -> User {
        let cdn_base = self.http.cdn_base_url();
        let avatar_url = u.avatar.as_ref().map(|hash| {
            format!(
                "{}/avatars/{}/{}.png?size=128",
                cdn_base.trim_end_matches('/'),
                u.id,
                hash,
            )
        });
        User {
            id: u.id.to_string(),
            display_name: u.global_name.unwrap_or(u.username),
            avatar_url,
            presence: PresenceStatus::Online,
            backend: BackendType::from(crate::SLUG),
        }
    }

    fn discord_message_to_poly(&self, m: api::DiscordMessage) -> Message {
        let author = self.discord_user_to_poly(m.author);
        let timestamp = chrono::DateTime::parse_from_rfc3339(&m.timestamp).map_or_else(
            |_| chrono::Utc::now(),
            |dt| dt.with_timezone(&chrono::Utc),
        );
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
            preview_image_url: None,
        }
    }

    /// Map a Discord `ChannelType` (twilight-model) to `poly_client::ChannelType`.
    fn map_channel_type(dc: twilight_model::channel::ChannelType) -> ChannelType {
        use twilight_model::channel::ChannelType as DC;
        match dc {
            DC::GuildVoice | DC::GuildStageVoice => ChannelType::Voice,
            DC::GuildAnnouncement => ChannelType::Announcement,
            DC::AnnouncementThread | DC::PublicThread | DC::PrivateThread => ChannelType::Thread,
            DC::GuildForum | DC::GuildMedia => ChannelType::Forum,
            // GuildText is the canonical text type; categories aren't exposed
            // as their own channels in the UI so they fall back to Text;
            // Private (DM), Group (group DM), GuildDirectory, Unknown(_), and
            // any future-added variant also fall back to Text.
            DC::GuildText
            | DC::GuildCategory
            | DC::Private
            | DC::Group
            | DC::GuildDirectory
            | DC::Unknown(_)
            | _ => ChannelType::Text,
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
            server_id: ch
                .guild_id
                .map_or_else(|| server_id.to_string(), |id| id.to_string()),
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
                    .map(std::string::ToString::to_string);
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
        "op": 2_i32,
        "d": {
            "token": "",
            "intents": 513_i32,
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
            TungsteniteMsg::Binary(_)
            | TungsteniteMsg::Ping(_)
            | TungsteniteMsg::Pong(_)
            | TungsteniteMsg::Frame(_) => continue,
        };

        let frame: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let op = frame
            .get("op")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

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
            AuthCredentials::DeviceCode { .. } | AuthCredentials::PolyServer { .. } => {
                return Err(ClientError::AuthFailed(
                    "Discord requires a user token or email+password".into(),
                ));
            }
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
                backend: BackendType::from(crate::SLUG),
            },
            token,
            backend: BackendType::from(crate::SLUG),
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
        let cdn_base = self.http.cdn_base_url();
        Ok(self.http.get_guilds().await?.into_iter().map(|g| {
            let icon_url = g.icon.as_deref()
                .map(|hash| format!("{}/icons/{}/{}.png?size=128", cdn_base.trim_end_matches('/'), g.id, hash));
            let banner_url = g.banner.as_deref()
                .map(|hash| format!("{}/banners/{}/{}.png", cdn_base.trim_end_matches('/'), g.id, hash));
            Server {
                id: g.id.to_string(),
                name: g.name,
                icon_url,
                banner_url,
                categories: vec![],
                backend: BackendType::from(crate::SLUG),
                unread_count: 0,
                mention_count: 0,
                account_id: account_id.clone(),
                account_display_name: account_name.clone(),
                default_channel_id: g.system_channel_id,
                description: None,
                star_count: None,
                language: None,
                forks_count: None,
                open_issues_count: None,
            }
        }).collect())
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        let account_id = self.account_id();
        let account_name = self.account_display_name();
        let cdn_base = self.http.cdn_base_url();
        let g = self.http.get_guild(id).await?;
        let icon_url = g.icon.as_deref()
            .map(|hash| format!("{}/icons/{}/{}.png?size=128", cdn_base.trim_end_matches('/'), g.id, hash));
        let banner_url = g.banner.as_deref()
            .map(|hash| format!("{}/banners/{}/{}.png", cdn_base.trim_end_matches('/'), g.id, hash));
        Ok(Server {
            id: g.id.to_string(),
            name: g.name,
            icon_url,
            banner_url,
            categories: vec![],
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id,
            account_display_name: account_name,
            default_channel_id: g.system_channel_id,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        })
    }

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        // The Discord API accepts `banner` as a base64 data URI for real Discord.
        // Our test server (Spacebar-compatible) accepts a URL string directly.
        // We pass the value as-is — for test servers this is a URL; for real
        // Discord the caller is responsible for encoding.
        let body = serde_json::json!({ "banner": banner_url });
        self.http
            .patch_guild(server_id, body)
            .await
            .map(|_| ())
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

    // --- Forum channels (H.2.b — moved to ForumBackend) ---

    fn as_forum(&self) -> Option<&dyn poly_client::ForumBackend> {
        Some(self)
    }

    // --- Thread channels (H.2.c — moved to ThreadsBackend) ---

    fn as_threads(&self) -> Option<&dyn poly_client::ThreadsBackend> {
        Some(self)
    }

    // --- Moderation (H.3.a — moved to ModerationBackend) ---

    fn as_moderation(&self) -> Option<&dyn poly_client::ModerationBackend> {
        Some(self)
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

    // ── Social graph methods moved to SocialGraphBackend (H.3.b) ─────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, _channel_id: &str) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        Ok(vec![])
    }

    async fn get_voice_participants(&self, _channel_id: &str) -> ClientResult<Vec<VoiceParticipant>> {
        Ok(vec![])
    }

    // ── Moderation methods moved to ModerationBackend (H.3.a) ────────────────

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
        BackendType::from(crate::SLUG)
    }

    fn backend_name(&self) -> &str {
        "Discord"
    }

    fn backend_capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            has_roles: true,
            has_kick: true,
            has_ban: true,
            has_timed_ban: true,
            has_channel_mgmt: true,
            has_moderation_log: true,
            ..BackendCapabilities::FULL_SOCIAL_CHAT
        }
    }

    // ── WP 1 / F10 — state-aware context menus ──────────────────────────────

    async fn get_context_menu_items(
        &self, target: MenuTargetKind, target_id: &str,
    ) -> Result<Vec<MenuItem>, ClientError> {
        match target {
            MenuTargetKind::Server => {
                // State-aware: Mute Server / Unmute Server, plus static items.
                let muted = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_servers.contains(target_id);
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
                let muted = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_channels.contains(target_id);
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
                let blocked = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked_users.contains(target_id);
                let is_friend = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).friend_ids.contains(target_id);
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
                let muted = self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_dms.contains(target_id);
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
            // Server / channel / user / message actions that are pure no-ops at this layer.
            "invite-people"
            | "privacy-settings"
            | "edit-per-server-profile"
            | "server-boost"
            | "leave-server"
            | "mark-channel-read"
            | "open-dm"
            | "copy-message-link"
            | "delete-message"
            | "close-dm" => Ok(ActionOutcome::Noop),
            "mute-server" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_servers.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-server" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_servers.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // Channel actions
            "mute-channel" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_channels.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-channel" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_channels.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // User actions
            "add-friend" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).friend_ids.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "remove-friend" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).friend_ids.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            "block-user" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked_users.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unblock-user" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).blocked_users.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
            // DM actions
            "mute-dm" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_dms.insert(target_id.to_string());
                Ok(ActionOutcome::Noop)
            }
            "unmute-dm" => {
                self.menu_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).muted_dms.remove(target_id);
                Ok(ActionOutcome::Noop)
            }
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

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
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

    /// Account-level overview: a card grid of the user's Discord guilds.
    ///
    /// Each card shows the guild name, description (if any), and a
    /// `"N members · X unread · @Y mentions"` meta line.  The actual row
    /// data is fetched by `get_view_rows` when `channel_id == ""`.
    async fn get_account_overview_view(&self) -> ClientResult<ViewDescriptor> {
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-discord-overview-title".to_string()),
                subtitle_key: Some("plugin-discord-overview-subtitle".to_string()),
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

    /// Paged row data for views.
    ///
    /// When `channel_id == ""` (the account-overview sentinel emitted by the
    /// host's `AccountOverviewView` route), returns one [`ViewRow`] per joined
    /// Discord guild, mapping guild name / description / unread badges into the
    /// card-grid layout declared by [`get_account_overview_view`].
    ///
    /// Member counts are fetched in parallel via `GET /guilds/{id}?with_counts=true`.
    /// Individual failures degrade gracefully to `"? members"` so one
    /// rate-limited guild doesn't blank the entire overview.
    ///
    /// Non-overview `channel_id`s return `NotSupported` (channel views are not
    /// yet implemented for Discord).
    async fn get_view_rows(
        &self,
        channel_id: &str,
        _cursor: Option<Cursor>,
        _sort_id: Option<&str>,
        _filter_id: Option<&str>,
        _tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        if !channel_id.is_empty() {
            return Err(ClientError::NotSupported("view-rows not yet implemented".into()));
        }

        let servers = self.get_servers().await?;

        // Fan out member-count fetches in parallel; degrade gracefully on
        // individual failures so one unavailable guild doesn't blank the card.
        let member_counts: Vec<Option<u32>> = {
            use futures::future;
            let futs: Vec<_> = servers
                .iter()
                .map(|s| self.http.get_guild_with_counts(&s.id))
                .collect();
            future::join_all(futs)
                .await
                .into_iter()
                .map(|r| r.ok().and_then(|g| g.approximate_member_count))
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
                    preview_image_url: None,
                    is_video: false,
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
        // Stickers/GIF picker lives in the unified MediaPickerPopup
        // (composer-common emoji button → tabs for emoji/GIF/stickers).
        // Don't duplicate it as a separate composer button.
        Ok(vec![])
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
        Err(ClientError::NotFound(format!("unknown composer action: {action_id}")))
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

    // ── Social graph methods moved to SocialGraphBackend (H.3.b) ────────────
    // ── DMs and groups moved to DmsAndGroupsBackend (H.3.c) ─────────────────

    /// Send a server invite to a specific user via DM.
    ///
    /// Two-step:
    /// 1. Fetch the server's `default_channel_id` (system channel), then create
    ///    an invite with `POST /channels/:channel_id/invites`.
    /// 2. Open a DM with the user and send the invite URL as a message.
    ///
    /// If the server has no system channel configured, returns `NotSupported`.
    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()> {
        // Step 1: resolve system channel.
        let guild = self.http.get_guild(server_id).await?;
        let system_channel_id = guild.system_channel_id.ok_or_else(|| {
            ClientError::NotSupported(
                "invite_user_to_server: server has no system channel; cannot create invite".to_string(),
            )
        })?;

        // Step 2: create invite (1 day, 1 use).
        let invite_code = self
            .http
            .create_invite(&system_channel_id, 86400, 1)
            .await?;
        let invite_url = format!("https://discord.gg/{invite_code}");

        // Step 3: open DM and send the invite URL.
        let dm_channel_id = self.http.open_dm(user_id).await?;
        self.http.send_message(&dm_channel_id, &invite_url).await?;
        Ok(())
    }

    fn get_signup_method(&self, _server_url: Option<&str>) -> SignupMethod {
        SignupMethod::External("https://discord.com/register".into())
    }

    fn client_version(&self) -> String {
        self.version_override
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string())
    }

    async fn set_client_version_override(
        &self,
        version_override: Option<String>,
    ) -> ClientResult<()> {
        let new_ua = version_override
            .clone()
            .unwrap_or_else(|| http::DEFAULT_CLIENT_VERSION.to_string());
        if let Ok(mut lock) = self.version_override.lock() {
            *lock = version_override;
        }
        self.http.set_user_agent(new_ua);
        Ok(())
    }
}

// ── H.2.b — ForumBackend ─────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ForumBackend for DiscordClient {
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

        let cap = usize::try_from(limit.unwrap_or(50).min(100)).unwrap_or(usize::MAX);

        // Fetch all active threads in the guild, filter to this forum.
        let active = self.http.get_active_threads(&guild_id).await?;
        let mut threads: Vec<api::DiscordChannel> = active
            .threads
            .into_iter()
            .filter(|t| {
                t.parent_id
                    .is_some_and(|pid| pid.to_string() == forum_channel_id)
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
                .map(|tags| tags.iter().map(std::string::ToString::to_string).collect())
                .unwrap_or_default();
            posts.push(ForumPost {
                thread: Self::discord_thread_to_thread_info(&t),
                applied_tags,
                starter_message_id,
            });
        }

        Ok(posts)
    }

    async fn create_forum_post(
        &self,
        _forum_channel_id: &str,
        _title: &str,
        _body: &str,
        _tags: Vec<String>,
    ) -> ClientResult<ForumPost> {
        Err(ClientError::NotSupported("create_forum_post".to_string()))
    }

    async fn get_recent_comments(
        &self,
        _channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported("get_recent_comments".to_string()))
    }
}

// ── H.2.c — ThreadsBackend ───────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ThreadsBackend for DiscordClient {
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
}

// ── H.3.a — ModerationBackend ────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ModerationBackend for DiscordClient {
    /// B-DS-1: Compute effective permissions for the authenticated user.
    ///
    /// Fetches `GET /guilds/{id}/members/@me` to get role IDs, then
    /// `GET /guilds/{id}/roles` for the permission bitfields. Combines via OR.
    /// Guild owner gets all flags true regardless of roles.
    async fn get_my_permissions(
        &self,
        server_id: &str,
        _channel_id: Option<&str>,
    ) -> ClientResult<MemberPermissions> {
        use twilight_model::id::marker::RoleMarker;
        use twilight_model::id::Id as TwilightId;

        // Discord permission bit constants.
        const KICK_MEMBERS: i64 = 1 << 1;
        const BAN_MEMBERS: i64 = 1 << 2;
        const ADMINISTRATOR: i64 = 1 << 3;
        const MANAGE_CHANNELS: i64 = 1 << 4;
        const MANAGE_GUILD: i64 = 1 << 5;
        const MANAGE_MESSAGES: i64 = 1 << 13;
        const MANAGE_ROLES: i64 = 1 << 28;
        const MODERATE_MEMBERS: i64 = 1 << 40;

        let member = self.http.get_guild_member_me(server_id).await?;
        let all_roles = self.http.get_guild_roles(server_id).await?;
        let guild = self.http.get_guild(server_id).await?;

        // Determine if caller is the guild owner.
        let caller_id = self.account_id();
        let is_owner = guild
            .owner_id
            .as_deref()
            .is_some_and(|oid| oid == caller_id);

        if is_owner {
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

        // Build a set of the caller's role IDs for fast lookup.
        let member_role_ids: std::collections::HashSet<TwilightId<RoleMarker>> =
            member.roles.into_iter().collect();

        // Find @everyone role (same ID as the guild).
        let everyone_id: u64 = server_id.parse().unwrap_or(0);

        // Combine permission bits: start with @everyone, then OR in member roles.
        let mut effective: i64 = 0;
        let mut highest_role_name = "Member".to_string();
        let mut highest_position = 0u32;

        for role in &all_roles {
            let role_id_u64 = role.id.get();
            let is_everyone = role_id_u64 == everyone_id;
            let is_member_role = member_role_ids.contains(&role.id);

            if is_everyone || is_member_role {
                let bits: i64 = role.permissions.parse().unwrap_or(0);
                effective |= bits;
                if is_member_role && role.position > highest_position {
                    highest_position = role.position;
                    highest_role_name = role.name.clone();
                }
            }
        }

        let has = |flag: i64| (effective & ADMINISTRATOR != 0) || (effective & flag != 0);

        Ok(MemberPermissions {
            manage_server: has(MANAGE_GUILD),
            manage_channels: has(MANAGE_CHANNELS),
            manage_roles: has(MANAGE_ROLES),
            kick_members: has(KICK_MEMBERS),
            ban_members: has(BAN_MEMBERS),
            manage_messages: has(MANAGE_MESSAGES),
            timeout_members: has(MODERATE_MEMBERS),
            display_role: highest_role_name,
            power_level: None,
        })
    }

    /// B-DS-2: Kick a member from the guild.
    async fn kick_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
    ) -> ClientResult<()> {
        self.http.kick_member(server_id, member_id, reason).await
    }

    /// B-DS-3: Permanently ban a member.
    ///
    /// Discord bans are always permanent — `timeout_member` handles timed
    /// suspensions via `communication_disabled_until`.
    async fn ban_member(
        &self,
        server_id: &str,
        member_id: &str,
        reason: Option<&str>,
        delete_message_history_secs: Option<u64>,
    ) -> ClientResult<()> {
        self.http
            .ban_member(server_id, member_id, reason, delete_message_history_secs)
            .await
    }

    /// B-DS-4: Unban a member.
    async fn unban_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http.unban_member(server_id, member_id).await
    }

    /// B-DS-5: List current bans.
    async fn get_bans(&self, server_id: &str) -> ClientResult<Vec<BannedMember>> {
        let bans = self.http.get_bans(server_id).await?;
        Ok(bans
            .into_iter()
            .map(|b| BannedMember {
                user_id: b.user.id.to_string(),
                display_name: b.user.global_name.unwrap_or(b.user.username),
                avatar_url: None,
                reason: b.reason,
                expires_at: None, // Discord bans are permanent
                banned_at: None,
            })
            .collect())
    }

    /// B-DS (timeout): Temporarily suspend a member via `communication_disabled_until`.
    async fn timeout_member(
        &self,
        server_id: &str,
        member_id: &str,
        until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> ClientResult<()> {
        let iso = until.to_rfc3339();
        self.http
            .set_member_timeout(server_id, member_id, Some(&iso))
            .await
    }

    /// B-DS (untimeout): Clear an active timeout.
    async fn untimeout_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        self.http.set_member_timeout(server_id, member_id, None).await
    }

    /// B-DS-6: Delete a single message.
    async fn delete_message(&self, channel_id: &str, message_id: &str) -> ClientResult<()> {
        self.http.delete_message(channel_id, message_id).await
    }

    /// B-DS-7: Update channel metadata.
    async fn update_channel(
        &self,
        channel_id: &str,
        update: UpdateChannelParams,
    ) -> ClientResult<()> {
        let mut body = serde_json::json!({});
        if let Some(obj) = body.as_object_mut() {
            if let Some(name) = &update.name {
                obj.insert("name".to_string(), serde_json::json!(name));
            }
            if let Some(topic) = &update.topic {
                obj.insert("topic".to_string(), serde_json::json!(topic));
            }
            if let Some(slow) = update.slow_mode_secs {
                obj.insert("rate_limit_per_user".to_string(), serde_json::json!(slow));
            }
            if let Some(nsfw) = update.nsfw {
                obj.insert("nsfw".to_string(), serde_json::json!(nsfw));
            }
            if let Some(pos) = update.position {
                obj.insert("position".to_string(), serde_json::json!(pos));
            }
        }
        self.http.patch_channel(channel_id, body).await.map(|_| ())
    }

    /// B-DS-8: Reorder channels within a guild.
    async fn reorder_channels(
        &self,
        server_id: &str,
        ordering: Vec<String>,
    ) -> ClientResult<()> {
        let payload: Vec<serde_json::Value> = ordering
            .into_iter()
            .enumerate()
            .map(|(pos, id)| serde_json::json!({ "id": id, "position": pos }))
            .collect();
        self.http.reorder_channels(server_id, &payload).await
    }

    /// B-DS-9: Fetch moderation log from Discord audit log.
    ///
    /// Maps action types: 20=kick, 22=ban_add, 23=ban_remove, 12=channel_update, 72=msg_delete.
    async fn get_moderation_log(
        &self,
        server_id: &str,
        limit: usize,
    ) -> ClientResult<Vec<ModerationLogEntry>> {
        const MODERATION_ACTION_TYPES: &[u32] = &[20, 22, 23, 12, 72];

        let resp = self.http.get_audit_log(server_id, limit).await?;

        // Build a user lookup map from the embedded users array.
        let user_map: std::collections::HashMap<String, api::DiscordUser> = resp
            .users
            .into_iter()
            .map(|u| (u.id.to_string(), u))
            .collect();

        let entries = resp
            .audit_log_entries
            .into_iter()
            .filter(|e| MODERATION_ACTION_TYPES.contains(&e.action_type))
            .map(|entry| {
                let action = match entry.action_type {
                    20 => ModerationAction::MemberKicked,
                    22 => ModerationAction::MemberBanned,
                    23 => ModerationAction::MemberUnbanned,
                    12 => ModerationAction::ChannelUpdated,
                    72 => ModerationAction::MessageDeleted,
                    _ => ModerationAction::Other(entry.action_type.to_string()),
                };

                // Resolve moderator user from the map.
                let moderator_id = entry
                    .user_id
                    .map(|id| id.to_string())
                    .unwrap_or_default();
                let moderator = user_map.get(&moderator_id).map_or_else(
                    || User {
                        id: moderator_id.clone(),
                        display_name: moderator_id.clone(),
                        avatar_url: None,
                        presence: PresenceStatus::Offline,
                        backend: BackendType::from(crate::SLUG),
                    },
                    |u| self.discord_user_to_poly(u.clone()),
                );

                // The audit log entry's snowflake ID encodes the timestamp.
                // Discord snowflake epoch: 2015-01-01T00:00:00.000Z = 1420070400000ms
                let entry_id_u64 = entry.id.get();
                let discord_epoch_ms: u64 = 1_420_070_400_000;
                let ts_ms = (entry_id_u64 >> 22).wrapping_add(discord_epoch_ms);
                let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                    i64::try_from(ts_ms).unwrap_or(i64::MAX),
                )
                .map_or_else(
                    || chrono::Utc::now().to_rfc3339(),
                    |dt| dt.to_rfc3339(),
                );

                ModerationLogEntry {
                    id: entry.id.to_string(),
                    action,
                    moderator,
                    target_user_id: entry.target_id.clone(),
                    target_display_name: None,
                    channel_id: None,
                    message_id: None,
                    reason: entry.reason,
                    timestamp,
                }
            })
            .collect();

        Ok(entries)
    }

    async fn get_server_roles(&self, server_id: &str) -> ClientResult<Vec<Role>> {
        let discord_roles = self.http.get_guild_roles(server_id).await?;

        let mut roles: Vec<Role> = discord_roles
            .into_iter()
            .map(|dr| {
                let perms_bits: i64 = dr.permissions.parse().unwrap_or(0);
                // Derive display_role name as the role's own name.
                let admin_bit: i64 = 1_i64 << 3_i32;
                let is_admin = perms_bits & admin_bit != 0;
                let has = |flag_bit: i64| is_admin || (perms_bits & flag_bit != 0);
                let permissions = MemberPermissions {
                    manage_server: has(1_i64 << 5_i32),
                    manage_channels: has(1_i64 << 4_i32),
                    manage_roles: has(1_i64 << 28_i32),
                    kick_members: has(1_i64 << 1_i32),
                    ban_members: has(1_i64 << 2_i32),
                    manage_messages: has(1_i64 << 13_i32),
                    timeout_members: has(1_i64 << 40_i32),
                    display_role: dr.name.clone(),
                    power_level: None,
                };
                let color = if dr.color == 0 {
                    None
                } else {
                    Some(format!("#{:06X}", dr.color))
                };
                Role {
                    id: dr.id.to_string(),
                    name: dr.name,
                    color,
                    permissions,
                    position: dr.position,
                }
            })
            .collect();

        // Sort by position descending (highest rank first).
        roles.sort_by(|a, b| b.position.cmp(&a.position));
        Ok(roles)
    }
}

// ── H.3.b — SocialGraphBackend ───────────────────────────────────────────────
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SocialGraphBackend for DiscordClient {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        let u = self.http.get_user(id).await?;
        Ok(self.discord_user_to_poly(u))
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        Ok(vec![])
    }

    async fn add_friend(&self, user_id: &str) -> ClientResult<()> {
        self.http.put_relationship(user_id, 1).await
    }

    async fn remove_friend(&self, user_id: &str) -> ClientResult<()> {
        self.http.delete_relationship(user_id).await
    }

    async fn respond_to_friend_request(
        &self,
        _user_id: &str,
        _accept: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "respond_to_friend_request: Discord does not expose this endpoint".to_string(),
        ))
    }

    /// Discord does not expose per-friend nicknames via its public API.
    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "set_friend_nickname: Discord does not expose friend nicknames via API".to_string(),
        ))
    }

    /// Set or clear a private note about a user. `None` clears (sends empty string).
    async fn set_user_note(&self, user_id: &str, note: Option<&str>) -> ClientResult<()> {
        self.http.put_user_note(user_id, note.unwrap_or("")).await
    }

    /// Block a user. Sends `PUT /users/@me/relationships/:user_id` with `{"type": 2}`.
    async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        self.http.put_relationship(user_id, 2).await
    }

    /// Unblock a user. Mirrors `block_user` using DELETE on the same endpoint.
    async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        self.http.delete_relationship(user_id).await
    }

    /// Discord does not expose a distinct "ignore" concept separate from blocking.
    /// We fall back to block so the action has a real effect rather than silently
    /// dropping the request.
    async fn ignore_user(&self, user_id: &str) -> ClientResult<()> {
        // TODO(discord): Discord has no server-side "ignore" — mapping to block.
        self.http.put_relationship(user_id, 2).await
    }

    /// Reverse of `ignore_user` — same as unblock since we mapped ignore → block.
    async fn unignore_user(&self, user_id: &str) -> ClientResult<()> {
        // TODO(discord): mirroring unblock since ignore maps to block above.
        self.http.delete_relationship(user_id).await
    }

    async fn get_presence(&self, _user_id: &str) -> ClientResult<PresenceStatus> {
        Ok(PresenceStatus::Offline)
    }

    async fn set_presence(&self, _status: PresenceStatus) -> ClientResult<()> {
        Ok(())
    }
}

// Discord supports DM channels, group DMs, and lifecycle management.
// Mute/unmute require guild context and are not yet implemented.

#[async_trait::async_trait]
impl poly_client::DmsAndGroupsBackend for DiscordClient {
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
                    backend: BackendType::from(crate::SLUG),
                },
                last_message: None,
                unread_count: 0,
                backend: BackendType::from(crate::SLUG),
                account_id: account_id.clone(),
            })
            .collect())
    }

    async fn open_direct_message_channel(&self, _user_id: &str) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_direct_message_channel: not yet implemented for Discord".to_string(),
        ))
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        Err(ClientError::NotSupported(
            "open_saved_messages_channel: Discord has no saved-messages concept".to_string(),
        ))
    }

    async fn add_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_group_member: use add_users_to_group_dm for Discord".to_string(),
        ))
    }

    async fn remove_group_member(&self, _group_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "remove_group_member: not yet implemented for Discord".to_string(),
        ))
    }

    async fn add_users_to_group_dm(
        &self,
        channel_id: &str,
        user_ids: &[String],
    ) -> ClientResult<()> {
        for uid in user_ids {
            self.http.add_group_dm_recipient(channel_id, uid).await?;
        }
        Ok(())
    }

    async fn close_dm_channel(&self, channel_id: &str) -> ClientResult<()> {
        self.http.delete_channel(channel_id).await
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "mute_conversation: Discord notification settings require guild context; not yet implemented".to_string(),
        ))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "unmute_conversation: Discord notification settings require guild context; not yet implemented".to_string(),
        ))
    }

    async fn leave_group_dm(&self, channel_id: &str) -> ClientResult<()> {
        self.http.delete_channel(channel_id).await
    }

    async fn edit_group_dm(
        &self,
        channel_id: &str,
        name: Option<&str>,
        avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        let mut body = serde_json::json!({});
        if let Some(obj) = body.as_object_mut() {
            if let Some(n) = name {
                obj.insert("name".to_string(), serde_json::json!(n));
            }
            if let Some(icon) = avatar_url {
                obj.insert("icon".to_string(), serde_json::json!(icon));
            }
        }
        self.http.patch_channel(channel_id, body).await.map(|_| ())
    }
}
