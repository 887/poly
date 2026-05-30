//! # poly-discord
//!
//! Discord messenger client for Poly.
//!
//! Implements [`poly_client::IsBackend`] against the Discord REST API v10.
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
pub mod build_info;
#[cfg(feature = "native")]
pub mod super_properties;
#[cfg(feature = "native")]
pub mod signup;
/// Phase D — anti-ban behavioural guardrails (rate, slow-mode, permission, typing, voice, health).
#[cfg(feature = "native")]
pub(crate) mod guardrails;
/// Phase E — Nitro tier detection and feature gating.
#[cfg(feature = "native")]
pub(crate) mod nitro;

/// Voice gateway transport — Phase B of `docs/plans/plan-voice-video-calls.md`.
/// NATIVE ONLY — `voice` feature requires `gateway` requires `native`.
/// WASM builds MUST NOT enable this feature (audiopus is FFI, not WASM-safe).
#[cfg(feature = "voice")]
pub mod voice;

/// Discord voice protocol on WASM — compiles on wasm32.
/// Implements the full Discord voice handshake + RTP path in the plugin,
/// routing over generic host-bridge primitives (/host/udp/*, /host/codec/opus/*,
/// /host/aead/*) instead of the old Discord-coupled /host/voice/* routes.
#[cfg(feature = "voice-bridge")]
pub mod voice_bridge;

/// Discord main gateway WebSocket transport for WASM — compiles on wasm32.
/// Connects to wss://gateway.discord.gg, sends op 2 IDENTIFY, and stashes
/// VOICE_STATE_UPDATE / VOICE_SERVER_UPDATE credentials so that
/// join_voice_channel_transport can pass real creds to DiscordVoiceBridgeClient.
#[cfg(feature = "gateway-bridge")]
pub mod gateway_bridge;

/// WIT bindings for the WASM plugin (WASI targets only).
#[cfg(target_os = "wasi")]
mod wit_bindings;
/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

/// Per-trait `impl Trait for DiscordClient` blocks (SOLID B.1 split).
///
/// `lib.rs` keeps the struct + constructors + free functions + mappers;
/// each trait implementation lives in its own sibling file under `backend/`.
pub(crate) mod backend;

/// Return Fluent translations for the given locale.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

#[cfg(feature = "native")]
use futures::stream::{self, Stream};
#[cfg(feature = "native")]
use http::DiscordHttpClient;
#[cfg(feature = "native")]
use poly_client::{ChannelType, Channel, Message, ClientEvent, SettingsStorageCell, User, PresenceStatus, BackendType, MessageContent, ThreadMetadata, ThreadInfo, ForumTag};
#[cfg(feature = "gateway")]
use poly_client::{VoiceParticipant, ClientError};
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
#[cfg(feature = "gateway")]
use std::collections::HashMap;
#[cfg(feature = "gateway")]
use std::sync::Arc;
#[cfg(feature = "voice")]
use tokio::sync::Mutex as TokioMutex;
#[cfg(feature = "gateway")]
use tokio::sync::RwLock;
#[cfg(all(feature = "native", feature = "voice-bridge", target_arch = "wasm32"))]
use std::sync::Arc as VbArc;
#[cfg(all(feature = "native", feature = "gateway-bridge", target_arch = "wasm32"))]
use std::sync::Arc as GbArc;
#[cfg(all(feature = "gateway-bridge", target_arch = "wasm32"))]
use wasm_bindgen_futures;

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
    use super::{ChannelType, DiscordClient, Channel, api, Message, ClientEvent};
    use twilight_model::channel::ChannelType as DC;

    /// Map a raw Discord channel type integer to `poly_client::ChannelType`.
    #[must_use]
    pub const fn map_discord_channel_type(raw: u8) -> ChannelType {
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
    /// Phase D — token-bucket rate guard (D.2).
    rate_guard: guardrails::RateGuard,
    /// Phase D — per-channel slow-mode guard (D.5).
    slow_mode_guard: guardrails::SlowModeGuard,
    /// Phase D — permission pre-flight guard (D.4).
    permission_guard: guardrails::PermissionGuard,
    /// Phase D — per-channel typing fire-rate cap (D.6).
    typing_cap: guardrails::TypingRateCap,
    /// Phase D — soft-warning health surface (D.8).
    discord_health: Mutex<guardrails::DiscordHealth>,
    /// Phase E — cached Nitro tier for the authenticated account (E.3).
    account_info: Mutex<nitro::DiscordAccountInfo>,
    /// B.11 — per-account voice session guard.
    /// Holds `Some` while a voice WebSocket is open.
    /// A second `connect_voice` call returns `VoiceError::AlreadyConnected`
    /// without opening a second WS — the load-bearing anti-ban guardrail.
    #[cfg(feature = "voice")]
    pub voice_session: voice::VoiceSessionGuard,

    /// C.2 — gateway-tracked voice participant cache.
    ///
    /// Populated from `VOICE_STATE_UPDATE` gateway dispatches.
    /// `channel_id → Vec<VoiceParticipant>`.  Updated atomically by the
    /// gateway loop via `voice_states_tx`; read by `get_voice_participants`.
    #[cfg(feature = "gateway")]
    voice_states: Arc<RwLock<HashMap<String, Vec<VoiceParticipant>>>>,

    /// C.5 — channel to send raw JSON payloads on the active main gateway WS.
    ///
    /// `event_stream()` replaces this channel each time it reconnects.
    /// `set_self_mute` and `start_direct_call` write here to send op 4 / op 13
    /// without opening a second WS connection.
    #[cfg(feature = "gateway")]
    gateway_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,

    /// C.4 — clone of the active event_stream sender so voice speaking events
    /// can be injected from the voice WS loop without a second WS connection.
    ///
    /// Set by `event_stream()` when the gateway stream is opened. `None` when
    /// the gateway feature is disabled or before the first event_stream call.
    #[cfg(feature = "gateway")]
    gateway_event_tx: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<ClientEvent>>>>,

    /// WASM voice-bridge client (Option A).
    ///
    /// Present when `feature = "voice-bridge"` on `target_arch = "wasm32"`.
    /// Drives the Discord voice protocol over generic host-bridge primitives
    /// (`/host/udp/*`, `/host/codec/opus/*`, `/host/aead/*`) instead of the
    /// native tokio-tungstenite + audiopus path which cannot compile for wasm32.
    ///
    /// Initialized to `None`; populated lazily in `join_voice_channel_transport`
    /// using the account_id present at call time.
    #[cfg(all(feature = "native", feature = "voice-bridge", target_arch = "wasm32"))]
    pub voice_bridge_client: VbArc<tokio::sync::Mutex<Option<voice_bridge::DiscordVoiceBridgeClient>>>,

    /// WASM gateway-bridge outbound channel (wasm32 + gateway-bridge feature).
    ///
    /// Send half of a `tokio::sync::mpsc::unbounded_channel`. The background
    /// `gateway_bridge::run_loop` selects on this receiver and forwards any
    /// message over the browser WebSocket.  Used by `join_voice_channel_transport`
    /// to send op 4 Voice State Update without holding an `Rc` (which is !Send).
    ///
    /// `UnboundedSender<String>` is `Send + Sync` — safe to store on `DiscordClient`.
    #[cfg(all(feature = "native", feature = "gateway-bridge", target_arch = "wasm32"))]
    pub gateway_bridge_tx: GbArc<std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>>,

    /// Stashed voice credentials from the gateway-bridge loop.
    ///
    /// Populated asynchronously when the gateway receives `VOICE_STATE_UPDATE`
    /// (session_id) and `VOICE_SERVER_UPDATE` (endpoint + token).
    /// `join_voice_channel_transport` reads from here and passes real creds to
    /// `DiscordVoiceBridgeClient::connect_voice`.
    #[cfg(all(feature = "native", feature = "gateway-bridge", target_arch = "wasm32"))]
    pub voice_server_creds: gateway_bridge::CredsGuard,
}

/// Discord gateway permission-bit constants.
///
/// Centralised here so `get_my_permissions` and `get_server_roles` (and any
/// future callers) always agree on the same bit positions.  Adding a new
/// permission is a one-line edit in this module — no silent drift between two
/// copies (OCP / DRY).
mod permission_bits {
    pub const KICK_MEMBERS: i64 = 1 << 1;
    pub const BAN_MEMBERS: i64 = 1 << 2;
    pub const ADMINISTRATOR: i64 = 1 << 3;
    pub const MANAGE_CHANNELS: i64 = 1 << 4;
    pub const MANAGE_GUILD: i64 = 1 << 5;
    pub const MANAGE_MESSAGES: i64 = 1 << 13;
    pub const MANAGE_ROLES: i64 = 1 << 28;
    pub const MODERATE_MEMBERS: i64 = 1 << 40;
}

#[cfg(feature = "native")]
impl DiscordClient {
    /// Private constructor that initialises every field.
    ///
    /// All public constructors are thin wrappers around this one so that
    /// adding a new field only requires a single edit here (SRP/DRY).
    fn build(base_url: String, gateway_url: Option<String>) -> Self {
        Self {
            http: DiscordHttpClient::new(base_url),
            account_id: None,
            account_display_name: None,
            settings_storage: SettingsStorageCell::new(),
            menu_state: Mutex::new(DiscordMenuState::default()),
            gateway_url,
            version_override: Mutex::new(None),
            rate_guard: guardrails::RateGuard::new(),
            slow_mode_guard: guardrails::SlowModeGuard::new(),
            permission_guard: guardrails::PermissionGuard::new(),
            typing_cap: guardrails::TypingRateCap::new(),
            discord_health: Mutex::new(guardrails::DiscordHealth::default()),
            account_info: Mutex::new(nitro::DiscordAccountInfo::default()),
            #[cfg(feature = "voice")]
            voice_session: Arc::new(TokioMutex::new(None)),
            #[cfg(feature = "gateway")]
            voice_states: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "gateway")]
            gateway_tx: Arc::new(Mutex::new(None)),
            #[cfg(feature = "gateway")]
            gateway_event_tx: Arc::new(Mutex::new(None)),
            #[cfg(all(feature = "native", feature = "voice-bridge", target_arch = "wasm32"))]
            voice_bridge_client: VbArc::new(tokio::sync::Mutex::new(None)),
            #[cfg(all(feature = "native", feature = "gateway-bridge", target_arch = "wasm32"))]
            gateway_bridge_tx: GbArc::new(std::sync::Mutex::new(None)),
            #[cfg(all(feature = "native", feature = "gateway-bridge", target_arch = "wasm32"))]
            voice_server_creds: GbArc::new(tokio::sync::Mutex::new(gateway_bridge::VoiceServerCreds::default())),
        }
    }

    #[must_use]
    pub fn new() -> Self {
        DiscordClientBuilder::new().build()
    }

    #[must_use]
    pub fn with_base_url(base_url: String) -> Self {
        DiscordClientBuilder::new().base_url(base_url).build()
    }

    /// Create a client with a REST base URL and a WS gateway URL.
    ///
    /// `gateway_ws_url` is the WebSocket URL the client will connect to in
    /// `event_stream()`.  Example: `"ws://127.0.0.1:9999/gateway/ws"`.
    #[must_use]
    pub fn with_base_url_and_gateway(base_url: String, gateway_ws_url: String) -> Self {
        DiscordClientBuilder::new()
            .base_url(base_url)
            .gateway_url(gateway_ws_url)
            .build()
    }

    fn account_id(&self) -> String {
        self.account_id.clone().unwrap_or_default()
    }

    fn account_display_name(&self) -> String {
        self.account_display_name.clone().unwrap_or_default()
    }

    /// Format the CDN icon and banner URLs for a Discord guild.
    ///
    /// Both `get_servers` and `get_server` need the same `format!` chains;
    /// centralising here eliminates silent drift (DRY/SRP).
    fn guild_image_urls(
        guild_id: &str,
        icon: Option<&str>,
        banner: Option<&str>,
        cdn_base: &str,
    ) -> (Option<String>, Option<String>) {
        let base = cdn_base.trim_end_matches('/');
        let icon_url = icon.map(|hash| format!("{base}/icons/{guild_id}/{hash}.png?size=128"));
        let banner_url = banner.map(|hash| format!("{base}/banners/{guild_id}/{hash}.png"));
        (icon_url, banner_url)
    }

    /// D.8 — Return a snapshot of the current backend health surface.
    ///
    /// Intended for the "Backend health" panel in Settings → Discord.
    /// The caller may also update the health from the rate guard at read time.
    pub fn discord_health_snapshot(&self) -> guardrails::DiscordHealth {
        let mut h = self
            .discord_health
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        h.update_from_rate_guard(&self.rate_guard);
        h
    }

    /// F.1 — Return a snapshot of all telemetry counters for the "Backend health" panel.
    ///
    /// Counters are monotonically increasing since backend init.  The snapshot
    /// can be polled by the UI on a timer or exposed via `DiscordHealth`.
    /// Grep `discord-anti-ban` in logs to correlate events with these counts.
    pub fn guardrail_stats(&self) -> guardrails::GuardrailStats {
        self.http.counters.snapshot()
    }

    /// E.3 — Return the cached Nitro tier for the authenticated account.
    pub fn nitro_tier(&self) -> nitro::NitroTier {
        self.account_info
            .lock()
            .map(|info| info.tier())
            .unwrap_or(nitro::NitroTier::None)
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
    const fn map_channel_type(dc: twilight_model::channel::ChannelType) -> ChannelType {
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
        let _ = &self; // required by method signature but body uses only associated functions
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
    // one flat match mapping each Discord gateway event name to its
    // ClientEvent(s); a single dispatch table reads clearer than fragmenting
    // the 1:1 event-name→event mapping across helpers.
    // lint-allow-unused: flat gateway-event dispatch table, intentionally one fn
    #[allow(clippy::too_many_lines)]
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
                let status: poly_client::PresenceStatus = match status_str {
                    "online" => poly_client::PresenceStatus::Online,
                    "idle" => poly_client::PresenceStatus::Idle,
                    "dnd" => poly_client::PresenceStatus::DoNotDisturb,
                    _ => poly_client::PresenceStatus::Offline,
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
/// Phase C/D additions:
/// - `voice_states` — shared cache of `channel_id → VoiceParticipant` list,
///   updated from `VOICE_STATE_UPDATE` dispatches.
/// - `local_user_id` — used to detect when the local user is in a CALL_CREATE
///   ringing list (D.3).
/// - `gw_rx` — receives raw JSON strings to forward on the WS (C.5, D.2).
///
/// Protocol decisions (Phase 6.5 + Phase C):
/// - Sends an op-2 IDENTIFY using the same `SuperProperties` as HTTP so the
///   gateway fingerprint is consistent with the `X-Super-Properties` header
///   (Phase C.2 — eliminates the HTTP/WS mismatch ban signal).
/// - Responds to HEARTBEAT_ACK (op 11) silently.
/// - Does NOT implement reconnect logic — stream simply ends on disconnect.
#[cfg(feature = "gateway")]
async fn gateway_connect_loop(
    gateway_url: String,
    super_props: crate::super_properties::SuperProperties,
    tx: UnboundedSender<ClientEvent>,
    voice_states: Arc<RwLock<HashMap<String, Vec<VoiceParticipant>>>>,
    local_user_id: String,
    mut gw_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
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

    // Send IDENTIFY (op 2) using the same SuperProperties as HTTP (Phase C.2).
    // The `properties` field is the raw JSON object — no base64 wrapping on WS.
    let identify_properties = super_props.to_identify_properties();
    let identify = serde_json::json!({
        "op": 2_i32,
        "d": {
            "token": "",
            "intents": 513_i32,
            "properties": identify_properties,
            "compress": false,
            "large_threshold": 250
        }
    });
    tracing::debug!(
        target: "poly_discord::gateway",
        build_number = super_props.client_build_number,
        os = %super_props.os,
        "sending gateway IDENTIFY"
    );
    use futures::SinkExt;
    if let Err(e) = write.send(TungsteniteMsg::Text(identify.to_string().into())).await {
        tracing::warn!(target: "poly_discord::gateway", error = %e, "failed to send IDENTIFY");
        return;
    }

    // The client that owns this stream has `&self` access; use a stub for parsing.
    let parser = DiscordClient::new();

    loop {
        tokio::select! {
            // Outbound: C.5 / D.2 — forward raw JSON from set_self_mute / start_direct_call.
            Some(raw) = gw_rx.recv() => {
                let _ = write.send(TungsteniteMsg::Text(raw.into())).await;
            }
            // Inbound: gateway frames.
            msg_result = read.next() => {
                let Some(msg_result) = msg_result else { break };
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

                    // C.3 — VOICE_STATE_UPDATE: update per-channel participant cache
                    // and emit VoiceUserJoined / VoiceUserLeft / VoiceStateUpdated.
                    if event_name == "VOICE_STATE_UPDATE" {
                        let voice_events = handle_voice_state_update(
                            &data,
                            &voice_states,
                        ).await;
                        for ev in voice_events {
                            if tx.send(ev).is_err() { return; }
                        }
                        // Also let parse_gateway_event handle any additional mapping.
                    }

                    // D.1 — CALL_CREATE: incoming DM call ringing.
                    if event_name == "CALL_CREATE" {
                        let channel_id = data
                            .get("channel_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        let ringing: Vec<String> = data
                            .get("ringing")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        // Determine caller: first voice_state that isn't the local user.
                        let caller_user_id = data
                            .get("voice_states")
                            .and_then(|vs| vs.as_array())
                            .and_then(|arr| {
                                arr.iter().find_map(|vs| {
                                    let uid = vs
                                        .get("user_id")
                                        .and_then(|v| v.as_str())?;
                                    if uid != local_user_id { Some(uid.to_string()) } else { None }
                                })
                            })
                            .unwrap_or_default();

                        // Only emit IncomingCall if the local user is in the ringing list.
                        if ringing.contains(&local_user_id) && !channel_id.is_empty() && !caller_user_id.is_empty() {
                            let ev = ClientEvent::IncomingCall {
                                dm_id: channel_id,
                                caller_user_id,
                                with_video: false,
                            };
                            if tx.send(ev).is_err() { return; }
                        }
                    }

                    // Existing parse_gateway_event for all other events.
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
    }
}

/// C.3 — handle a `VOICE_STATE_UPDATE` gateway dispatch.
///
/// Updates the shared `voice_states` cache and returns the `ClientEvent`s
/// to emit (`VoiceUserJoined`, `VoiceUserLeft`, or `VoiceStateUpdated`).
/// Uses `BatchedSignal::set_if_changed` semantics: only emits if the participant
/// list actually changed (hang class #8 mitigation via the caller's
/// `set_if_changed` in the UI consumer).
#[cfg(feature = "gateway")]
async fn handle_voice_state_update(
    data: &serde_json::Value,
    voice_states: &Arc<RwLock<HashMap<String, Vec<VoiceParticipant>>>>,
) -> Vec<ClientEvent> {
    let channel_id = data
        .get("channel_id")
        .and_then(|v| if v.is_null() { None } else { v.as_str() })
        .map(|s| s.to_string());
    let user_id = data
        .get("user_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let is_muted = data.get("self_mute").and_then(|v| v.as_bool()).unwrap_or(false);
    let is_deafened = data.get("self_deaf").and_then(|v| v.as_bool()).unwrap_or(false);

    if user_id.is_empty() {
        return vec![];
    }

    let mut states = voice_states.write().await;

    // User left all voice channels (channel_id is null).
    if channel_id.is_none() {
        // Remove participant from any channel they were in.
        let mut events = vec![];
        for (ch_id, participants) in states.iter_mut() {
            if let Some(pos) = participants.iter().position(|p| p.user.id == user_id) {
                participants.remove(pos);
                events.push(ClientEvent::VoiceUserLeft {
                    channel_id: ch_id.clone(),
                    user_id: user_id.clone(),
                });
            }
        }
        return events;
    }

    let channel_id = channel_id.unwrap();

    // Check if the user is already in this channel (state update vs join).
    let participants = states.entry(channel_id.clone()).or_default();
    if let Some(participant) = participants.iter_mut().find(|p| p.user.id == user_id) {
        // Existing participant — state update.
        participant.is_muted = is_muted;
        participant.is_deafened = is_deafened;
        let updated = participant.clone();
        vec![ClientEvent::VoiceStateUpdated {
            channel_id,
            participant: updated,
        }]
    } else {
        // New participant joining the channel.
        let participant = VoiceParticipant {
            user: poly_client::User {
                id: user_id,
                display_name: data
                    .get("member")
                    .and_then(|m| m.get("user"))
                    .and_then(|u| u.get("username"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                avatar_url: None,
                presence: poly_client::PresenceStatus::Online,
                backend: poly_client::BackendType::from(crate::SLUG),
            },
            is_muted,
            is_deafened,
            is_streaming: false,
            is_video_on: false,
            is_speaking: false,
        };
        participants.push(participant.clone());
        vec![ClientEvent::VoiceUserJoined {
            channel_id,
            participant,
        }]
    }
}

// ── B.2 / B.10 — Voice connect / disconnect (native + gateway + voice) ────────

/// Voice gateway orchestration methods for `DiscordClient`.
///
/// Sends op 4 Voice State Update on the main gateway WS, waits for
/// `VOICE_STATE_UPDATE` and `VOICE_SERVER_UPDATE`, then delegates to the
/// `voice` module for the full UDP/Opus/AEAD pipeline.
#[cfg(feature = "voice")]
impl DiscordClient {
    /// Join a voice channel.
    ///
    /// Sends op 4 Voice State Update on the main gateway, collects the
    /// `session_id` (from `VOICE_STATE_UPDATE`) and `endpoint`/`token`
    /// (from `VOICE_SERVER_UPDATE`), then connects the voice WebSocket
    /// and UDP transport (Phase B.3–B.9).
    ///
    /// `guild_id` is the server's ID; `channel_id` is the voice channel.
    /// Pass `audio` from the shell's active `AudioBackend` instance.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::AlreadyConnected` if this account already has
    /// an open voice connection (B.11 anti-ban guardrail).
    pub async fn connect_voice(
        &self,
        guild_id: &str,
        channel_id: &str,
        audio: &dyn poly_audio_backend::AudioBackend,
        transmit_mode: Option<voice::TransmitMode>,
    ) -> Result<(), voice::VoiceError> {
        // B.11 — reject early if already connected.
        {
            let session = self.voice_session.lock().await;
            if session.is_some() {
                return Err(voice::VoiceError::AlreadyConnected);
            }
        }

        let Some(ref gateway_url) = self.gateway_url else {
            return Err(voice::VoiceError::WsConnect("no gateway URL configured".into()));
        };

        // Build Voice State Update op 4.
        let vsu = voice::voice_state_update_payload(guild_id, Some(channel_id), false, false);

        // Open a dedicated gateway WS just to send op 4 and collect the two
        // voice events.  The main gateway WS is already in gateway_connect_loop
        // (spawned in event_stream). We use a fresh connection here so we don't
        // race with the main stream's parser.
        let (ws_stream, _) = tokio_tungstenite::connect_async(gateway_url.as_str())
            .await
            .map_err(|e| voice::VoiceError::WsConnect(e.to_string()))?;

        let (mut ws_write, mut ws_read) = futures::StreamExt::split(ws_stream);

        // Send IDENTIFY first (required before any other op).
        let props = self.http.super_properties();
        let identify_props = props.to_identify_properties();
        let identify = serde_json::json!({
            "op": 2,
            "d": {
                "token": self.http.token().unwrap_or_default(),
                "intents": 513,
                "properties": identify_props,
                "compress": false,
            }
        });
        futures::SinkExt::send(&mut ws_write, tokio_tungstenite::tungstenite::Message::Text(
            identify.to_string().into()
        ))
        .await
        .map_err(|e| voice::VoiceError::WsConnect(e.to_string()))?;

        // Send op 4 Voice State Update.
        futures::SinkExt::send(&mut ws_write, tokio_tungstenite::tungstenite::Message::Text(
            vsu.into()
        ))
        .await
        .map_err(|e| voice::VoiceError::WsConnect(e.to_string()))?;

        // Collect VOICE_STATE_UPDATE (session_id) and VOICE_SERVER_UPDATE (endpoint/token).
        let mut session_id: Option<String> = None;
        let mut endpoint: Option<String> = None;
        let mut voice_token: Option<String> = None;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);

        loop {
            if tokio::time::Instant::now() > deadline {
                return Err(voice::VoiceError::VoiceStateMissing);
            }

            let msg = tokio::time::timeout(
                std::time::Duration::from_secs(10),
                futures::StreamExt::next(&mut ws_read),
            )
            .await
            .map_err(|_| voice::VoiceError::VoiceStateMissing)?;

            let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) = msg else {
                continue;
            };

            let frame: serde_json::Value = match serde_json::from_str(text.as_str()) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let op = frame.get("op").and_then(|o| o.as_u64()).unwrap_or(99);
            let data = frame.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let event_name = frame.get("t").and_then(|v| v.as_str()).unwrap_or("");

            match (op, event_name) {
                (0, "VOICE_STATE_UPDATE") => {
                    let sid = data
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !sid.is_empty() {
                        session_id = Some(sid);
                        tracing::debug!(target: "poly_discord::voice", "received VOICE_STATE_UPDATE");
                    }
                }
                (0, "VOICE_SERVER_UPDATE") => {
                    endpoint = data
                        .get("endpoint")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    voice_token = data
                        .get("token")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    tracing::debug!(target: "poly_discord::voice", ?endpoint, "received VOICE_SERVER_UPDATE");
                }
                _ => {}
            }

            if session_id.is_some() && endpoint.is_some() && voice_token.is_some() {
                break;
            }
        }

        let info = voice::VoiceServerInfo {
            endpoint: endpoint.ok_or(voice::VoiceError::VoiceStateMissing)?,
            token: voice_token.ok_or(voice::VoiceError::VoiceStateMissing)?,
            session_id: session_id.ok_or(voice::VoiceError::VoiceStateMissing)?,
            guild_id: Some(guild_id.to_string()),
            user_id: self.account_id(),
        };

        // C.4 — wire speaking events through the existing gateway event sender if available.
        #[cfg(feature = "gateway")]
        let speaking_tx = self.gateway_event_tx
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .map(|tx| (channel_id.to_string(), tx));
        #[cfg(not(feature = "gateway"))]
        let speaking_tx: Option<(String, tokio::sync::mpsc::UnboundedSender<ClientEvent>)> = None;
        voice::connect_voice(info, audio, transmit_mode, Arc::clone(&self.voice_session), speaking_tx).await
    }

    /// Leave the currently-joined voice channel (B.10).
    ///
    /// Sends op 4 Voice State Update with `channel_id: null` on the main
    /// gateway, closes the voice WS, and releases the audio streams.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::WsConnect` if the gateway WS cannot be reached.
    pub async fn disconnect_voice(
        &self,
        guild_id: &str,
    ) -> Result<(), voice::VoiceError> {
        // Send op 4 with channel_id = null to tell Discord we left.
        if let Some(ref gateway_url) = self.gateway_url {
            let vsu = voice::voice_state_update_payload(guild_id, None, false, false);
            // Best-effort: if we can't open the WS, we still release the local session.
            if let Ok((ws_stream, _)) = tokio_tungstenite::connect_async(gateway_url.as_str()).await {
                let (mut write, _) = futures::StreamExt::split(ws_stream);
                let _ = futures::SinkExt::send(
                    &mut write,
                    tokio_tungstenite::tungstenite::Message::Text(vsu.into()),
                )
                .await;
            }
        }

        // Drop the voice session — this signals the encode/decode/WS tasks.
        let mut session = self.voice_session.lock().await;
        if let Some(conn) = session.take() {
            conn.disconnect().await;
        }
        Ok(())
    }
}

// ── Phase E — Video transport (start/stop camera + screen share) ─────────────

#[cfg(feature = "voice")]
impl DiscordClient {
    /// Start sending local camera video on the active voice connection.
    ///
    /// Creates a `DiscordVideoTransport` and sends op 12 Video + op 14 Client
    /// Connect on the voice WS. Frames from `frame_rx` are encoded via the
    /// host-bridge H.264 encoder and sent on the shared UDP socket.
    ///
    /// `bridge_base_url` should be the host-bridge base URL, e.g. `"http://127.0.0.1:9333"`.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::AlreadyConnected`-equivalent if not in a voice call,
    /// or `VideoTransportError` on transport failures.
    pub async fn start_video(
        &self,
        frame_rx: tokio::sync::mpsc::Receiver<poly_video_backend::types::VideoFrame>,
        bridge_base_url: String,
    ) -> Result<(), voice::video::VideoTransportError> {
        let mut session = self.voice_session.lock().await;
        let conn = session.as_mut().ok_or(voice::video::VideoTransportError::WsChannelClosed)?;

        let transport = voice::video::DiscordVideoTransport::start(
            conn.local_ssrc,
            false, // camera
            std::sync::Arc::clone(&conn.udp),
            conn.secret_key,
            conn.encryption_mode.clone(),
            conn.ws_out_tx.clone(),
            bridge_base_url,
            frame_rx,
        )
        .await?;

        conn.video_transport = Some(transport);
        Ok(())
    }

    /// Stop sending camera video. Sends op 12 with empty streams to Discord.
    pub async fn stop_video(&self) {
        let mut session = self.voice_session.lock().await;
        if let Some(conn) = session.as_mut() {
            if let Some(transport) = conn.video_transport.take() {
                transport.stop(&conn.ws_out_tx).await;
            }
        }
    }

    /// Start sending local screen share on the active voice connection.
    ///
    /// Uses a separate SSRC (audio_ssrc + 2) for screen-share-as-second-stream.
    /// Discord treats camera and screen share as separate video streams.
    pub async fn start_screen_share(
        &self,
        frame_rx: tokio::sync::mpsc::Receiver<poly_video_backend::types::VideoFrame>,
        bridge_base_url: String,
    ) -> Result<(), voice::video::VideoTransportError> {
        let mut session = self.voice_session.lock().await;
        let conn = session.as_mut().ok_or(voice::video::VideoTransportError::WsChannelClosed)?;

        // Screen share uses a different SSRC offset than camera.
        // We temporarily adjust the audio_ssrc by +1 so video_ssrc = audio_ssrc + 2.
        let screen_audio_ssrc = conn.local_ssrc + 1;
        let transport = voice::video::DiscordVideoTransport::start(
            screen_audio_ssrc,
            true, // screen share
            std::sync::Arc::clone(&conn.udp),
            conn.secret_key,
            conn.encryption_mode.clone(),
            conn.ws_out_tx.clone(),
            bridge_base_url,
            frame_rx,
        )
        .await?;

        conn.video_transport = Some(transport);
        Ok(())
    }

    /// Stop sending screen share.
    pub async fn stop_screen_share(&self) {
        self.stop_video().await
    }
}

// ── C.5 / D.2 / D.4 — Gateway control methods ────────────────────────────────
//
// These methods send raw JSON on the main gateway WS back-channel (gateway_tx).
// They are gated on `gateway` (not `voice`) because they only need the WS
// connection, not the UDP/Opus/AEAD pipeline.

#[cfg(feature = "gateway")]
impl DiscordClient {
    /// C.5 — Toggle microphone mute / deafen state.
    ///
    /// Sends op 4 Voice State Update on the main gateway with the new flags.
    /// `guild_id` is the server the user is currently connected to; passing
    /// an empty string keeps the existing guild context.
    ///
    /// Errors if the gateway back-channel is not open (event_stream not called).
    pub fn set_self_mute(
        &self,
        guild_id: &str,
        channel_id: Option<&str>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), ClientError> {
        let payload = serde_json::json!({
            "op": 4,
            "d": {
                "guild_id": guild_id,
                "channel_id": channel_id,
                "self_mute": self_mute,
                "self_deaf": self_deaf,
            }
        });
        self.send_gateway_payload(payload.to_string())
    }

    /// D.2 — Initiate a DM call (op 13 Call Connect).
    ///
    /// Sends op 13 on the main gateway WS with the channel ID.
    /// Discord will respond with `VOICE_SERVER_UPDATE` which the gateway
    /// loop forwards to the UI as a voice-join flow.
    ///
    /// Errors if the gateway back-channel is not open (event_stream not called).
    pub fn start_direct_call(&self, dm_channel_id: &str) -> Result<(), ClientError> {
        // D.7 — include `ringing` list with the partner's user ID when available.
        let payload = serde_json::json!({
            "op": 13,
            "d": {
                "channel_id": dm_channel_id,
            }
        });
        self.send_gateway_payload(payload.to_string())
    }

    /// D.4 — Decline / cancel a DM call.
    ///
    /// Sends op 13 with `channel_id = null` (cancels ringing) and calls
    /// `POST /channels/{dm_channel_id}/call/ring/stop` via REST.
    ///
    /// Errors if the gateway back-channel is not open or the REST call fails.
    pub async fn decline_direct_call(
        &self,
        dm_channel_id: &str,
    ) -> Result<(), ClientError> {
        // Cancel ringing on the gateway side.
        let stop_ringing = serde_json::json!({
            "op": 13,
            "d": {
                "channel_id": serde_json::Value::Null,
            }
        });
        let _ = self.send_gateway_payload(stop_ringing.to_string());

        // REST call to stop the ring.
        self.http
            .post_empty(&format!("/api/v10/channels/{dm_channel_id}/call/ring/stop"))
            .await
            .map_err(|e| ClientError::Internal(format!("ring/stop failed: {e}")))?;
        Ok(())
    }

    /// Internal: push a raw JSON string onto the gateway WS back-channel.
    fn send_gateway_payload(&self, payload: String) -> Result<(), ClientError> {
        let guard = self
            .gateway_tx
            .lock()
            .map_err(|_| ClientError::Internal("gateway_tx mutex poisoned".into()))?;
        match guard.as_ref() {
            Some(tx) => tx
                .send(payload)
                .map_err(|_| ClientError::Internal("gateway back-channel closed".into())),
            None => Err(ClientError::Internal(
                "gateway back-channel not open — call event_stream() first".into(),
            )),
        }
    }
}

#[cfg(feature = "native")]
impl Default for DiscordClient {
    fn default() -> Self { Self::new() }
}

/// Builder for `DiscordClient`. (SOLID B.5 — OCP win: future config knobs
/// add a `.with_x(_)` method instead of forking the constructor.)
///
/// Example:
/// ```ignore
/// let client = DiscordClientBuilder::new()
///     .base_url("https://discord.com".to_string())
///     .gateway_url("wss://gateway.discord.gg".to_string())
///     .build();
/// ```
#[cfg(feature = "native")]
#[derive(Default)]
pub struct DiscordClientBuilder {
    base_url: Option<String>,
    gateway_url: Option<String>,
}

#[cfg(feature = "native")]
impl DiscordClientBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the REST base URL.  Defaults to `https://discord.com`.
    #[must_use]
    pub fn base_url(mut self, base_url: String) -> Self {
        self.base_url = Some(base_url);
        self
    }

    /// Override the gateway WS URL.  When unset, `event_stream()` returns
    /// an empty stream (no real-time events).
    #[must_use]
    pub fn gateway_url(mut self, gateway_url: String) -> Self {
        self.gateway_url = Some(gateway_url);
        self
    }

    #[must_use]
    pub fn build(self) -> DiscordClient {
        DiscordClient::build(
            self.base_url.unwrap_or_else(|| "https://discord.com".to_string()),
            self.gateway_url,
        )
    }
}

// ── Gateway-bridge helpers ────────────────────────────────────────────────────

#[cfg(all(feature = "native", feature = "gateway-bridge", target_arch = "wasm32"))]
impl DiscordClient {
    /// Wait up to `max_ms` milliseconds for all three voice credentials
    /// (`endpoint`, `token`, `session_id`) to be populated by the gateway-bridge loop.
    ///
    /// The gateway-bridge stashes credentials asynchronously when `VOICE_STATE_UPDATE`
    /// (→ `session_id`) and `VOICE_SERVER_UPDATE` (→ `endpoint` + `token`) arrive,
    /// typically 3–50 ms after op 4 is sent. A single-shot read races against that
    /// arrival; this helper polls with 25 ms steps until all three fields are non-empty
    /// or the deadline expires.
    ///
    /// Returns `Some((endpoint, token, session_id))` on success, `None` on timeout.
    async fn wait_for_voice_creds(&self, max_ms: u64) -> Option<(String, String, String)> {
        let steps = (max_ms / 25).max(1);
        for _ in 0..steps {
            {
                let creds = self.voice_server_creds.lock().await;
                if creds.is_complete() {
                    return Some((
                        creds.endpoint.clone().unwrap_or_default(),
                        creds.token.clone().unwrap_or_default(),
                        creds.session_id.clone().unwrap_or_default(),
                    ));
                }
            }
            gloo_timers::future::TimeoutFuture::new(25).await;
        }
        // One final check after the last sleep.
        let creds = self.voice_server_creds.lock().await;
        if creds.is_complete() {
            Some((
                creds.endpoint.clone().unwrap_or_default(),
                creds.token.clone().unwrap_or_default(),
                creds.session_id.clone().unwrap_or_default(),
            ))
        } else {
            None
        }
    }
}


// ── H.2.b — ForumBackend ─────────────────────────────────────────────────────


// ── H.2.c — ThreadsBackend ───────────────────────────────────────────────────


// ── H.3.a — ModerationBackend ────────────────────────────────────────────────


// ── H.3.b — SocialGraphBackend ───────────────────────────────────────────────

// Discord supports DM channels, group DMs, and lifecycle management.
// Mute/unmute require guild context and are not yet implemented.


// ── H.4.a — MessagingBackend ─────────────────────────────────────────────────


// ── H.4.b — ServerAdminBackend ───────────────────────────────────────────────


// ── D.5 — Discord mechanism declaration unit tests ────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod mechanism_tests {
    use super::DiscordClient;
    // `client_mechanisms` / `set_client_mechanism` are provided by the
    // `IsBackend` trait impl in `backend/is_backend.rs`; bring it into scope.
    use poly_client::{HostCap, IsBackend};

    /// Verify that Discord declares a `captcha-sandbox` mechanism that
    /// requires `HostCap::SandboxBrowser`. This is the Phase D.1 contract:
    /// the mechanism must exist in the list so the `MechanismToggle` UI
    /// renders it (disabled on shells that don't advertise the cap).
    #[tokio::test]
    async fn discord_declares_captcha_sandbox_mechanism() {
        let client = DiscordClient::new();
        let mechs = client.client_mechanisms().await.unwrap();

        let captcha = mechs
            .iter()
            .find(|m| m.id == "captcha-sandbox")
            .expect("captcha-sandbox mechanism must be declared");

        assert_eq!(
            captcha.requires_host_cap,
            Some(HostCap::SandboxBrowser),
            "captcha-sandbox must require SandboxBrowser host cap"
        );
        // Default state: disabled (user must opt in to sandbox mode).
        assert!(!captcha.enabled, "captcha-sandbox should default to disabled");
        assert!(
            !captcha.name_key.is_empty(),
            "name_key must be non-empty FTL key"
        );
        assert!(
            captcha.description_key.is_some(),
            "captcha-sandbox should have a description key"
        );
    }

    /// Verify that Discord also declares `super-properties` with no host cap
    /// requirement (it must always be toggleable).
    #[tokio::test]
    async fn discord_declares_super_properties_mechanism() {
        let client = DiscordClient::new();
        let mechs = client.client_mechanisms().await.unwrap();

        let sp = mechs
            .iter()
            .find(|m| m.id == "super-properties")
            .expect("super-properties mechanism must be declared");

        assert_eq!(
            sp.requires_host_cap, None,
            "super-properties must not require a host cap"
        );
        // Default: enabled (disabling it breaks Discord login).
        assert!(sp.enabled, "super-properties should default to enabled");
    }

    /// Verify that `set_client_mechanism` accepts valid mechanism IDs and
    /// rejects unknown ones.
    #[tokio::test]
    async fn discord_set_mechanism_rejects_unknown_ids() {
        let client = DiscordClient::new();
        let result = client.set_client_mechanism("not-a-real-mechanism", true).await;
        assert!(
            result.is_err(),
            "set_client_mechanism should return Err for unknown mechanism IDs"
        );
    }
}

// ── C.1 — VoiceTransportBackend ──────────────────────────────────────────────

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

// ── C.1 — ViewDescriptorBackend ──────────────────────────────────────────────

// ── C.1 — ContextActionBackend ───────────────────────────────────────────────
