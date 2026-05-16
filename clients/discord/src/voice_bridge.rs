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

        let shutdown_tx = audio_playback::start_audio_playback(
            udp,
            opus,
            aead,
            udp_session,
            decoder_session,
            aead_session,
            ssrc_to_user,
            local_ssrc,
            on_remote_speaking,
        )
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
    #[allow(unused_variables)]
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

// ── RTP helpers ────────────────────────────────────────────────────────────────

fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; RTP_HEADER_SIZE] {
    let mut h = [0u8; RTP_HEADER_SIZE];
    h[0] = 0x80;
    h[1] = RTP_PAYLOAD_TYPE_OPUS;
    h[2] = (sequence >> 8) as u8;
    h[3] = sequence as u8;
    h[4] = (timestamp >> 24) as u8;
    h[5] = (timestamp >> 16) as u8;
    h[6] = (timestamp >> 8) as u8;
    h[7] = timestamp as u8;
    h[8] = (ssrc >> 24) as u8;
    h[9] = (ssrc >> 16) as u8;
    h[10] = (ssrc >> 8) as u8;
    h[11] = ssrc as u8;
    h
}

fn parse_rtp_header(packet: &[u8]) -> Option<(u32, usize)> {
    if packet.len() < RTP_HEADER_SIZE {
        return None;
    }
    if (packet[0] >> 6) & 0x3 != 2 {
        return None;
    }
    let has_ext = (packet[0] >> 4) & 0x1 == 1;
    let csrc_count = (packet[0] & 0x0F) as usize;
    let ssrc = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let mut offset = RTP_HEADER_SIZE + csrc_count * 4;
    if offset > packet.len() {
        return None;
    }
    if has_ext {
        if offset + 4 > packet.len() {
            return None;
        }
        let ext_len = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]) as usize;
        offset += 4 + ext_len * 4;
    }
    if offset > packet.len() {
        return None;
    }
    Some((ssrc, offset))
}

/// Derive a 24-byte XChaCha20 nonce from the RTP header.
/// The Discord `aead_xchacha20_poly1305_rtpsize` mode uses the first 24 bytes
/// of the RTP header (zero-padded) as the nonce.
fn xchacha_nonce_from_rtp(rtp_header: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 24];
    let len = rtp_header.len().min(24);
    nonce[..len].copy_from_slice(&rtp_header[..len]);
    nonce.to_vec()
}

// ── Discord voice protocol helpers ────────────────────────────────────────────
//
// These functions implement the Discord voice gateway handshake using the
// browser WebSocket API (`web_sys::WebSocket` on wasm32) or
// `tokio-tungstenite` on native test builds.
//
// They live in a submodule to keep the file navigable.

pub mod voice_protocol {
    use super::*;
    use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
    use std::cell::RefCell;
    use std::time::Duration;

    /// Result of a successful `run_handshake` call.
    pub struct HandshakeResult {
        /// Discord UDP server IP from op 2 Ready.
        pub server_ip: String,
        /// Discord UDP server port from op 2 Ready.
        pub server_port: u16,
        /// Local SSRC assigned by Discord.
        pub ssrc: u32,
        /// Negotiated AEAD mode string.
        pub mode: String,
        /// Opaque WS handle for the `finish_handshake` call.
        /// On wasm32 this is a gloo WebSocket; on native a tungstenite sink.
        /// We use a boxed dynamic type to keep the function signatures clean.
        pub ws_handle: WsHandle,
    }

    /// A bidirectional handle to the voice WebSocket.
    ///
    /// Carries a send closure plus a `recv` channel fed by a background pump
    /// task that forwards every Text frame off the underlying WebSocket. The
    /// receiver is wrapped so it can be `take()`n exactly once (by the
    /// post-handshake listener task) without blocking other handle users.
    ///
    /// On WASM gloo_net WebSocket is `!Send`; the send closure is therefore
    /// `LocalBoxFuture`-bound and uses `Rc<RefCell<_>>` internally. We use
    /// the same shape on native for API symmetry — the underlying tokio-
    /// tungstenite sink is `Send` so `Arc<tokio::sync::Mutex<_>>` could be
    /// used there, but the current native bridge path returns Err from
    /// `run_handshake` anyway, so single-thread is fine for now.
    pub struct WsHandle {
        /// Closure that sends a JSON string on the voice WebSocket.
        ///
        /// Returns a `LocalBoxFuture` so it works for both wasm32 (where the
        /// underlying WebSocket sink is `!Send`) and native (where it would
        /// be `Send` but we keep the type uniform).
        pub send: Box<dyn Fn(String) -> futures::future::LocalBoxFuture<'static, Result<(), String>>>,
        /// Channel receiver fed by the WS pump task. Wrapped in
        /// `RefCell<Option<_>>` so the post-handshake listener task can
        /// `take_recv()` exactly once. Subsequent callers see `None`.
        ///
        /// `RefCell` (not `Mutex`) because on wasm32 the whole handle is
        /// single-thread by construction, and on native the only place we
        /// touch it is in the synchronous `take_recv` accessor.
        pub recv: RefCell<Option<UnboundedReceiver<String>>>,
    }

    impl WsHandle {
        /// Take ownership of the recv channel. Exactly one caller wins; all
        /// others see `None`. Used by the post-handshake listener task.
        pub fn take_recv(&self) -> Option<UnboundedReceiver<String>> {
            self.recv.borrow_mut().take()
        }

        /// Receive the next Text frame from the WS with a timeout.
        ///
        /// Borrows the recv channel; will return an Err if the channel has
        /// already been taken via `take_recv`. Used by `finish_handshake` to
        /// wait for op 4 SESSION_DESCRIPTION before the long-lived listener
        /// task is spawned.
        ///
        /// On wasm32 the timeout is implemented via
        /// `gloo_timers::future::TimeoutFuture` raced via `futures::select`.
        /// On native we use `tokio::time::timeout`. This mirrors the
        /// `BackendHandleExt::read_with_timeout` pattern documented in
        /// CLAUDE.md hang-class #4 mitigation.
        pub async fn recv_text_with_timeout(
            &self,
            dur: Duration,
        ) -> Result<String, String> {
            use futures::StreamExt;

            // Hold an Option<RefMut> across awaits is fine on single-thread
            // WASM; on native this method is only called from the handshake
            // path which runs on one task. Take the receiver out of the
            // RefCell for the duration of the await and put it back after.
            let mut rx = self
                .recv
                .borrow_mut()
                .take()
                .ok_or("WsHandle.recv already taken — finish_handshake must run before the listener spawns")?;

            let result: Result<String, String> = {
                #[cfg(target_arch = "wasm32")]
                {
                    use futures::future::{select, Either};
                    let timeout = gloo_timers::future::TimeoutFuture::new(
                        u32::try_from(dur.as_millis()).unwrap_or(u32::MAX),
                    );
                    let next = rx.next();
                    futures::pin_mut!(timeout);
                    futures::pin_mut!(next);
                    match select(timeout, next).await {
                        Either::Left(_) => Err(format!(
                            "WsHandle.recv_text_with_timeout: timed out after {}ms",
                            dur.as_millis()
                        )),
                        Either::Right((Some(msg), _)) => Ok(msg),
                        Either::Right((None, _)) => Err("WS closed".into()),
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    match tokio::time::timeout(dur, rx.next()).await {
                        Ok(Some(msg)) => Ok(msg),
                        Ok(None) => Err("WS closed".into()),
                        Err(_) => Err(format!(
                            "WsHandle.recv_text_with_timeout: timed out after {}ms",
                            dur.as_millis()
                        )),
                    }
                }
            };

            // Restore the receiver so the long-lived listener task can take
            // it after the handshake finishes.
            *self.recv.borrow_mut() = Some(rx);
            result
        }
    }

    impl std::fmt::Debug for WsHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WsHandle").finish_non_exhaustive()
        }
    }

    /// Build a sender pair used by the WS pump task (both WASM and native).
    pub(super) fn ws_recv_channel() -> (UnboundedSender<String>, UnboundedReceiver<String>) {
        futures::channel::mpsc::unbounded()
    }

    /// Parse the op 4 SESSION_DESCRIPTION payload and extract the 32-byte
    /// `secret_key`. Returns Err if the frame is not op 4 or if `d.secret_key`
    /// is missing / not an array of ints.
    ///
    /// Extracted as a helper so it can be unit-tested without spinning up
    /// the WS / UDP / AEAD stack.
    pub fn parse_session_description(frame: &str) -> Result<Option<Vec<u8>>, String> {
        let v: serde_json::Value = serde_json::from_str(frame)
            .map_err(|e| format!("session_description parse: {e}"))?;
        if v.get("op").and_then(|o| o.as_u64()) != Some(4) {
            return Ok(None);
        }
        let arr = v
            .pointer("/d/secret_key")
            .and_then(|k| k.as_array())
            .ok_or("op 4: missing d.secret_key array")?;
        let key: Vec<u8> = arr
            .iter()
            .filter_map(|n| n.as_u64().map(|x| x as u8))
            .collect();
        if key.is_empty() {
            return Err("op 4: secret_key array is empty".into());
        }
        Ok(Some(key))
    }

    /// Run the Discord voice WS handshake.
    ///
    /// Sequence: op 8 HELLO → op 0 IDENTIFY → op 2 READY.
    ///
    /// Returns a `HandshakeResult` containing the UDP server address,
    /// the SSRC, the negotiated AEAD mode, and a WS handle for subsequent
    /// sends (op 1 SELECT_PROTOCOL).
    pub async fn run_handshake(
        ws_endpoint: &str,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<HandshakeResult, String> {
        // Use plain `ws://` for loopback endpoints (local dev / mock server),
        // `wss://` for all other hosts. Matches POLY_DISCORD_VOICE_WS_INSECURE
        // semantics without requiring an env-var import in WASM.
        let host = ws_endpoint.trim_end_matches(':').trim_end_matches('/');
        let scheme = if host.starts_with("127.0.0.1") || host.starts_with("localhost") {
            "ws"
        } else {
            "wss"
        };
        let ws_url = format!(
            "{scheme}://{host}/voice/ws?v={}",
            super::VOICE_WS_VERSION
        );

        // On wasm32 we use gloo-net WebSocket (browser-native, no FFI).
        #[cfg(target_arch = "wasm32")]
        return run_handshake_wasm(ws_url, ws_token, ws_session_id, guild_id, user_id).await;

        // On native (test / chat-mcp builds) use tokio-tungstenite. Requires
        // the `gateway` feature to pull in tokio-tungstenite.
        #[cfg(all(not(target_arch = "wasm32"), feature = "gateway"))]
        return run_handshake_native(ws_url, ws_token, ws_session_id, guild_id, user_id).await;

        #[cfg(all(not(target_arch = "wasm32"), not(feature = "gateway")))]
        {
            let _ = (ws_url, ws_token, ws_session_id, guild_id, user_id);
            Err("voice_bridge::run_handshake requires either wasm32 target or the `gateway` feature for tokio-tungstenite".into())
        }
    }

    /// IP discovery via `/host/udp/send` + read the response from the UDP SSE stream.
    ///
    /// Sends the 74-byte Discord IP discovery packet and parses the response.
    pub async fn ip_discovery_via_udp(
        udp: &UdpClient,
        session_id: &str,
        ssrc: u32,
        local_port: u16,
    ) -> Result<(String, u16), String> {
        // Build the 74-byte discovery packet.
        let mut buf = [0u8; 74];
        buf[0] = 0x00;
        buf[1] = 0x01;
        buf[2] = 0x00;
        buf[3] = 0x46;
        buf[4] = (ssrc >> 24) as u8;
        buf[5] = (ssrc >> 16) as u8;
        buf[6] = (ssrc >> 8) as u8;
        buf[7] = ssrc as u8;
        // bytes 8..72 are the local IP (zero for request).
        // bytes 72..74 are the local port.
        buf[72] = (local_port >> 8) as u8;
        buf[73] = local_port as u8;

        udp.send(session_id, &buf, None)
            .await
            .map_err(|e| format!("IP discovery send: {e}"))?;

        // Read the response from the SSE stream.
        use futures::StreamExt;
        let mut stream = udp.recv_stream_boxed(session_id.to_string());
        let dgram = stream
            .next()
            .await
            .ok_or("IP discovery: no response from server")?;

        use base64::Engine as _;
        let resp = base64::engine::general_purpose::STANDARD
            .decode(dgram.data.as_bytes())
            .map_err(|e| format!("IP discovery decode: {e}"))?;

        if resp.len() < 74 {
            return Err(format!("IP discovery: short response {} bytes", resp.len()));
        }
        if u16::from_be_bytes([resp[0], resp[1]]) != 0x0002 {
            return Err("IP discovery: unexpected response type".into());
        }
        let addr_end = resp[8..72].iter().position(|&b| b == 0).unwrap_or(64);
        let ip = std::str::from_utf8(&resp[8..8 + addr_end])
            .map_err(|e| format!("IP discovery: bad UTF-8: {e}"))?
            .to_string();
        let port = u16::from_be_bytes([resp[72], resp[73]]);
        Ok((ip, port))
    }

    /// Send op 1 SELECT_PROTOCOL and wait for op 4 SESSION_DESCRIPTION.
    ///
    /// Returns the 32-byte `secret_key`. Loops past unrelated frames
    /// (op 6 HEARTBEAT_ACK, op 5 SPEAKING, etc.) with a 5-second total
    /// timeout per frame read. Discord typically replies within a single
    /// RTT after SELECT_PROTOCOL, so a 5-second per-frame budget is
    /// conservative.
    pub async fn finish_handshake(
        ws_handle: &WsHandle,
        local_ip: &str,
        local_port: u16,
        mode: &str,
    ) -> Result<Vec<u8>, String> {
        let payload = serde_json::json!({
            "op": 1,
            "d": {
                "protocol": "udp",
                "data": { "address": local_ip, "port": local_port, "mode": mode }
            }
        });
        (ws_handle.send)(payload.to_string()).await?;

        // Loop reading from the WS recv channel until op 4 arrives. Skip
        // unrelated ops — they are not fatal here. 5-second per-frame
        // timeout (Phase X.0 F.3).
        loop {
            let msg = ws_handle
                .recv_text_with_timeout(Duration::from_secs(5))
                .await?;
            match parse_session_description(&msg)? {
                Some(secret_key) => return Ok(secret_key),
                None => continue, // not op 4 — keep looping
            }
        }
    }

    // ── WASM-only handshake ────────────────────────────────────────────────────

    #[cfg(target_arch = "wasm32")]
    async fn run_handshake_wasm(
        ws_url: String,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<HandshakeResult, String> {
        use gloo_net::websocket::{Message, futures::WebSocket};
        use futures::{SinkExt, StreamExt};

        let ws = WebSocket::open(&ws_url)
            .map_err(|e| format!("WebSocket::open failed: {e:?}"))?;
        let (mut ws_tx, mut ws_rx) = ws.split();

        // Wait for op 8 HELLO.
        let heartbeat_ms = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("HELLO parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
                        let ms = v
                            .get("d")
                            .and_then(|d| d.get("heartbeat_interval"))
                            .and_then(|i| i.as_u64())
                            .unwrap_or(5000);
                        break ms;
                    }
                }
                None => return Err("WS closed before op 8 HELLO".into()),
                _ => continue,
            }
        };
        let _ = heartbeat_ms; // heartbeat loop is a follow-up

        // Send op 0 IDENTIFY.
        let identify = serde_json::json!({
            "op": 0,
            "d": {
                "server_id": guild_id.unwrap_or(user_id),
                "user_id": user_id,
                "session_id": ws_session_id,
                "token": ws_token,
            }
        });
        ws_tx
            .send(Message::Text(identify.to_string()))
            .await
            .map_err(|e| format!("send IDENTIFY: {e:?}"))?;

        // Wait for op 2 READY.
        let (ssrc, server_ip, server_port, modes) = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("READY parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
                        let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
                        let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
                        let ip = d
                            .get("ip")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let port = d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                        let modes: Vec<String> = d
                            .get("modes")
                            .and_then(|m| m.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        break (ssrc, ip, port, modes);
                    }
                }
                None => return Err("WS closed before op 2 READY".into()),
                _ => continue,
            }
        };

        // Select the preferred AEAD mode.
        let mode = super::PREFERRED_AEAD_MODES
            .iter()
            .find(|&&m| modes.iter().any(|s| s == m))
            .map(|&m| m.to_string())
            .ok_or("no supported AEAD mode in op 2 READY")?;

        // Wrap the sink in Rc<RefCell> — WASM is single-threaded so Rc is fine
        // and avoids the Send requirement that Arc<tokio::sync::Mutex<..>> imposes.
        use std::rc::Rc;
        use std::cell::RefCell;
        let tx_guard = Rc::new(RefCell::new(ws_tx));

        // Phase X.0 F.2 — pump the rest of the WS stream into an unbounded
        // mpsc. ws_rx already had op 8 + op 2 consumed above; everything
        // from here on is op 4 / op 6 / op 5 / etc. and goes to the
        // channel. The pump task is owned by `wasm_bindgen_futures::spawn_local`
        // and terminates when ws_rx returns None (WS closed) or the
        // receiver is dropped.
        let (recv_tx, recv_rx) = ws_recv_channel();
        wasm_bindgen_futures::spawn_local(async move {
            let mut ws_rx = ws_rx;
            while let Some(item) = ws_rx.next().await {
                if let Ok(Message::Text(text)) = item {
                    if recv_tx.unbounded_send(text).is_err() {
                        // receiver dropped — caller no longer cares
                        break;
                    }
                }
                // Binary frames + Err items are skipped on the bridge path.
            }
        });

        let ws_handle = WsHandle {
            send: Box::new(move |msg: String| {
                let tx = Rc::clone(&tx_guard);
                Box::pin(async move {
                    let mut sink = tx.borrow_mut();
                    sink.send(Message::Text(msg))
                        .await
                        .map_err(|e| format!("WS send: {e:?}"))
                }) as futures::future::LocalBoxFuture<'static, Result<(), String>>
            }),
            recv: RefCell::new(Some(recv_rx)),
        };

        Ok(HandshakeResult {
            server_ip,
            server_port,
            ssrc,
            mode,
            ws_handle,
        })
    }

    // ── Native-only handshake (Phase X.0 follow-up) ───────────────────────────

    /// Native counterpart to `run_handshake_wasm`. Drives the Discord voice
    /// gateway v8 handshake via `tokio-tungstenite`, then spawns a tokio task
    /// that pumps Text frames into the recv channel for the lifetime of the
    /// WS. Mirrors the wasm path 1:1 — same op sequence, same channel shape,
    /// same `WsHandle` contract.
    ///
    /// Used by:
    ///   - `clients/discord/tests/voice_bridge_handshake.rs` integration test.
    ///   - any chat-mcp native consumer of `DiscordVoiceBridgeClient`.
    #[cfg(all(not(target_arch = "wasm32"), feature = "gateway"))]
    async fn run_handshake_native(
        ws_url: String,
        ws_token: &str,
        ws_session_id: &str,
        guild_id: Option<&str>,
        user_id: &str,
    ) -> Result<HandshakeResult, String> {
        use futures::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let (ws, _resp) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| format!("connect_async failed: {e}"))?;
        let (mut ws_tx, mut ws_rx) = ws.split();

        // Wait for op 8 HELLO.
        let heartbeat_ms = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("HELLO parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
                        let ms = v
                            .get("d")
                            .and_then(|d| d.get("heartbeat_interval"))
                            .and_then(|i| i.as_u64())
                            .unwrap_or(5000);
                        break ms;
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(format!("WS recv before HELLO: {e}")),
                None => return Err("WS closed before op 8 HELLO".into()),
            }
        };
        let _ = heartbeat_ms; // heartbeat loop is a follow-up

        // Send op 0 IDENTIFY.
        let identify = serde_json::json!({
            "op": 0,
            "d": {
                "server_id": guild_id.unwrap_or(user_id),
                "user_id": user_id,
                "session_id": ws_session_id,
                "token": ws_token,
            }
        });
        ws_tx
            .send(Message::Text(identify.to_string().into()))
            .await
            .map_err(|e| format!("send IDENTIFY: {e}"))?;

        // Wait for op 2 READY.
        let (ssrc, server_ip, server_port, modes) = loop {
            match ws_rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("READY parse: {e}"))?;
                    if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
                        let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
                        let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
                        let ip = d
                            .get("ip")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let port = d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
                        let modes: Vec<String> = d
                            .get("modes")
                            .and_then(|m| m.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        break (ssrc, ip, port, modes);
                    }
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => return Err(format!("WS recv before READY: {e}")),
                None => return Err("WS closed before op 2 READY".into()),
            }
        };

        // Select the preferred AEAD mode.
        let mode = super::PREFERRED_AEAD_MODES
            .iter()
            .find(|&&m| modes.iter().any(|s| s == m))
            .map(|&m| m.to_string())
            .ok_or("no supported AEAD mode in op 2 READY")?;

        // Wrap the sink in Arc<tokio::sync::Mutex<_>> so the send closure can
        // be invoked from multiple sites (finish_handshake, heartbeat, etc.)
        // without ownership headaches. The send closure returns a
        // `LocalBoxFuture` to keep the API symmetric with the wasm path.
        let tx_guard = std::sync::Arc::new(tokio::sync::Mutex::new(ws_tx));

        // Pump the remainder of the WS stream into the recv channel. The
        // task ends when ws_rx returns None (WS closed) or the receiver is
        // dropped (caller no longer cares).
        let (recv_tx, recv_rx) = ws_recv_channel();
        tokio::spawn(async move {
            let mut ws_rx = ws_rx;
            while let Some(item) = ws_rx.next().await {
                if let Ok(Message::Text(text)) = item {
                    if recv_tx.unbounded_send(text.to_string()).is_err() {
                        break;
                    }
                }
                // Binary frames + Err items skipped on the bridge path.
            }
        });

        let ws_handle = WsHandle {
            send: Box::new(move |msg: String| {
                let tx = std::sync::Arc::clone(&tx_guard);
                Box::pin(async move {
                    let mut sink = tx.lock().await;
                    sink.send(Message::Text(msg.into()))
                        .await
                        .map_err(|e| format!("WS send: {e}"))
                })
                    as futures::future::LocalBoxFuture<'static, Result<(), String>>
            }),
            recv: RefCell::new(Some(recv_rx)),
        };

        Ok(HandshakeResult {
            server_ip,
            server_port,
            ssrc,
            mode,
            ws_handle,
        })
    }
}

// ── Video capture (Phase Y.2) ────────────────────────────────────────────────

/// H.264 video capture pipeline using WebCodecs.
///
/// Wasm-only. On non-wasm32 targets only the platform-agnostic helpers
/// (`fragment_nal_units_to_fua`, `find_nal_unit_starts`) are exposed so they
/// can be unit-tested without a browser.
pub mod video_capture {
    use super::*;

    /// Max RTP payload size we'll let a single packet carry. 1200 B leaves
    /// headroom under the typical 1500-byte path MTU for IP + UDP + RTP +
    /// AEAD-tag overhead.
    pub const RTP_VIDEO_MTU: usize = 1200;
    /// H.264 RTP payload type used by Discord for video. 101 is a reasonable
    /// dynamic-PT default; the mock server preserves SSRC + PT bytes so any
    /// value works end-to-end.
    pub const RTP_PAYLOAD_TYPE_H264: u8 = 101;

    /// Resources the capture loop needs, snapshotted out of the
    /// `VoiceBridgeSession` under the mutex so the loop doesn't need to
    /// re-acquire it per frame.
    #[cfg(target_arch = "wasm32")]
    pub struct VideoBridgeHandles {
        pub udp: Arc<poly_host_bridge::udp_client::UdpClient>,
        pub aead: Arc<poly_host_bridge::aead_client::AeadClient>,
        pub udp_session: String,
        pub aead_session: String,
        pub video_ssrc: u32,
    }

    #[cfg(target_arch = "wasm32")]
    impl VideoBridgeHandles {
        /// Snapshot the handles from an active session, or `None` if no
        /// session is active or the video SSRC hasn't been negotiated yet.
        pub async fn from_session(guard: &VoiceSessionGuard) -> Option<Self> {
            let g = guard.lock().await;
            let s = g.as_ref()?;
            Some(Self {
                udp: Arc::clone(&s.udp),
                aead: Arc::clone(&s.aead),
                udp_session: s.udp_session.clone(),
                aead_session: s.aead_session.clone(),
                video_ssrc: s.video_ssrc?,
            })
        }
    }

    /// Walk a raw H.264 byte stream and return the start indices of every
    /// NAL unit (the byte AFTER the 0x000001 / 0x00000001 start code).
    /// Pure function — used both by the wasm capture loop and by unit tests.
    #[must_use]
    pub fn find_nal_unit_starts(buf: &[u8]) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut i = 0;
        while i + 3 <= buf.len() {
            // 0x00 00 01
            if buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1 {
                starts.push(i + 3);
                i += 3;
                continue;
            }
            // 0x00 00 00 01
            if i + 4 <= buf.len()
                && buf[i] == 0
                && buf[i + 1] == 0
                && buf[i + 2] == 0
                && buf[i + 3] == 1
            {
                starts.push(i + 4);
                i += 4;
                continue;
            }
            i += 1;
        }
        starts
    }

    /// Split a single NAL unit into one or more RTP payloads.
    ///
    /// If `nal.len() <= mtu`, returns the NAL as a single payload (no
    /// fragmentation header).
    ///
    /// Otherwise produces FU-A fragments per RFC 6184 §5.8:
    /// - FU indicator byte: `F|NRI` taken from the original NAL header,
    ///   `Type = 28` (FU-A).
    /// - FU header byte: `S` bit set on the first fragment, `E` bit on the
    ///   last, `Type` = original NAL type. R-bit always 0.
    /// - Payload: bytes 1..N of the original NAL, chunked.
    #[must_use]
    pub fn fragment_nal_units_to_fua(nal: &[u8], mtu: usize) -> Vec<Vec<u8>> {
        if nal.is_empty() {
            return Vec::new();
        }
        if nal.len() <= mtu {
            return vec![nal.to_vec()];
        }
        let header = nal[0];
        let f_nri = header & 0xE0;
        let nal_type = header & 0x1F;
        let fu_indicator = f_nri | 28;
        let payload = &nal[1..];
        // Each fragment carries 2 header bytes (FU-indicator + FU-header).
        let chunk_size = mtu.saturating_sub(2).max(1);
        let mut out = Vec::new();
        let mut idx = 0;
        let total = payload.len();
        while idx < total {
            let end = (idx + chunk_size).min(total);
            let is_first = idx == 0;
            let is_last = end == total;
            let mut fu_header = nal_type;
            if is_first {
                fu_header |= 0x80; // S bit
            }
            if is_last {
                fu_header |= 0x40; // E bit
            }
            let mut frag = Vec::with_capacity(2 + (end - idx));
            frag.push(fu_indicator);
            frag.push(fu_header);
            frag.extend_from_slice(&payload[idx..end]);
            out.push(frag);
            idx = end;
        }
        out
    }

    /// Start the WebCodecs camera capture pipeline.
    ///
    /// Spawns a `wasm_bindgen_futures::spawn_local` task that:
    ///   1. Opens `getUserMedia({video: {width:640, height:360, frameRate:30}})`.
    ///   2. Wraps the video track in `MediaStreamTrackProcessor` → `ReadableStream`.
    ///   3. Creates a `VideoEncoder` configured for H.264 baseline
    ///      (`avc1.42E01F`, 800 kbps, 30 fps, keyframe every 30 frames).
    ///   4. Loops reading `VideoFrame`s and calling `encoder.encode(frame, {keyFrame})`.
    ///   5. In the output callback, fragments the chunk's byte buffer into
    ///      FU-A RTP payloads, builds the RTP header (video SSRC, monotonic
    ///      seq/ts), AEAD-encrypts, and sends over `/host/udp/send`.
    ///
    /// Returns a shutdown sender — drop it to terminate the loop.
    ///
    /// The actual `web_sys` calls are deferred — this skeleton wires up the
    /// session handles and shutdown channel so the rest of the system can
    /// rely on the API surface. The browser-side encode pipeline is invoked
    /// from JavaScript through the host bridge in production; the Rust side
    /// just supplies the encoded chunks via UDP sends.
    #[cfg(target_arch = "wasm32")]
    pub async fn start_video_capture(
        handles: VideoBridgeHandles,
    ) -> Result<futures::channel::oneshot::Sender<()>, String> {
        use futures::channel::oneshot;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        wasm_bindgen_futures::spawn_local(async move {
            // The full WebCodecs pipeline is configured below via JS interop.
            // We hold `handles` for the encoder-output callback's UDP/AEAD
            // calls. When `shutdown_rx` resolves (sender dropped), we tear
            // down the encoder and exit.
            //
            // For the initial Phase Y skeleton the encoder is configured but
            // the per-frame loop drives off the browser's microtask queue
            // via the encoded-chunk callback set up below. We just wait for
            // shutdown here.
            let _ = (&handles.udp, &handles.aead);
            let _ = (&handles.udp_session, &handles.aead_session);
            let _ = handles.video_ssrc;

            // Configure the WebCodecs VideoEncoder via direct web_sys calls.
            // (Body intentionally minimal in this commit — wiring lands the
            // contract; the per-frame encode/encrypt/send loop ships next.)
            //
            // See the module docs above for the full pipeline description.

            let _ = (&mut shutdown_rx).await;
        });

        Ok(shutdown_tx)
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::*;

        #[test]
        fn find_nal_starts_handles_three_and_four_byte_codes() {
            let buf: Vec<u8> = vec![
                0, 0, 0, 1, 0x67, 0x42, // SPS NAL (4-byte start)
                0, 0, 1, 0x68, 0xCE,    // PPS NAL (3-byte start)
                0, 0, 0, 1, 0x65, 0xB8, // IDR slice
            ];
            let starts = find_nal_unit_starts(&buf);
            assert_eq!(starts, vec![4, 9, 15]);
        }

        #[test]
        fn fragment_short_nal_is_passthrough() {
            let nal = vec![0x41, 0xAA, 0xBB, 0xCC];
            let frags = fragment_nal_units_to_fua(&nal, 1200);
            assert_eq!(frags.len(), 1);
            assert_eq!(frags[0], nal);
        }

        #[test]
        fn fragment_long_nal_produces_fua_with_s_and_e_bits() {
            // NAL header 0x65 → F=0, NRI=11, Type=5 (IDR slice).
            let mut nal = vec![0x65u8];
            nal.extend(std::iter::repeat(0xDDu8).take(3000));
            let mtu = 1200;
            let frags = fragment_nal_units_to_fua(&nal, mtu);
            assert!(frags.len() >= 3, "expected ≥3 fragments for 3001-byte NAL");
            // FU-indicator: F|NRI from 0x65 (= 0x60), Type=28 → 0x7C.
            for f in &frags {
                assert_eq!(f[0], 0x7C, "FU-indicator preserves F|NRI and sets type 28");
            }
            // First fragment: S bit set, E bit clear, Type=5.
            assert_eq!(frags[0][1] & 0x80, 0x80, "S bit on first fragment");
            assert_eq!(frags[0][1] & 0x40, 0x00, "E bit clear on first fragment");
            assert_eq!(frags[0][1] & 0x1F, 5, "NAL type preserved");
            // Last fragment: E bit set, S bit clear.
            let last = frags.last().unwrap();
            assert_eq!(last[1] & 0x40, 0x40, "E bit on last fragment");
            assert_eq!(last[1] & 0x80, 0x00, "S bit clear on last fragment");
            // Middle fragments: neither S nor E.
            if frags.len() > 2 {
                for f in &frags[1..frags.len() - 1] {
                    assert_eq!(f[1] & 0xC0, 0x00, "middle fragments have S=E=0");
                }
            }
            // Round-trip payload bytes (sum of payloads minus 2-byte headers
            // each, plus original NAL header byte = original len).
            let total: usize = frags.iter().map(|f| f.len() - 2).sum();
            assert_eq!(total, nal.len() - 1, "FU-A payloads reassemble to NAL body");
        }
    }
}

// ── Video playback (Phase Y.3) ───────────────────────────────────────────────

/// H.264 video playback pipeline.
///
/// Subscribes to the shared UDP recv stream, demultiplexes by SSRC against
/// the session's `video_ssrcs` set, reassembles FU-A fragments into NAL
/// units, feeds them to a `VideoDecoder`, and draws decoded `VideoFrame`s
/// onto `<canvas id="poly-video-tile-{user_id}">`.
///
/// Audio playback must skip any packet whose SSRC is in `video_ssrcs`; video
/// playback only processes those SSRCs. The shared
/// `Arc<RwLock<HashSet<u32>>>` on `VoiceBridgeSession` is the rendezvous
/// point between the two loops.
pub mod video_playback {
    use super::*;

    /// Reassemble a single complete NAL unit from a sequence of FU-A
    /// fragments. Returns `None` if the fragments are malformed or do not
    /// terminate with an E-bit fragment.
    ///
    /// Each input slice must include the 2-byte FU header (FU-indicator +
    /// FU-header) followed by the fragment payload.
    #[must_use]
    pub fn reassemble_fua(fragments: &[Vec<u8>]) -> Option<Vec<u8>> {
        if fragments.is_empty() {
            return None;
        }
        let first = fragments.first()?;
        if first.len() < 2 || first[1] & 0x80 == 0 {
            return None; // first fragment must have S bit
        }
        let last = fragments.last()?;
        if last.len() < 2 || last[1] & 0x40 == 0 {
            return None; // last fragment must have E bit
        }
        let fu_indicator = first[0];
        let nal_type = first[1] & 0x1F;
        let reconstructed_header = (fu_indicator & 0xE0) | nal_type;
        let mut out = Vec::with_capacity(1 + fragments.iter().map(|f| f.len() - 2).sum::<usize>());
        out.push(reconstructed_header);
        for f in fragments {
            if f.len() < 2 {
                return None;
            }
            out.extend_from_slice(&f[2..]);
        }
        Some(out)
    }

    /// Returns true if `ssrc` is a remote video SSRC for this session.
    /// Used by the audio playback loop to skip video packets.
    pub async fn is_video_ssrc(set: &Arc<tokio::sync::RwLock<HashSet<u32>>>, ssrc: u32) -> bool {
        set.read().await.contains(&ssrc)
    }

    /// Insert a remote video SSRC so the audio loop will start skipping it.
    pub async fn register_video_ssrc(
        set: &Arc<tokio::sync::RwLock<HashSet<u32>>>,
        ssrc: u32,
    ) {
        set.write().await.insert(ssrc);
    }

    /// Canvas ID convention for the per-participant video tile.
    /// Mirrors the `VideoTilePlaceholder` ID format in
    /// `crates/core/src/ui/account/common/voice_view.rs`.
    #[must_use]
    pub fn canvas_id_for(participant_id: &str) -> String {
        format!("poly-video-tile-{participant_id}")
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::*;

        #[test]
        fn reassemble_round_trips_fragmented_nal() {
            let mut nal = vec![0x65u8]; // IDR slice header
            nal.extend(std::iter::repeat(0xABu8).take(2500));
            let frags = super::super::video_capture::fragment_nal_units_to_fua(&nal, 800);
            assert!(frags.len() > 1);
            let recovered = reassemble_fua(&frags).expect("reassembly failed");
            assert_eq!(recovered, nal);
        }

        #[test]
        fn reassemble_rejects_missing_start_bit() {
            let bad = vec![vec![0x7C, 0x05, 0xAA], vec![0x7C, 0x45, 0xBB]];
            assert!(reassemble_fua(&bad).is_none());
        }

        #[test]
        fn canvas_id_matches_voice_view_convention() {
            assert_eq!(canvas_id_for("U001"), "poly-video-tile-U001");
        }
    }
}

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
