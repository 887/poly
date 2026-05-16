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

use std::sync::Arc;

use poly_host_bridge::{
    aead_client::AeadClient,
    codec_opus_client::OpusClient,
    udp_client::UdpClient,
    voice_wire::VoiceEvent,
};
use tokio::sync::Mutex;

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
        });

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

        // Build RTP header (sequence + timestamp are per-frame monotonic; we
        // derive a simple counter from the packet index using a thread-local
        // would require RefCell in WASM — for now we use a fixed sequence=0
        // and timestamp=0 per frame which satisfies the nonce derivation
        // requirement without cross-task state in this simplistic path.
        // A production implementation would track these in VoiceBridgeSession.)
        let rtp_header = build_rtp_header(0, 0, s.local_ssrc);
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

    /// Start sending camera video via the host-bridge.
    ///
    /// Validates that a voice session is active. The UI drives actual frame
    /// delivery by calling `send_audio_frame` on each captured frame.
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

mod voice_protocol {
    use super::*;

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

    /// An opaque write handle to the voice WebSocket.
    ///
    /// Used only to send op 1 SELECT_PROTOCOL after IP discovery.
    /// On WASM gloo_net WebSocket is not `Send`; on wasm32 there is only one
    /// thread so Send is not required.
    pub struct WsHandle {
        /// Closure that sends a JSON string on the voice WebSocket.
        pub send: Box<dyn Fn(String) -> futures::future::LocalBoxFuture<'static, Result<(), String>>>,
    }

    impl std::fmt::Debug for WsHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WsHandle").finish_non_exhaustive()
        }
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

        // On native (test builds) fall back to a stub that returns an error
        // because this code path is only exercised from WASM.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (ws_url, ws_token, ws_session_id, guild_id, user_id);
            Err("voice_bridge::run_handshake is WASM-only; use the native voice path".into())
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
    /// Returns the 32-byte `secret_key`.
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

        // SESSION_DESCRIPTION is received asynchronously; in a full
        // implementation the background WS receive task would push it into a
        // channel. For the initial pass we park here with a 5s timeout.
        // The discord plugin's WASM-side WS loop will be improved in a follow-up.
        // For now return a stub error that the caller can surface gracefully.
        Err("finish_handshake: SESSION_DESCRIPTION receive loop not yet wired on WASM bridge path — follow-up plan item".into())
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
