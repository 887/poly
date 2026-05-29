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
//! Implements [`poly_client::IsBackend`] using the Stoat REST and
//! WebSocket APIs from `developers.stoat.chat`.
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Implements `ClientBackend` directly.
//! - **WASM plugin** (target `wasm32-wasip2`): Exports WIT `messenger-client`.
//!
//! DECISION(D21): WASM Plugin Backends.
//!
//! ## Module layout (SOLID-audit-stoat D.2)
//!
//! The capability-trait impls (`IsBackend`, `MessagingBackend`,
//! `ModerationBackend`, `SocialGraphBackend`, `DmsAndGroupsBackend`,
//! `ServerAdminBackend`, `VoiceTransportBackend`, `SettingsBackend`,
//! `ViewDescriptorBackend`, `ContextActionBackend`) each live in their
//! own sibling module:
//!
//! - [`is_backend`] — primary `IsBackend` impl + Bonfire WS parser
//! - [`server_admin`] — `ServerAdminBackend`
//! - [`moderation`] — `ModerationBackend`
//! - [`social_graph`] — `SocialGraphBackend`
//! - [`dms_and_groups`] — `DmsAndGroupsBackend`
//! - [`messaging`] — `MessagingBackend`
//! - [`voice_transport`] — `VoiceTransportBackend`
//! - [`settings`] — `SettingsBackend`
//! - [`view_descriptor`] — `ViewDescriptorBackend`
//! - [`context_action`] — `ContextActionBackend`

/// The backend slug used in all [`poly_client::BackendType`] constructions for this crate.
pub const SLUG: &str = "stoat";

#[cfg(feature = "native")]
mod api;

#[cfg(feature = "native")]
mod config;

#[cfg(feature = "native")]
mod http;

#[cfg(feature = "native")]
pub mod signup;

// ── SOLID-audit-stoat D.2 — capability-trait modules ─────────────────────────
#[cfg(feature = "native")]
mod context_action;
#[cfg(feature = "native")]
mod dms_and_groups;
#[cfg(feature = "native")]
mod is_backend;
#[cfg(feature = "native")]
mod messaging;
#[cfg(feature = "native")]
mod moderation;
#[cfg(feature = "native")]
mod server_admin;
#[cfg(feature = "native")]
mod settings;
#[cfg(feature = "native")]
mod social_graph;
#[cfg(feature = "native")]
mod view_descriptor;
#[cfg(feature = "native")]
mod voice_transport;

/// Stoat voice transport (Vortex WS + Opus encode/decode).
/// NATIVE ONLY — WASM builds MUST NOT enable the `voice` feature.
/// Phase F of plan-voice-video-calls.md.
#[cfg(feature = "voice")]
pub mod voice;

/// Shared voice constants, types, and utilities — cfg-free, compiles on both
/// native and wasm32. Both `voice.rs` (native) and `voice_wasm.rs` (WASM, Phase B)
/// import from here. See `docs/plans/plan-stoat-voice-wasm.md` Phase B serial prep.
pub mod voice_common;

/// Shared video constants and codec-layer helpers (RFC 6184 FU-A frag/reassembly,
/// NAL parsing) — cfg-free, compiles on both native and wasm32. Ported from
/// `clients/discord/src/voice_bridge/video_{capture,playback}.rs` so the
/// codec layer is ready when a Stoat video transport is chosen (Vortex-extension
/// vs LiveKit-SFU). See `docs/plans/plan-stoat-video-wasm.md` for the open
/// transport question.
pub(crate) mod video_common;

/// RNNoise-based noise-cancellation filter for the Stoat voice pipeline (B.8).
///
/// cfg-free — compiles on both native and wasm32. The nnnoiseless crate is pure
/// Rust and has no C FFI dependencies. Used by `voice_wasm_audio_capture` on
/// wasm32 and exposed via `StoatClient::set_noise_cancel` for runtime toggling.
pub(crate) mod voice_noise_filter;

/// Stoat voice transport — WASM target (Phase B of `plan-stoat-voice-wasm.md`).
/// Sibling to `voice.rs`; uses `gloo_net` WS + `/host/codec/opus/*` instead of
/// `tokio_tungstenite` + `audiopus`.
#[cfg(target_arch = "wasm32")]
pub(crate) mod voice_wasm;

/// Stoat WASM mic capture (Phase B.3). Stub until B.3 agent lands the real
/// `MediaStreamTrackProcessor` implementation.
#[cfg(target_arch = "wasm32")]
pub(crate) mod voice_wasm_audio_capture;

/// Stoat WASM speaker playback (Phase B.4). Stub until B.4 agent lands the real
/// `AudioContext` + `AudioBufferSourceNode` implementation.
#[cfg(target_arch = "wasm32")]
pub(crate) mod voice_wasm_audio_playback;

/// Stoat WASM video capture (Phase B.3 of `plan-stoat-video-wasm.md`).
/// Camera → WebCodecs H.264 encoder → FU-A fragmentation → Vortex WS with
/// `FrameKind::Video` discriminator. Shares the WS opened by `voice_wasm`.
#[cfg(target_arch = "wasm32")]
pub(crate) mod video_wasm_capture;

/// Stoat WASM video playback (Phase B.4 of `plan-stoat-video-wasm.md`).
/// Per-user FU-A reassembly → WebCodecs H.264 decoder → canvas draw.
/// Receives frames dispatched by `voice_wasm` after kind-byte routing.
#[cfg(target_arch = "wasm32")]
pub(crate) mod video_wasm_playback;

/// Stoat WASM video transport surface (Phase B.5 of `plan-stoat-video-wasm.md`).
///
/// Exposes `start_video_capture` and `stop_video_capture` as inherent methods on
/// [`StoatClient`] so the UI can drive the camera without reaching into
/// `video_wasm_capture` directly. On WASM the methods call into
/// [`video_wasm_capture::start_video_capture`], borrowing the shared WS sender
/// from the live [`voice_wasm::StoatVoiceConnection`]. The returned
/// [`video_wasm_capture::StoatVideoCaptureHandle`] is stored in `self.video_wasm_conn`
/// so the camera stays open for the session lifetime.
#[cfg(target_arch = "wasm32")]
mod video_transport;

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
use api::{StoatSendMessageRequest, reply_preview_from_message};
#[cfg(feature = "native")]
pub use config::{OFFICIAL_STOAT_BASE_URL, StoatAuthInput, StoatConfig, StoatConfigError};
#[cfg(feature = "native")]
use futures::future;
#[cfg(feature = "native")]
use http::StoatHttpClient;
#[cfg(feature = "native")]
use poly_client::{
    BackendType, ClientError, ClientResult, DmChannel, Message, MessageContent, MessageQuery,
    SettingsStorageCell, User,
};
#[cfg(feature = "voice")]
use poly_client::VoiceParticipant;
#[cfg(feature = "native")]
use poly_host_bridge::http::{Method, RequestBuilder};
#[cfg(feature = "native")]
use std::collections::{HashMap, HashSet};
#[cfg(feature = "native")]
use std::sync::Mutex;
#[cfg(feature = "native")]
use uuid::Uuid;

/// Return the raw FTL translation source for the Stoat client plugin.
///
/// Mirrors the WIT `plugin-metadata.get-translations(locale)` export used by
/// the WASM plugin host.
#[must_use]
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}

/// Callback type stored in `StoatClient::ws_write_tx` for outbound Bonfire WS frames.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
type WsWriteTx = std::sync::Mutex<Option<Box<dyn Fn(String) + Send + Sync + 'static>>>;

/// In-memory state for context-menu toggle actions (F10).
/// Persistent storage is F9 — out of scope here.
#[cfg(feature = "native")]
#[derive(Debug, Default)]
pub(crate) struct StoatMenuState {
    pub(crate) muted_channels: HashSet<String>,
    pub(crate) muted_servers: HashSet<String>,
    pub(crate) blocked_users: HashSet<String>,
    pub(crate) friends: HashSet<String>,
    pub(crate) closed_dms: HashSet<String>,
    pub(crate) muted_dms: HashSet<String>,
}

/// Stoat (Revolt) messenger client.
#[cfg(feature = "native")]
pub struct StoatClient {
    pub(crate) http: StoatHttpClient,
    /// Pack C P18 — in-memory settings storage stub. TODO: migrate to
    /// `host-api.kv_set` once exposed to plugins for true persistence.
    pub(crate) settings_storage: SettingsStorageCell,
    /// F10 — in-memory state for context-menu toggle actions.
    pub(crate) menu_state: Mutex<StoatMenuState>,
    /// Stored version override (None = use http::DEFAULT_CLIENT_VERSION).
    pub(crate) version_override: std::sync::Mutex<Option<String>>,
    /// F.8 — per-account voice connection lock. Only one active voice session
    /// per StoatClient at a time; second connect returns AlreadyConnected.
    #[cfg(feature = "voice")]
    pub(crate) voice_guard: voice::VoiceSessionGuard,
    /// F.6/F.7 — voice participant cache populated by Vortex WS events.
    /// channel_id → Vec<VoiceParticipant>
    #[cfg(feature = "voice")]
    pub(crate) voice_participants: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, Vec<VoiceParticipant>>>>,
    /// B.6 — live WASM voice connection handle.
    ///
    /// Holds `Some` while a WASM Vortex voice WS is open.
    /// Set by `join_voice_channel_transport` on wasm32.
    /// The `StoatVoiceConnection` must be stored here (not dropped) or all
    /// background tasks (encode/decode/event loops) immediately stop.
    /// Analogous to discord's `voice_bridge_client` field.
    #[cfg(target_arch = "wasm32")]
    pub(crate) voice_wasm_conn: std::sync::Arc<std::sync::Mutex<Option<voice_wasm::StoatVoiceConnection>>>,
    /// B.8 — runtime noise-cancellation toggle.
    ///
    /// Shared with the audio-capture task via an `Arc<AtomicBool>`.  Writing
    /// this flag takes effect on the very next 480-sample RNNoise chunk —
    /// there is no audio gap.  Defaults to `true` (noise cancellation on).
    ///
    /// Call `set_noise_cancel(enabled)` to update.  The UI writes to
    /// `VoiceMediaSettings.noise_cancel_enabled` and a `use_reactive_effect`
    /// in voice settings forwards changes here (deferred to UI-layer work;
    /// wiring tracked at crates/core/src/ui/account/settings/voice_settings.rs).
    #[cfg(target_arch = "wasm32")]
    pub(crate) voice_noise_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// B.5 — live WASM video capture handle.
    ///
    /// Holds `Some` while a WASM video capture session is active.
    /// Set by `start_video_capture`; cleared by `stop_video_capture` or when
    /// the voice connection is torn down.  The `StoatVideoCaptureHandle` must
    /// be stored here (not dropped) or the camera is released immediately.
    #[cfg(target_arch = "wasm32")]
    pub(crate) video_wasm_conn: std::sync::Arc<std::sync::Mutex<Option<video_wasm_capture::StoatVideoCaptureHandle>>>,
    /// H.2/H.5 — transient voice channels created for DM calls.
    ///
    /// Maps `dm_channel_id → transient_channel_id` so that H.5 cleanup can
    /// DELETE the synthetic channel when the call ends.  Present on all build
    /// targets (native + wasm32) because `start_dm_call_transport` is called
    /// from both.
    #[cfg(feature = "native")]
    pub(crate) transient_dm_channels: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
    /// C.1 — Bonfire WS write-path sender for outbound frames (e.g. typing indicators).
    ///
    /// Populated by `event_stream` when the WS task successfully authenticates.
    /// `send_typing` writes a `ChannelStartTyping` JSON frame through this
    /// callback without needing to know the underlying WS transport type.
    ///
    /// The callback is `Send + Sync` so it can be called from any async context.
    /// An unbounded-channel sender is used internally so the call never blocks.
    /// Set to `None` until `event_stream` establishes the WS connection.
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    pub(crate) ws_write_tx: WsWriteTx,
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
            version_override: std::sync::Mutex::new(None),
            #[cfg(feature = "voice")]
            voice_guard: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            #[cfg(feature = "voice")]
            voice_participants: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            #[cfg(target_arch = "wasm32")]
            voice_wasm_conn: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(target_arch = "wasm32")]
            voice_noise_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            #[cfg(target_arch = "wasm32")]
            video_wasm_conn: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(feature = "native")]
            transient_dm_channels: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
            ws_write_tx: std::sync::Mutex::new(None),
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

    /// F.3 / K.3 — Connect to a Stoat voice channel using the given audio backend.
    ///
    /// 1. Calls `POST /channels/{channel_id}/join_call` to obtain Vortex credentials.
    /// 2. Connects the Vortex WebSocket and starts Opus encode/decode loops.
    ///
    /// # Errors
    ///
    /// Returns [`StoatVoiceError::AlreadyConnected`] if a voice session is already active.
    /// Call [`disconnect_voice`] first.
    #[cfg(feature = "voice")]
    pub async fn connect_voice(
        &self,
        channel_id: &str,
        audio: &(dyn poly_audio_backend::AudioBackend + Send + Sync),
        transmit_mode: Option<voice_common::TransmitMode>,
        event_tx: tokio::sync::mpsc::Sender<poly_client::ClientEvent>,
    ) -> Result<(), voice_common::StoatVoiceError> {
        // Step 1: obtain Vortex token + WS URL.
        let response = self
            .http
            .authenticated_request(Method::POST, &format!("/channels/{channel_id}/join_call"))
            .map_err(|e| voice_common::StoatVoiceError::JoinCallFailed(e.to_string()))?
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| voice_common::StoatVoiceError::JoinCallFailed(e.to_string()))?;

        let resp: serde_json::Value = response
            .json()
            .await
            .map_err(|e| voice_common::StoatVoiceError::JoinCallFailed(e.to_string()))?;

        let token = resp.get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| voice_common::StoatVoiceError::JoinCallFailed("missing token".into()))?
            .to_string();
        let ws_url = resp.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| voice_common::StoatVoiceError::JoinCallFailed("missing url".into()))?
            .to_string();

        let server_info = voice_common::VortexServerInfo {
            token,
            ws_url,
            channel_id: channel_id.to_string(),
        };

        // Step 2: open voice connection.
        voice::connect_voice(
            std::sync::Arc::clone(&self.voice_guard),
            server_info,
            audio,
            transmit_mode,
            event_tx,
        )
        .await
    }

    /// F.8 — Disconnect from the active Stoat voice channel.
    #[cfg(feature = "voice")]
    pub async fn disconnect_voice(&self) {
        voice::disconnect_voice(std::sync::Arc::clone(&self.voice_guard)).await;
    }

    /// F.7 — Get cached voice participants for a channel.
    #[cfg(feature = "voice")]
    pub async fn voice_participants_for(&self, channel_id: &str) -> Vec<poly_client::VoiceParticipant> {
        voice::get_voice_participants_cached(&self.voice_guard, channel_id).await
    }

    /// B.8 — Toggle RNNoise noise cancellation on the running WASM voice session.
    ///
    /// Takes effect on the next 480-sample chunk — no audio gap, no reconnect
    /// required.  Safe to call while disconnected (the flag is stored and applied
    /// when the next voice session is started).
    ///
    /// The UI layer calls this from a `use_reactive_effect` that watches
    /// `VoiceMediaSettings.noise_cancel_enabled` (wired in
    /// `crates/core/src/ui/account/settings/voice_settings.rs`).
    #[cfg(target_arch = "wasm32")]
    pub fn set_noise_cancel(&self, enabled: bool) {
        self.voice_noise_cancel
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
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

    pub(crate) async fn fetch_last_message_preview(
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

    pub(crate) async fn map_dm_like_channel(
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
            backend: BackendType::from(crate::SLUG),
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

    pub(crate) async fn send_message_internal(
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

#[cfg(all(test, feature = "native"))]
mod tests {
    use poly_client::{
        IsBackend, BackendType, PresenceStatus,
    };
    use super::{OFFICIAL_STOAT_BASE_URL, StoatClient};
    use crate::http::StoatSessionState;
    use axum::{
        Json, Router,
        extract::State,
        http::HeaderMap,
        response::IntoResponse,
        routing::{get, post},
    };

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
                    backend: BackendType::from(crate::SLUG),
                },
                session_name: Some("Poly".to_string()),
            })
        });

        let session = session.expect("authenticate should succeed");
        assert_eq!(session.backend, BackendType::from(crate::SLUG));
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

        let sent = poly_client::MessagingBackend::send_reply_message(
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
        assert_eq!(user.backend, BackendType::from(crate::SLUG));

        Ok(())
    }
}
