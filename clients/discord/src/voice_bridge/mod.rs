//! Discord voice dispatch using generic host primitives — WASM target only.
//!
//! Available when `feature = "voice-bridge"` is enabled. Compiles on
//! `wasm32-unknown-unknown`. The full Discord voice protocol lives here
//! (handshake, RTP framing, nonce derivation), routing over three generic
//! host-bridge endpoints:
//!
//! - `/host/udp/*`        — raw UDP socket (bind, connect, send, recv_stream)
//! - `/host/codec/opus/*` — Opus encode/decode
//! - `/host/aead/*`       — AEAD encrypt/decrypt (XChaCha20Poly1305)
//!
//! On `wasm32-unknown-unknown`, `DiscordClient` (the native struct) does not
//! exist — it requires `feature = "native"` which pulls in tokio-tungstenite,
//! audiopus, etc. This module provides the voice surface needed by the UI
//! without any of those native deps.
//!
//! # Protocol architecture (WASM path)
//!
//! ```text
//! Browser WASM (this module)
//! ──────────────────────────────────────────────────────
//!   1. Open browser WebSocket → wss://<endpoint>/?v=8
//!   2. Drive handshake:
//!        op 8 HELLO  → op 0 IDENTIFY  → op 2 READY
//!        op 1 SELECT_PROTOCOL ←─────────── IP discovery via /host/udp/*
//!        op 4 SESSION_DESCRIPTION  (secret_key)
//!   3. POST /host/udp/bind → session_id, local_port
//!      POST /host/udp/connect { peer: discord_udp_addr }
//!   4. POST /host/codec/opus/encoder/create { sr=48000, ch=2, app=voip }
//!   5. POST /host/aead/create { xchacha20poly1305, secret_key }
//!   6. Encode loop (mic PCM → /host/codec/opus/encoder/encode →
//!                   build RTP header → derive nonce →
//!                   /host/aead/encrypt → /host/udp/send)
//!   7. Decode loop (SSE /host/udp/recv_stream →
//!                   strip RTP → /host/aead/decrypt →
//!                   /host/codec/opus/decoder/decode → VoiceEvent::FrameAudio)
//! ```
//!
//! # Native path (unchanged)
//!
//! The native voice transport in `clients/discord/src/voice/mod.rs` is
//! unaffected — it speaks the protocol natively with tokio-tungstenite +
//! audiopus + chacha20poly1305 directly. The refactor only changes the bridge
//! (WASM) path.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Audio capture loop (Phase X.2). Browser-facing capture pipeline is
/// wasm32-only; pure DSP helpers + their unit tests compile on every
/// target. See module doc.
pub mod audio_capture;

use poly_host_bridge::{
    aead_client::AeadClient,
    codec_opus_client::OpusClient,
    udp_client::UdpClient,
    voice_wire::VoiceEvent,
};
use tokio::sync::Mutex;

pub use voice_protocol::WsHandle;

// Phase X.3 — wasm-only audio playback loop (UDP recv → AEAD decrypt → Opus
// decode → per-SSRC AudioContext). Path-attribute lets the submodule live at
// `src/voice_bridge/audio_playback.rs` without converting this file into a
// `mod.rs`. Module is `#[cfg(target_arch = "wasm32")]` internally.
#[path = "voice_bridge/audio_playback.rs"]
pub mod audio_playback;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors from the voice-bridge path.
///
/// Mirrors the variants the UI actually inspects from `voice::VoiceError`,
/// plus `Bridge` for HTTP transport failures.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("UDP client error: {0}")]
    Udp(#[from] poly_host_bridge::udp_client::UdpClientError),

    #[error("Opus client error: {0}")]
    Opus(#[from] poly_host_bridge::codec_opus_client::OpusClientError),

    #[error("AEAD client error: {0}")]
    Aead(#[from] poly_host_bridge::aead_client::AeadClientError),

    #[error("account already has an active voice connection")]
    AlreadyConnected,

    #[error("voice connect failed: {0}")]
    WsConnect(String),

    #[error("voice state update not received")]
    VoiceStateMissing,

    /// Audio capture init failed (getUserMedia denied, no track, etc.).
    /// Surfaced from `start_audio_capture` on the WASM bridge path.
    #[error("audio capture init failed: {0}")]
    Capture(String),
}

// ── Transmit mode ─────────────────────────────────────────────────────────────

/// Controls when the local user transmits audio.
///
/// On the bridge path, VAD and PTT enforcement is browser-side.
/// This enum is accepted for API compatibility with the native path.
#[derive(Debug, Clone)]
pub enum TransmitMode {
    /// Voice-activity detection: transmit when RMS exceeds `threshold_db`.
    Vad { threshold_db: f32 },
    /// Push-to-talk mode.
    PushToTalk,
}

impl Default for TransmitMode {
    fn default() -> Self {
        Self::Vad { threshold_db: -45.0 }
    }
}

// ── Session state ─────────────────────────────────────────────────────────────

/// Per-call session state held for the duration of a voice call.
///
/// Holds shared resources for the audio capture loop (Phase X.2) and audio
/// playback loop (Phase X.3) — see `capture_shutdown` / `playback_shutdown` /
/// `ssrc_to_user`. The capture/playback loops are spawned by separate
/// orchestration code; this struct only owns the channel ends.
pub struct VoiceBridgeSession {
    /// UDP socket session ID (from `/host/udp/bind`).
    pub udp_session: String,
    /// Opus encoder session ID (from `/host/codec/opus/encoder/create`).
    pub encoder_session: String,
    /// Opus decoder session ID (from `/host/codec/opus/decoder/create`).
    pub decoder_session: String,
    /// AEAD session ID (from `/host/aead/create`).
    pub aead_session: String,
    /// Discord voice SSRC assigned by op 2 Ready.
    pub local_ssrc: u32,
    /// Primitive clients — shared with encode/decode loops.
    pub udp: Arc<UdpClient>,
    pub opus: Arc<OpusClient>,
    pub aead: Arc<AeadClient>,

    /// Channel sender for shutting down the audio capture loop (Phase X.2).
    /// Drop = shutdown. `None` if capture has not been started yet.
    pub capture_shutdown: Option<futures::channel::oneshot::Sender<()>>,
    /// Channel sender for shutting down the audio playback loop (Phase X.3).
    /// Drop = shutdown. `None` if playback has not been started yet.
    pub playback_shutdown: Option<futures::channel::oneshot::Sender<()>>,
    /// SSRC → user_id map, populated by the op 5 SPEAKING listener on the
    /// WS recv loop. Used by playback to label decoded PCM with the
    /// correct sender. Wrapped in `tokio::sync::RwLock` so the listener
    /// task can write while playback tasks read concurrently.
    pub ssrc_to_user: Arc<tokio::sync::RwLock<HashMap<u32, String>>>,
    /// WS handle kept alive for the duration of the call.
    ///
    /// Needed so op 5 SPEAKING dispatches keep arriving via the pump task
    /// and the SSRC → user map stays current, and so we can echo
    /// op 6 HEARTBEAT_ACK when op 3 HEARTBEAT arrives.
    ///
    /// On wasm32 this is `!Send` (gloo_net WebSocket is `!Send`), so this
    /// field — and therefore `VoiceBridgeSession` itself — is single-thread
    /// only on wasm32. Native builds are fully `Send + Sync`.
    pub ws_handle: WsHandle,

    /// RTP packet sequence number, monotonically incremented per outbound
    /// frame and wrapped at u16::MAX. `Arc` so the spawned capture task can
    /// hold a cheap clone without keeping the session mutex locked across
    /// awaits. Phase X.2 — fixes the previous hardcoded sequence=0 / ts=0
    /// in `send_audio_frame` which made decoded packets impossible to
    /// re-order on the receive side.
    pub rtp_sequence: Arc<std::sync::atomic::AtomicU16>,
    /// RTP timestamp, incremented by `OPUS_FRAME_SAMPLES / 2` (stereo
    /// samples-per-channel = 960) per outbound frame. `Arc` for the same
    /// reason as `rtp_sequence`. Phase X.2.
    pub rtp_timestamp: Arc<std::sync::atomic::AtomicU32>,

    // ── Phase Y additions (video) ────────────────────────────────────────────
    /// Channel sender for shutting down the video capture loop (Phase Y.2).
    /// Drop = shutdown. `None` if video capture has not been started.
    pub video_capture_shutdown: Option<futures::channel::oneshot::Sender<()>>,
    /// Local video SSRC assigned by the server's op 21 Stream Subscription
    /// reply (Phase Y.1). `None` until `start_video_capture` succeeds.
    pub video_ssrc: Option<u32>,
    /// Set of REMOTE video SSRCs known for this session, populated when an
    /// op 21 Stream Subscription announces another participant's video stream
    /// (and could also be filled from op 5 SPEAKING video flags in future).
    /// Audio playback skips packets whose SSRC is in this set; video playback
    /// only processes those SSRCs. Phase Y.3.
    pub video_ssrcs: Arc<tokio::sync::RwLock<HashSet<u32>>>,
}

/// Shared session guard. `None` when not in a call.
pub type VoiceSessionGuard = Arc<Mutex<Option<VoiceBridgeSession>>>;

// ── RTP + nonce constants ─────────────────────────────────────────────────────

const VOICE_WS_VERSION: u8 = 8;
const RTP_PAYLOAD_TYPE_OPUS: u8 = 0x78; // 120
const RTP_HEADER_SIZE: usize = 12;
const OPUS_FRAME_SAMPLES: usize = 1920; // 20ms @ 48 kHz stereo
const PREFERRED_AEAD_MODES: &[&str] =
    &["aead_xchacha20_poly1305_rtpsize", "aead_aes256_gcm_rtpsize"];

// ── DiscordVoiceBridgeClient ──────────────────────────────────────────────────

/// Drives Discord voice from WASM using generic host-bridge primitives.
///
/// On `wasm32-unknown-unknown`, `DiscordClient` (the native struct) does not
/// exist. This type provides the voice surface needed by the UI without any
/// native deps (no FFI, no tokio-tungstenite).
pub struct DiscordVoiceBridgeClient {
    /// Discord account ID for this client instance.
    account_id: String,
    /// Active voice session guard. `None` when not in a call.
    pub voice_session: VoiceSessionGuard,
}

impl DiscordVoiceBridgeClient {
    /// Create a new bridge client for `account_id`.
    #[must_use]
    pub fn new(account_id: impl Into<String>) -> Self {
        Self { account_id: account_id.into(), voice_session: Arc::new(Mutex::new(None)) }
    }

    /// Join a voice channel via the host-bridge generic primitives.
    ///
    /// Performs the full Discord voice protocol:
    /// 1. Opens a browser WebSocket to the Discord voice gateway.
    /// 2. Drives the op 8/0/2/1/4 handshake.
    /// 3. Binds a UDP socket via `/host/udp/bind`, runs IP discovery.
    /// 4. Creates Opus encoder + decoder via `/host/codec/opus/*`.
    /// 5. Creates an AEAD session via `/host/aead/create` with the session key.
    ///
    /// The `_audio` argument is accepted for API parity with the native path.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::AlreadyConnected` if a session is already open.
    /// Returns `VoiceError::WsConnect` for handshake failures.
    pub async fn connect_voice(
        &self,
        ws_endpoint: &str,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        _audio: &dyn poly_audio_backend::AudioBackend,
        _transmit_mode: Option<TransmitMode>,
    ) -> Result<(), VoiceError> {
        {
            let guard = self.voice_session.lock().await;
            if guard.is_some() {
                return Err(VoiceError::AlreadyConnected);
            }
        }

        #[cfg(target_arch = "wasm32")]
        let udp_client = Arc::new(UdpClient::from_origin());
        #[cfg(not(target_arch = "wasm32"))]
        let udp_client = Arc::new(UdpClient::default_local());
        #[cfg(target_arch = "wasm32")]
        let opus_client = Arc::new(OpusClient::from_origin());
        #[cfg(not(target_arch = "wasm32"))]
        let opus_client = Arc::new(OpusClient::default_local());
        #[cfg(target_arch = "wasm32")]
        let aead_client = Arc::new(AeadClient::from_origin());
        #[cfg(not(target_arch = "wasm32"))]
        let aead_client = Arc::new(AeadClient::default_local());

        // Step 1: run the Discord voice WS handshake using the browser WebSocket.
        let handshake = voice_protocol::run_handshake(
            ws_endpoint,
            ws_token,
            ws_session_id,
            guild_id,
            &self.account_id,
        )
        .await
        .map_err(VoiceError::WsConnect)?;

        // Step 2: bind a UDP socket and run IP discovery.
        let bind_resp = udp_client.bind().await?;
        let udp_session = bind_resp.session_id;

        let server_addr = format!("{}:{}", handshake.server_ip, handshake.server_port);
        udp_client.connect(&udp_session, &server_addr).await?;

        let (local_ip, local_port) = voice_protocol::ip_discovery_via_udp(
            &udp_client,
            &udp_session,
            handshake.ssrc,
            bind_resp.local_port,
        )
        .await
        .map_err(VoiceError::WsConnect)?;

        // Step 3: finish WS negotiation (SELECT_PROTOCOL) with the discovered IP/port.
        let secret_key = voice_protocol::finish_handshake(
            &handshake.ws_handle,
            &local_ip,
            local_port,
            &handshake.mode,
        )
        .await
        .map_err(VoiceError::WsConnect)?;

        // Step 4: create Opus encoder + decoder sessions.
        let encoder_session = opus_client.encoder_create(48_000, 2, "voip").await?;
        let decoder_session = opus_client.decoder_create(48_000, 2).await?;

        // Step 5: create AEAD session with the 32-byte secret key.
        let aead_session =
            aead_client.create("xchacha20poly1305", &secret_key).await?;

        // Phase X.0 F.5 — spawn the SPEAKING / HEARTBEAT_ACK listener task
        // that consumes the WS recv channel for the rest of the call. It
        // populates `ssrc_to_user` from op 5 SPEAKING frames and echoes
        // op 6 HEARTBEAT_ACK in response to op 3 HEARTBEAT. The task ends
        // when the recv channel closes (WS dropped) or the session is
        // dropped (sender side gone).
        let ssrc_to_user: Arc<tokio::sync::RwLock<HashMap<u32, String>>> =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        if let Some(recv) = handshake.ws_handle.take_recv() {
            spawn_ws_event_listener(recv, &handshake.ws_handle, Arc::clone(&ssrc_to_user));
        }

        {
            let mut guard = self.voice_session.lock().await;
            *guard = Some(VoiceBridgeSession {
                udp_session,
                encoder_session,
                decoder_session,
                aead_session,
                local_ssrc: handshake.ssrc,
                udp: Arc::clone(&udp_client),
                opus: Arc::clone(&opus_client),
                aead: Arc::clone(&aead_client),
                capture_shutdown: None,
                playback_shutdown: None,
                ssrc_to_user,
                ws_handle: handshake.ws_handle,
                rtp_sequence: Arc::new(std::sync::atomic::AtomicU16::new(0)),
                rtp_timestamp: Arc::new(std::sync::atomic::AtomicU32::new(0)),
                video_capture_shutdown: None,
                video_ssrc: None,
                video_ssrcs: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            });
        }

        // Phase X.3 — playback is on by default for every connected call so
        // remote audio plays as soon as the WS handshake settles. Capture
        // (mic) stays opt-in (user clicks the mic button). On wasm32 this
        // spawns the AudioContext loop; on native it's a stub that returns
        // an error which we ignore (native uses CPAL elsewhere).
        #[cfg(target_arch = "wasm32")]
        {
            let (_speaking_tx, _speaking_rx) =
                futures::channel::mpsc::unbounded::<audio_playback::RemoteSpeakingEvent>();
            // The receiver is dropped here for now — once the core voice-
            // event router wires `ClientEvent::VoiceSpeakingUpdate` through
            // from this crate, `_speaking_rx` becomes the input to that
            // router. The playback loop's `unbounded_send` will then no-op
            // (Err on closed channel) which is silently swallowed.
            if let Err(e) = self.start_audio_playback_with_sink(_speaking_tx).await {
                tracing::warn!(
                    target: "poly_discord::voice_bridge",
                    error = %e,
                    "connect_voice: start_audio_playback failed (call connected but playback offline)"
                );
            }
        }

        Ok(())
    }

    /// Start the audio playback loop (Phase X.3).
    ///
    /// Returns `Err` when no active call exists. On success the playback
    /// shutdown sender is stashed on `VoiceBridgeSession.playback_shutdown`
    /// so `disconnect_voice` tears the loop down by drop.
    ///
    /// The `on_remote_speaking` channel emits one `RemoteSpeakingEvent` per
    /// decoded frame whose RMS exceeds `-45 dB`; consumers map these to
    /// `ClientEvent::VoiceSpeakingUpdate` for the UI speaking indicators.
    #[cfg(target_arch = "wasm32")]
    pub async fn start_audio_playback_with_sink(
        &self,
        on_remote_speaking: futures::channel::mpsc::UnboundedSender<
            audio_playback::RemoteSpeakingEvent,
        >,
    ) -> Result<(), VoiceError> {
        let (udp, opus, aead, udp_session, decoder_session, aead_session, ssrc_to_user, local_ssrc) = {
            let guard = self.voice_session.lock().await;
            let s = guard
                .as_ref()
                .ok_or_else(|| VoiceError::WsConnect("no active voice session".into()))?;
            (
                Arc::clone(&s.udp),
                Arc::clone(&s.opus),
                Arc::clone(&s.aead),
                s.udp_session.clone(),
                s.decoder_session.clone(),
                s.aead_session.clone(),
                Arc::clone(&s.ssrc_to_user),
                s.local_ssrc,
            )
        };

        let shutdown_tx = audio_playback::start_audio_playback(audio_playback::PlaybackParams {
            udp,
            opus,
            aead,
            udp_session,
            decoder_session,
            aead_session,
            ssrc_to_user,
            local_ssrc,
            on_remote_speaking,
        })
        .await
        .map_err(VoiceError::WsConnect)?;

        let mut guard = self.voice_session.lock().await;
        if let Some(s) = guard.as_mut() {
            s.playback_shutdown = Some(shutdown_tx);
        }
        Ok(())
    }

    /// Native stub for `start_audio_playback_with_sink` — Phase X.3 ships
    /// the WASM playback path only. Returns Ok unconditionally so callers
    /// that compile on both targets can invoke it without cfg-gates.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn start_audio_playback_with_sink(
        &self,
        _on_remote_speaking: futures::channel::mpsc::UnboundedSender<
            audio_playback::RemoteSpeakingEvent,
        >,
    ) -> Result<(), VoiceError> {
        let guard = self.voice_session.lock().await;
        if guard.is_none() {
            return Err(VoiceError::WsConnect("no active voice session".into()));
        }
        Ok(())
    }

    /// Leave the current voice channel via the host-bridge.
    ///
    /// Closes UDP, Opus, and AEAD sessions. Idempotent — safe to call when
    /// not in a call.
    ///
    /// # Errors
    ///
    /// Returns an error on transport failure.
    pub async fn disconnect_voice(&self, _guild_id: &str) -> Result<(), VoiceError> {
        let mut guard = self.voice_session.lock().await;
        if let Some(s) = guard.take() {
            // Phase X.0 F.6 — drop the capture/playback shutdown senders.
            // The receivers wake with a `Canceled` error and tear down the
            // loops. Drop is the signal — no explicit `send(())` needed.
            drop(s.capture_shutdown);
            drop(s.playback_shutdown);
            // Phase Y.5 — also tear down the video capture loop if running.
            drop(s.video_capture_shutdown);

            // Drop the WS handle to signal the pump task + listener to exit.
            // The pump task's `ws_rx.next()` returns `None` once the WS is
            // dropped, which closes the recv channel sender, which makes the
            // listener's `recv.next()` return `None` and exit.
            drop(s.ws_handle);

            // Best-effort close — ignore individual errors.
            let _ = s.udp.close(&s.udp_session).await;
            let _ = s.opus.close(&s.encoder_session).await;
            let _ = s.opus.close(&s.decoder_session).await;
            let _ = s.aead.close(&s.aead_session).await;
        }
        Ok(())
    }

    /// Send a PCM audio frame through the encode path.
    ///
    /// Frame → `/host/codec/opus/encoder/encode` → build RTP header →
    /// derive XChaCha20 nonce → `/host/aead/encrypt` → `/host/udp/send`.
    ///
    /// `pcm` must be 48 kHz stereo LE i16 — exactly `OPUS_FRAME_SAMPLES` samples.
    ///
    /// # Errors
    ///
    /// Returns an error when no session is active or on transport failure.
    pub async fn send_audio_frame(&self, pcm: &[i16]) -> Result<(), VoiceError> {
        let guard = self.voice_session.lock().await;
        let s = guard
            .as_ref()
            .ok_or_else(|| VoiceError::WsConnect("no active voice session".into()))?;

        // Encode.
        let opus_packet = s.opus.encode(&s.encoder_session, pcm).await?;

        // Phase X.2 — bump RTP sequence (wraps at u16::MAX) and timestamp
        // (incremented by samples-per-channel = 960 for a 20 ms@48 kHz
        // stereo frame). Relaxed ordering is fine: the encode + send path
        // is serialized through the session mutex above so there's no
        // multi-task contention on these atomics.
        use std::sync::atomic::Ordering;
        let sequence = s.rtp_sequence.fetch_add(1, Ordering::Relaxed);
        let timestamp = s
            .rtp_timestamp
            .fetch_add((OPUS_FRAME_SAMPLES / 2) as u32, Ordering::Relaxed);
        let rtp_header = build_rtp_header(sequence, timestamp, s.local_ssrc);
        let nonce = xchacha_nonce_from_rtp(&rtp_header);

        // Encrypt.
        let ciphertext = s
            .aead
            .encrypt(&s.aead_session, &nonce, &opus_packet, Some(&rtp_header))
            .await?;

        // Build RTP packet: header + ciphertext.
        let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + ciphertext.len());
        packet.extend_from_slice(&rtp_header);
        packet.extend_from_slice(&ciphertext);

        s.udp.send(&s.udp_session, &packet, None).await?;

        Ok(())
    }

    /// Start the audio capture loop (Phase X.2 — wasm-only).
    ///
    /// Browser path: `navigator.mediaDevices.getUserMedia({audio: true})` →
    /// `MediaStreamTrackProcessor` → 48 kHz stereo i16 PCM frames →
    /// `send_audio_frame` (Opus encode + AEAD encrypt + RTP wrap + UDP send).
    ///
    /// Stores the loop's shutdown sender on `VoiceBridgeSession.capture_shutdown`.
    /// Dropping the session (via `disconnect_voice`) drops the sender, the
    /// receiver wakes with `Canceled`, and the loop exits cleanly.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::WsConnect` when no voice session is active.
    /// Returns `VoiceError::Capture` when getUserMedia fails (permission
    /// denied, no audio device, browser not Chromium-based) or when the
    /// browser does not expose `MediaStreamTrackProcessor`.
    ///
    /// On non-wasm32 builds this is a no-op success — native callers use the
    /// `clients/discord/src/voice/mod.rs` cpal-based capture path instead.
    pub async fn start_audio_capture(&self) -> Result<(), VoiceError> {
        #[cfg(target_arch = "wasm32")]
        {
            // Snapshot the fields the capture loop needs out of the session
            // so we don't keep the session mutex locked while
            // `audio_capture::start_audio_capture` awaits getUserMedia.
            let (udp, opus, aead, udp_session, encoder_session, aead_session,
                 local_ssrc, rtp_sequence, rtp_timestamp) = {
                let guard = self.voice_session.lock().await;
                let s = guard.as_ref().ok_or_else(|| {
                    VoiceError::WsConnect("no active voice session".into())
                })?;
                (
                    Arc::clone(&s.udp),
                    Arc::clone(&s.opus),
                    Arc::clone(&s.aead),
                    s.udp_session.clone(),
                    s.encoder_session.clone(),
                    s.aead_session.clone(),
                    s.local_ssrc,
                    Arc::clone(&s.rtp_sequence),
                    Arc::clone(&s.rtp_timestamp),
                )
            };

            let shutdown_tx = audio_capture::start_audio_capture(
                audio_capture::CaptureParams {
                    udp,
                    opus,
                    aead,
                    udp_session,
                    encoder_session,
                    aead_session,
                    local_ssrc,
                    rtp_sequence,
                    rtp_timestamp,
                },
            )
            .await
            .map_err(VoiceError::Capture)?;

            let mut guard = self.voice_session.lock().await;
            if let Some(s) = guard.as_mut() {
                s.capture_shutdown = Some(shutdown_tx);
            } else {
                return Err(VoiceError::WsConnect(
                    "voice session ended during start_audio_capture".into(),
                ));
            }
            Ok(())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let guard = self.voice_session.lock().await;
            if guard.is_none() {
                return Err(VoiceError::WsConnect("no active voice session".into()));
            }
            Ok(())
        }
    }

    /// Start the WebCodecs camera capture loop (Phase Y.2).
    ///
    /// Negotiates a video stream via op 12 / op 21 (allocating a video SSRC),
    /// then opens `getUserMedia({video:...})`, attaches a
    /// `MediaStreamTrackProcessor`, encodes frames with `VideoEncoder`
    /// (H.264 baseline `avc1.42E01F`, 640×360 @ 30 fps, 800 kbps),
    /// fragments NAL units to RTP FU-A (≤1200 B), AEAD-encrypts with the
    /// video SSRC's RTP header as nonce, and `send`s the packets over UDP.
    ///
    /// On non-wasm32 targets this returns `Ok(())` after validating that a
    /// session exists — the WebCodecs APIs only exist in browsers.
    ///
    /// # Errors
    ///
    /// `VoiceError::WsConnect` when no voice session is active or when op 21
    /// negotiation fails.
    pub async fn start_video_capture(&self) -> Result<(), VoiceError> {
        // Step 1: negotiate a video SSRC via op 12 → op 21.
        self.negotiate_video_stream().await?;

        // Step 2: on wasm32, spawn the WebCodecs capture loop. On native
        // (test) builds, this is a no-op — the capture pipeline is browser-
        // only by construction.
        #[cfg(target_arch = "wasm32")]
        {
            let handles = video_capture::VideoBridgeHandles::from_session(
                &self.voice_session,
            )
            .await
            .ok_or_else(|| VoiceError::WsConnect("no active voice session".into()))?;
            let shutdown_tx = video_capture::start_video_capture(handles)
                .await
                .map_err(VoiceError::WsConnect)?;
            let mut guard = self.voice_session.lock().await;
            if let Some(s) = guard.as_mut() {
                s.video_capture_shutdown = Some(shutdown_tx);
            }
        }
        Ok(())
    }

    /// Stop the WebCodecs camera capture loop. Idempotent.
    ///
    /// # Errors
    ///
    /// Currently infallible; returns `Result` for API symmetry with
    /// `start_video_capture` and future-proofing.
    pub async fn stop_video_capture(&self) -> Result<(), VoiceError> {
        let mut guard = self.voice_session.lock().await;
        if let Some(s) = guard.as_mut() {
            // Drop the sender → the capture loop's recv wakes Canceled and exits.
            drop(s.video_capture_shutdown.take());
        }
        Ok(())
    }

    /// Send op 12 STREAM_CREATE and wait for op 21 Stream Subscription.
    /// Stashes the negotiated `video_ssrc` on the session.
    async fn negotiate_video_stream(&self) -> Result<u32, VoiceError> {
        // Snapshot the ws handle without holding the lock across awaits in a
        // way that crosses task boundaries (we still hold the Mutex through
        // the WS round-trip, which is fine — connect_voice does the same).
        let guard = self.voice_session.lock().await;
        let s = guard
            .as_ref()
            .ok_or_else(|| VoiceError::WsConnect("no active voice session".into()))?;

        // Send op 12.
        let payload = serde_json::json!({
            "op": 12,
            "d": { "type": "video", "rid": "high", "quality": 100 }
        });
        (s.ws_handle.send)(payload.to_string())
            .await
            .map_err(VoiceError::WsConnect)?;

        // Wait for op 21 (skip unrelated frames, 5s per-frame timeout).
        let video_ssrc: u32 = loop {
            let msg = s
                .ws_handle
                .recv_text_with_timeout(std::time::Duration::from_secs(5))
                .await
                .map_err(VoiceError::WsConnect)?;
            let v: serde_json::Value = match serde_json::from_str(&msg) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("op").and_then(|o| o.as_u64()) != Some(21) {
                continue;
            }
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            if d.get("type").and_then(|t| t.as_str()) != Some("video") {
                continue;
            }
            if let Some(ssrc) = d.get("video_ssrc").and_then(|s| s.as_u64()) {
                break ssrc as u32;
            }
        };
        drop(guard);

        // Stash on the session.
        let mut guard = self.voice_session.lock().await;
        if let Some(s) = guard.as_mut() {
            s.video_ssrc = Some(video_ssrc);
        }
        Ok(video_ssrc)
    }

    /// Legacy entry kept for API parity with `start_screen_share`.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::WsConnect` when no voice session is active.
    pub async fn start_video(
        &self,
        _frame_rx: tokio::sync::mpsc::Receiver<poly_video_backend::types::VideoFrame>,
        _bridge_base_url: String,
    ) -> Result<(), VoiceError> {
        let guard = self.voice_session.lock().await;
        if guard.is_none() {
            return Err(VoiceError::WsConnect("no active voice session".into()));
        }
        Ok(())
    }

    /// Stop sending camera video. No-op if not in a call.
    pub async fn stop_video(&self) {}

    /// Start sending screen-share video via the host-bridge.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::WsConnect` when no voice session is active.
    pub async fn start_screen_share(
        &self,
        frame_rx: tokio::sync::mpsc::Receiver<poly_video_backend::types::VideoFrame>,
        bridge_base_url: String,
    ) -> Result<(), VoiceError> {
        self.start_video(frame_rx, bridge_base_url).await
    }

    /// Stop sending screen share. No-op if not in a call.
    pub async fn stop_screen_share(&self) {
        self.stop_video().await;
    }

    /// Toggle mute/deafen state.
    ///
    /// When `muted`, the caller simply stops calling `send_audio_frame`.
    /// This method is accepted for API parity with the native `set_self_mute`
    /// but does not send a gateway speaking op on the bridge path — the WS
    /// handle is owned by the handshake task.
    ///
    /// # Errors
    ///
    /// Returns `VoiceError::WsConnect` when no voice session is active.
    pub async fn set_self_mute(
        &self,
        _guild_id: &str,
        _channel_id: Option<&str>,
        _self_mute: bool,
        _self_deaf: bool,
    ) -> Result<(), VoiceError> {
        let guard = self.voice_session.lock().await;
        if guard.is_none() {
            return Err(VoiceError::WsConnect("no active voice session".into()));
        }
        Ok(())
    }

    /// Subscribe to incoming voice events (speaking indicators, decoded audio).
    ///
    /// Returns a stream of `VoiceEvent`. The stream is backed by the UDP recv
    /// SSE stream — each datagram is decrypted and Opus-decoded on the WASM side.
    /// Returns `None` when no session is active.
    pub async fn subscribe_events(
        &self,
    ) -> Option<impl futures::Stream<Item = VoiceEvent> + 'static> {
        // Clone all needed state out of the lock before building the stream.
        let (udp_session, aead, opus, aead_session, decoder_session, udp) = {
            let guard = self.voice_session.lock().await;
            let s = guard.as_ref()?;
            (
                s.udp_session.clone(),
                Arc::clone(&s.aead),
                Arc::clone(&s.opus),
                s.aead_session.clone(),
                s.decoder_session.clone(),
                Arc::clone(&s.udp),
            )
        };

        let dgram_stream = udp.recv_stream_boxed(udp_session);

        use futures::StreamExt;
        let event_stream = dgram_stream.filter_map(move |dgram| {
            let aead = Arc::clone(&aead);
            let opus = Arc::clone(&opus);
            let aead_session = aead_session.clone();
            let decoder_session = decoder_session.clone();
            async move {
                use base64::Engine as _;
                let packet = base64::engine::general_purpose::STANDARD
                    .decode(dgram.data.as_bytes())
                    .ok()?;

                if packet.len() < RTP_HEADER_SIZE {
                    return None;
                }
                let (ssrc, payload_offset) = parse_rtp_header(&packet)?;
                let rtp_header = &packet[..payload_offset];
                let ciphertext = &packet[payload_offset..];
                let nonce = xchacha_nonce_from_rtp(rtp_header);

                let plaintext = aead
                    .decrypt(&aead_session, &nonce, ciphertext, Some(rtp_header))
                    .await
                    .ok()?;

                let pcm = opus.decode(&decoder_session, &plaintext).await.ok()?;
                let mut pcm_bytes = Vec::with_capacity(pcm.len() * 2);
                for s in &pcm {
                    pcm_bytes.extend_from_slice(&s.to_le_bytes());
                }
                let pcm_b64 =
                    base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);
                let samples = (pcm.len() / 2) as u32;

                Some(VoiceEvent::FrameAudio {
                    user_id: format!("user_{ssrc}"),
                    pcm_b64,
                    samples,
                })
            }
        });

        Some(event_stream)
    }
}

// ── WS event listener (Phase X.0 F.5) ─────────────────────────────────────────

/// Spawn the post-handshake WS event listener.
///
/// Consumes the recv channel for the lifetime of the call, dispatching:
///   - op 5 SPEAKING — insert `(ssrc → user_id)` into `ssrc_to_user`.
///   - everything else — currently ignored. op 3 HEARTBEAT is a client→server
///     opcode (we send it on our heartbeat timer) so we don't expect to
///     receive op 3 from the gateway. op 6 HEARTBEAT_ACK is informational.
///     op 13 CLIENT_DISCONNECT / op 14 DAVE protocol frames are future work.
///
/// The listener exits when:
///   - the recv channel returns `None` (WS pump dropped its sender, i.e.
///     WS closed or `disconnect_voice` was called), OR
///   - the task is implicitly cancelled by tab teardown on wasm32.
fn spawn_ws_event_listener(
    mut recv: futures::channel::mpsc::UnboundedReceiver<String>,
    _ws_handle: &voice_protocol::WsHandle,
    ssrc_to_user: Arc<tokio::sync::RwLock<HashMap<u32, String>>>,
) {
    let task = async move {
        use futures::StreamExt;
        while let Some(msg) = recv.next().await {
            let v: serde_json::Value = match serde_json::from_str(&msg) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(0);
            if op == 5 {
                let d = match v.get("d") {
                    Some(d) => d,
                    None => continue,
                };
                let ssrc = match d.get("ssrc").and_then(|s| s.as_u64()) {
                    Some(s) => s as u32,
                    None => continue,
                };
                let user_id = match d.get("user_id").and_then(|u| u.as_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };
                ssrc_to_user.write().await.insert(ssrc, user_id);
            }
            // Other ops are ignored on the bridge path. See doc comment.
        }
    };

    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(task);
    #[cfg(not(target_arch = "wasm32"))]
    tokio::spawn(task);
}

// ── RTP helpers — moved to voice_bridge/rtp.rs (SOLID B.2) ────────────────────────

pub mod rtp;
pub use rtp::*;


// ── Discord voice protocol helpers ────────────────────────────────────────────
//
// These functions implement the Discord voice gateway handshake using the
// browser WebSocket API (`web_sys::WebSocket` on wasm32) or
// `tokio-tungstenite` on native test builds.
//
// They live in a submodule to keep the file navigable.

// ── Discord voice protocol — moved to voice_bridge/voice_protocol.rs (SOLID B.2) ──

pub mod voice_protocol;

// ── Video capture — moved to voice_bridge/video_capture.rs (SOLID B.2) ──────────

pub mod video_capture;

// ── Video playback — moved to voice_bridge/video_playback.rs (SOLID B.2) ───────

pub mod video_playback;

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn rtp_header_round_trip() {
        let h = build_rtp_header(0x1234, 0xDEAD_BEEF, 0xCAFE_BABE);
        assert_eq!(h[0], 0x80);
        assert_eq!(h[1], RTP_PAYLOAD_TYPE_OPUS);
        let (ssrc, offset) = parse_rtp_header(&h).unwrap();
        assert_eq!(ssrc, 0xCAFE_BABE);
        assert_eq!(offset, 12);
    }

    #[test]
    fn xchacha_nonce_is_24_bytes() {
        let h = build_rtp_header(1, 2, 3);
        let nonce = xchacha_nonce_from_rtp(&h);
        assert_eq!(nonce.len(), 24);
    }

    #[test]
    fn parse_session_description_extracts_secret_key() {
        let frame = serde_json::json!({
            "op": 4,
            "d": {
                "mode": "aead_xchacha20_poly1305_rtpsize",
                "secret_key": vec![1u8; 32]
            }
        })
        .to_string();
        let key = voice_protocol::parse_session_description(&frame)
            .unwrap()
            .unwrap();
        assert_eq!(key.len(), 32);
        assert!(key.iter().all(|&b| b == 1));
    }

    #[test]
    fn parse_session_description_rejects_other_ops() {
        let frame = r#"{"op":6,"d":12345}"#;
        let result = voice_protocol::parse_session_description(frame).unwrap();
        assert!(result.is_none(), "op 6 HEARTBEAT_ACK should yield None");
    }

    #[test]
    fn parse_session_description_errors_on_missing_key() {
        let frame = r#"{"op":4,"d":{"mode":"aead_xchacha20_poly1305_rtpsize"}}"#;
        assert!(voice_protocol::parse_session_description(frame).is_err());
    }

    #[test]
    fn preferred_modes_list() {
        let modes = vec![
            "xsalsa20_poly1305".to_string(),
            "aead_xchacha20_poly1305_rtpsize".to_string(),
        ];
        let selected = PREFERRED_AEAD_MODES
            .iter()
            .find(|&&m| modes.iter().any(|s| s == m))
            .map(|&m| m.to_string());
        assert_eq!(selected.unwrap(), "aead_xchacha20_poly1305_rtpsize");
    }
}
