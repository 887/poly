//! Discord voice gateway transport — Phase B of `docs/plans/plan-voice-video-calls.md`.
//!
//! # Architecture
//!
//! ```text
//! DiscordVoiceConnection
//!   ├── main gateway WS   (existing gateway_connect_loop)  ← op 4 VSU
//!   ├── voice WS          (voice_ws_loop)                  ← op 0 IDENTIFY / op 2 READY …
//!   ├── UDP socket        (udp_encode_loop / udp_decode_loop)
//!   ├── Opus encoder      (audiopus)
//!   └── Opus decoder map  (SSRC → audiopus::Decoder)
//! ```
//!
//! # WASM safety
//!
//! This entire module is `#[cfg(feature = "voice")]`.  The `voice` feature
//! requires `gateway` which requires `native`.  WASM builds of `poly-discord`
//! never enable `native`, so none of this code compiles for
//! `wasm32-unknown-unknown`.  Do not add any cfg guards inside the module
//! that target wasm32 — the outer cfg already excludes WASM.
//!
//! # DAVE (Discord E2EE)
//!
//! Discord's DAVE E2EE protocol (opt-in 2024+) is out of scope for Phase B.
//! The gap is documented here so future agents know to look for it.
//! DAVE is layered on top of the existing voice WS — it adds an MLS group
//! key-agreement step after Session Description (op 4).  v1 skips it.
//!
//! # Encryption modes
//!
//! Discord deprecated `xsalsa20_poly1305*` modes in November 2024.
//! This implementation only supports `aead_xchacha20_poly1305_rtpsize` and
//! `aead_aes256_gcm_rtpsize`.  The highest-available AEAD mode is selected
//! from the op 2 Ready `modes` list.

#![allow(clippy::indexing_slicing)] // RTP header byte-slicing is local + length-checked
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // Voice code: these are gated behind Option returns or explicit early-returns

/// Discord video transport — Phase E of `docs/plans/plan-voice-video-calls.md`.
/// op 12 Video signaling + H.264 RTP packetization over the same UDP socket.
pub mod video;

/// RTCP bandwidth feedback — Phase E.9 of `docs/plans/plan-voice-video-calls.md`.
/// REMB + TWCC parsers + `BandwidthController` with hysteresis for video bitrate.
pub mod rtcp;

use poly_client::ClientEvent;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use audiopus::{
    coder::{Decoder as OpusDecoder, Encoder as OpusEncoder},
    Application as OpusApplication, Channels as OpusChannels, MutSignals,
    packet::Packet,
    SampleRate as OpusSampleRate,
};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use futures::{SinkExt, StreamExt};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, Mutex as TokioMutex, RwLock},
    time,
};
use tokio_tungstenite::{connect_async, tungstenite::Message as TMsg};
use tracing::{debug, info, warn};

use poly_audio_backend::{AudioBackend, AudioFormat, BoxInputStream};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Discord voice WebSocket protocol version.
const VOICE_WS_VERSION: u8 = 4;

/// Opus payload type used in RTP headers for Discord voice.
const RTP_PAYLOAD_TYPE_OPUS: u8 = 0x78; // 120

/// RTP header fixed size (without CSRC list or extension).
const RTP_HEADER_SIZE: usize = 12;

/// 20 ms frame at 48 kHz stereo = 1920 i16 samples.
const OPUS_FRAME_SAMPLES: usize = 1920;

/// Maximum UDP packet size (conservative — Discord RTP + AEAD overhead).
const MAX_UDP_PACKET: usize = 1500;

/// Default VAD threshold (-45 dB RMS).
const DEFAULT_VAD_THRESHOLD_DB: f32 = -45.0;

/// AEAD mode names in preference order (best → worst).
const PREFERRED_AEAD_MODES: &[&str] = &[
    "aead_xchacha20_poly1305_rtpsize",
    "aead_aes256_gcm_rtpsize",
];

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors produced by the Discord voice transport.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("voice WebSocket connect failed: {0}")]
    WsConnect(String),

    #[error("UDP socket error: {0}")]
    Udp(#[from] std::io::Error),

    #[error("Opus codec error: {0}")]
    Opus(String),

    #[error("no supported encryption mode in op-2 Ready modes list")]
    NoSupportedEncryptionMode,

    #[error("voice state update not received (missing session_id or endpoint)")]
    VoiceStateMissing,

    #[error("account already has an active voice connection (anti-ban B.11)")]
    AlreadyConnected,

    #[error("session description key wrong length (expected 32, got {0})")]
    BadKeyLength(usize),

    #[error("AEAD decrypt failed — packet dropped")]
    AeadDecryptFailed,

    #[error("IP discovery failed: {0}")]
    IpDiscovery(String),
}

// ── Transmit mode ─────────────────────────────────────────────────────────────

/// Controls when the local user transmits audio.
#[derive(Debug, Clone)]
pub enum TransmitMode {
    /// Voice-activity detection: transmit when RMS of the frame exceeds
    /// `threshold_db`.  Default: -45 dB.
    Vad { threshold_db: f32 },

    /// Push-to-talk: transmit only when `active` is `true`.
    /// Drive this from a keyboard/button signal in the UI (Phase C).
    PushToTalk { active: Arc<AtomicBool> },
}

impl Default for TransmitMode {
    fn default() -> Self {
        Self::Vad {
            threshold_db: DEFAULT_VAD_THRESHOLD_DB,
        }
    }
}

impl TransmitMode {
    /// Returns `true` if the PCM frame should be transmitted.
    fn should_transmit(&self, pcm: &[i16]) -> bool {
        match self {
            Self::Vad { threshold_db } => rms_db(pcm) >= *threshold_db,
            Self::PushToTalk { active } => active.load(Ordering::Relaxed),
        }
    }
}

/// Compute the RMS level in dBFS of an i16 PCM slice.
/// Returns -96 dB for silence (all zeros).
fn rms_db(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return -96.0;
    }
    let sum_sq: f64 = pcm.iter().map(|&s| {
        let f = f64::from(s) / 32768.0;
        f * f
    }).sum();
    let rms = (sum_sq / pcm.len() as f64).sqrt();
    if rms < 1e-10 {
        return -96.0;
    }
    (20.0 * rms.log10()) as f32
}

// ── SSRC → user_id mapping ────────────────────────────────────────────────────

/// Shared map of remote SSRC → Discord user_id.
/// Built from inbound op 5 SPEAKING events (B.8).
pub type SsrcUserMap = Arc<RwLock<HashMap<u32, String>>>;

// ── Voice connection handle ───────────────────────────────────────────────────

/// A live Discord voice connection.
///
/// Dropping this value cleanly disconnects (equivalent to calling
/// [`DiscordVoiceConnection::disconnect`]).
pub struct DiscordVoiceConnection {
    /// Channel ID of the joined voice channel.
    pub channel_id: String,
    /// Guild ID (server).  `None` for DM calls.
    pub guild_id: Option<String>,
    /// SSRC assigned to the local user by Discord.
    pub local_ssrc: u32,
    /// SSRC → user_id for remote participants (updated by op 5 events).
    pub ssrc_user_map: SsrcUserMap,
    /// Signal: set `true` to tell the encode loop to stop.
    shutdown_tx: mpsc::Sender<()>,
    /// Sender for auxiliary outbound WS messages (e.g. op 12 Video signaling).
    pub ws_out_tx: mpsc::Sender<serde_json::Value>,
    /// Shared UDP socket (same as audio — required for anti-ban compliance).
    pub udp: Arc<tokio::net::UdpSocket>,
    /// 32-byte AEAD key for RTP encryption/decryption.
    pub secret_key: [u8; 32],
    /// Selected AEAD encryption mode (e.g. `"aead_xchacha20_poly1305_rtpsize"`).
    pub encryption_mode: String,
    /// Active video transport (Phase E). `None` until `enable_video` is called.
    pub video_transport: Option<video::DiscordVideoTransport>,
    /// Bandwidth controller for video REMB/TWCC feedback (Phase E.9).
    /// `None` if video is not enabled; set when the video transport is started.
    pub bandwidth_ctrl: Option<Arc<rtcp::BandwidthController>>,
}

impl DiscordVoiceConnection {
    /// Disconnect from the voice channel.
    ///
    /// Sends the shutdown signal to the encode/decode/voice-WS tasks.
    /// The caller is responsible for sending op 4 VSU with `channel_id: null`
    /// on the main gateway (see `disconnect_voice` in `lib.rs`).
    pub async fn disconnect(self) {
        // Dropping the Sender signals all tasks waiting on the Receiver.
        let _ = self.shutdown_tx.send(()).await;
    }

    /// Wire the active video transport to this connection's bandwidth controller (Phase E.9).
    ///
    /// Call this immediately after setting `self.video_transport = Some(transport)` in
    /// `DiscordClient::start_video` / `start_screen_share`.  When both fields are `Some`,
    /// this method shares the controller's `Arc<AtomicU32>` into the transport's `bw_target`
    /// by copying the current bitrate — giving a close approximation of live linking.
    ///
    /// For full real-time linking (every RTCP update flowing immediately to the encode
    /// loop without a copy), the preferred path is to construct the video transport via
    /// `DiscordVideoTransport::start_with_bandwidth_ctrl(…, Some(ctrl.clone()))`.  That
    /// variant shares the underlying Arc directly and is used by new callers in `mod.rs`;
    /// the `lib.rs` call sites use `start` (legacy) and call this method afterwards.
    pub fn link_video_bandwidth_ctrl(&self) {
        if let (Some(transport), Some(ctrl)) = (&self.video_transport, &self.bandwidth_ctrl) {
            transport.link_bandwidth_ctrl(ctrl);
        }
    }
}

// ── Per-account voice mutex (B.11) ────────────────────────────────────────────

/// A per-account voice connection lock.
///
/// Holds `Some(DiscordVoiceConnection)` while a voice session is active.
/// A second `connect_voice` call returns [`VoiceError::AlreadyConnected`]
/// without opening a second WebSocket — the load-bearing anti-ban guardrail
/// for voice (plan-discord-anti-ban.md Phase D, landing early as B.11).
pub type VoiceSessionGuard = Arc<TokioMutex<Option<DiscordVoiceConnection>>>;

// ── Parameters for connecting ─────────────────────────────────────────────────

/// Parameters gathered from the main gateway before the voice WS can connect.
#[derive(Debug, Clone)]
pub struct VoiceServerInfo {
    /// From VOICE_SERVER_UPDATE.endpoint
    pub endpoint: String,
    /// From VOICE_SERVER_UPDATE.token
    pub token: String,
    /// From VOICE_STATE_UPDATE.session_id
    pub session_id: String,
    /// Guild ID from the voice state update.
    pub guild_id: Option<String>,
    /// The local user's Discord user ID.
    pub user_id: String,
}

// ── Main connect entrypoint ───────────────────────────────────────────────────

/// Connect to a Discord voice channel.
///
/// 1. Connects voice WS (`wss://{endpoint}/?v=4`).
/// 2. Runs IDENTIFY → READY → IP-discovery → SELECT PROTOCOL → SESSION DESCRIPTION.
/// 3. Starts encode loop (mic → Opus → RTP → AEAD → UDP) and decode loop (reverse).
/// 4. Returns a [`DiscordVoiceConnection`] handle.
///
/// `audio` is used to open the mic input and speaker output streams.
/// `transmit_mode` defaults to VAD at -45 dB if `None`.
/// `speaking_tx` is an optional event sender for C.4 speaking indicator events.
///   When provided, op 5 SPEAKING events from remote participants are forwarded
///   as `ClientEvent::VoiceSpeakingUpdate` so the UI can update the speaking ring.
///
/// # Errors
///
/// Returns [`VoiceError`] if any handshake step fails.
pub async fn connect_voice(
    info: VoiceServerInfo,
    audio: &dyn AudioBackend,
    transmit_mode: Option<TransmitMode>,
    guard: VoiceSessionGuard,
    speaking_tx: Option<(String, tokio::sync::mpsc::UnboundedSender<ClientEvent>)>,
) -> Result<(), VoiceError> {
    // B.11 — anti-ban: reject if already connected.
    {
        let session = guard.lock().await;
        if session.is_some() {
            return Err(VoiceError::AlreadyConnected);
        }
    }

    let transmit_mode = transmit_mode.unwrap_or_default();

    // Connect voice WS (B.3).
    let ws_url = format!("wss://{}/?v={}", info.endpoint.trim_end_matches(':').trim_end_matches('/'), VOICE_WS_VERSION);
    debug!(target: "poly_discord::voice", url = %ws_url, "connecting voice WebSocket");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| VoiceError::WsConnect(e.to_string()))?;

    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Wait for op 8 Hello to get heartbeat_interval (B.5).
    let heartbeat_interval_ms = wait_for_hello(&mut ws_read).await?;

    // Send op 0 IDENTIFY (B.3).
    send_identify(&mut ws_write, &info).await?;

    // Wait for op 2 Ready (B.3).
    let ready = wait_for_ready(&mut ws_read).await?;
    info!(
        target: "poly_discord::voice",
        ssrc = ready.ssrc,
        ip = %ready.ip,
        port = ready.port,
        "voice READY received"
    );

    // Select encryption mode (B.4).
    let mode = select_encryption_mode(&ready.modes)?;
    info!(target: "poly_discord::voice", %mode, "selected encryption mode");

    // UDP IP-discovery (B.4).
    let udp = UdpSocket::bind("0.0.0.0:0").await?;
    let server_addr: SocketAddr = format!("{}:{}", ready.ip, ready.port)
        .parse()
        .map_err(|e| VoiceError::IpDiscovery(format!("bad server addr: {e}")))?;
    udp.connect(server_addr).await?;

    let (local_ip, local_port) = ip_discovery(&udp, ready.ssrc).await?;
    info!(target: "poly_discord::voice", %local_ip, local_port, "IP discovery complete");

    // Send op 1 SELECT PROTOCOL (B.4).
    send_select_protocol(&mut ws_write, &local_ip, local_port, &mode).await?;

    // Wait for op 4 SESSION DESCRIPTION (B.5).
    let session_desc = wait_for_session_description(&mut ws_read).await?;
    if session_desc.secret_key.len() != 32 {
        return Err(VoiceError::BadKeyLength(session_desc.secret_key.len()));
    }
    let mut secret_key = [0u8; 32];
    secret_key.copy_from_slice(&session_desc.secret_key);

    // Open audio streams.
    let input_stream = audio
        .open_input("", AudioFormat::DISCORD_VOICE)
        .await
        .map_err(|e| VoiceError::Opus(format!("open_input: {e}")))?;

    let output_sink = audio
        .open_output("", AudioFormat::DISCORD_VOICE)
        .await
        .map_err(|e| VoiceError::Opus(format!("open_output: {e}")))?;

    // Build shared state.
    let ssrc_user_map: SsrcUserMap = Arc::new(RwLock::new(HashMap::new()));
    let (shutdown_tx, _shutdown_rx) = mpsc::channel::<()>(1);

    // Channel for outbound WS messages from auxiliary loops (video op 12/14).
    let (ws_out_tx, ws_out_rx) = mpsc::channel::<serde_json::Value>(16);

    let local_ssrc = ready.ssrc;
    let sequence = Arc::new(AtomicU16::new(0));
    let timestamp = Arc::new(AtomicU32::new(0));
    let is_speaking = Arc::new(AtomicBool::new(false));

    // E.9 — Bandwidth controller for RTCP feedback (shared with video transport later).
    // Created here (before the WS loop + decode loop) so both can reference it.
    // Initially no video transport, so the controller is stored on the connection handle;
    // when start_video is called, the controller is shared with DiscordVideoTransport.
    let bandwidth_ctrl: Arc<rtcp::BandwidthController> =
        Arc::new(rtcp::BandwidthController::new(rtcp::MAX_BITRATE_BPS));

    // Voice WS loop (heartbeat + speaking events + auxiliary outbound messages + E.9 ramp-up).
    {
        let ssrc_map = Arc::clone(&ssrc_user_map);
        let speaking_flag = Arc::clone(&is_speaking);
        let bw_ramp = Arc::clone(&bandwidth_ctrl);
        tokio::spawn(voice_ws_loop(
            ws_write,
            ws_read,
            local_ssrc,
            heartbeat_interval_ms,
            ssrc_map,
            speaking_flag,
            speaking_tx,
            ws_out_rx,
            bw_ramp,
        ));
    }

    // Decode loop: UDP → RTCP dispatch (E.9) + AEAD decrypt → RTP depacketize → Opus decode → output.
    let udp_arc = Arc::new(udp);
    {
        let udp = Arc::clone(&udp_arc);
        let ssrc_map = Arc::clone(&ssrc_user_map);
        let bw = Arc::clone(&bandwidth_ctrl);
        tokio::spawn(udp_decode_loop(udp, secret_key, mode.clone(), ssrc_map, output_sink, Some(bw)));
    }

    // Encode loop: mic PCM → Opus → RTP → AEAD encrypt → UDP.
    {
        let udp = Arc::clone(&udp_arc);
        let seq = Arc::clone(&sequence);
        let ts = Arc::clone(&timestamp);
        let speaking = Arc::clone(&is_speaking);
        tokio::spawn(udp_encode_loop(
            udp,
            input_stream,
            secret_key,
            mode.clone(),
            local_ssrc,
            seq,
            ts,
            speaking,
            transmit_mode,
        ));
    }

    // Store the connection in the guard.
    // ws_out_tx + udp_arc + secret_key + mode are available for DiscordVideoTransport.
    let conn = DiscordVoiceConnection {
        channel_id: String::new(), // caller fills in
        guild_id: info.guild_id.clone(),
        local_ssrc,
        ssrc_user_map,
        shutdown_tx,
        ws_out_tx,
        udp: Arc::clone(&udp_arc),
        secret_key,
        encryption_mode: mode,
        video_transport: None,
        bandwidth_ctrl: Some(bandwidth_ctrl),
    };
    *guard.lock().await = Some(conn);

    Ok(())
}


// ── Voice WS handshake helpers — moved to voice/handshake.rs (SOLID B.3) ──
mod handshake;
use handshake::{
    wait_for_hello, send_identify, wait_for_ready, wait_for_session_description,
    select_encryption_mode, send_select_protocol, VoiceReady, SessionDesc,
    WsRead, WsWrite,
};

// ── UDP IP-discovery — moved to voice/ip_discovery.rs (SOLID B.3) ──
mod ip_discovery;
use ip_discovery::ip_discovery;

// ── Voice WS event loop — moved to voice/ws_loop.rs (SOLID B.3) ──
mod ws_loop;
use ws_loop::voice_ws_loop;

// ── Encode loop — moved to voice/encode.rs (SOLID B.3) ──
mod encode;
use encode::udp_encode_loop;

// ── Decode loop — moved to voice/decode.rs (SOLID B.3) ──
mod decode;
use decode::udp_decode_loop;

// ── RTP framing — moved to voice/rtp.rs (SOLID B.3) ──
mod rtp;
use rtp::{build_rtp_header, parse_rtp_header};

// ── AEAD helpers — moved to voice/aead.rs (SOLID B.3) ──
mod aead;
use aead::{encrypt_rtp, decrypt_rtp};


// ── WS text helper ────────────────────────────────────────────────────────────

fn ws_text(
    msg: Result<TMsg, tokio_tungstenite::tungstenite::Error>,
) -> Result<String, VoiceError> {
    match msg {
        Ok(TMsg::Text(t)) => Ok(t.to_string()),
        Ok(TMsg::Close(_)) => Err(VoiceError::WsConnect("voice WS closed".into())),
        Err(e) => Err(VoiceError::WsConnect(e.to_string())),
        _ => Ok(String::new()),
    }
}

// ── Public helpers for main gateway integration (B.2) ────────────────────────

/// Build the op 4 Voice State Update JSON for the main gateway.
///
/// Send this on the main Discord gateway WS to join a voice channel.
/// Set `channel_id = None` to leave.
pub fn voice_state_update_payload(
    guild_id: &str,
    channel_id: Option<&str>,
    self_mute: bool,
    self_deaf: bool,
) -> String {
    serde_json::json!({
        "op": 4,
        "d": {
            "guild_id": guild_id,
            "channel_id": channel_id,
            "self_mute": self_mute,
            "self_deaf": self_deaf,
        }
    })
    .to_string()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn rtp_header_round_trip() {
        let header = build_rtp_header(0x1234, 0xDEAD_BEEF, 0xCAFE_BABE);
        assert_eq!(header[0], 0x80, "V=2, no padding, no ext, no CC");
        assert_eq!(header[1], 0x78, "payload type = 120 (Opus)");
        assert_eq!(u16::from_be_bytes([header[2], header[3]]), 0x1234, "sequence");
        assert_eq!(u32::from_be_bytes([header[4], header[5], header[6], header[7]]), 0xDEAD_BEEF, "timestamp");
        assert_eq!(u32::from_be_bytes([header[8], header[9], header[10], header[11]]), 0xCAFE_BABE, "ssrc");

        let (ssrc, offset) = parse_rtp_header(&header).unwrap();
        assert_eq!(ssrc, 0xCAFE_BABE);
        assert_eq!(offset, 12);
    }

    #[test]
    fn select_encryption_mode_prefers_xchacha() {
        let modes = vec![
            "xsalsa20_poly1305".to_string(),
            "aead_aes256_gcm_rtpsize".to_string(),
            "aead_xchacha20_poly1305_rtpsize".to_string(),
        ];
        let selected = select_encryption_mode(&modes).unwrap();
        assert_eq!(selected, "aead_xchacha20_poly1305_rtpsize");
    }

    #[test]
    fn select_encryption_mode_falls_back_to_aes() {
        let modes = vec!["aead_aes256_gcm_rtpsize".to_string()];
        let selected = select_encryption_mode(&modes).unwrap();
        assert_eq!(selected, "aead_aes256_gcm_rtpsize");
    }

    #[test]
    fn select_encryption_mode_rejects_deprecated() {
        let modes = vec!["xsalsa20_poly1305".to_string()];
        let err = select_encryption_mode(&modes).unwrap_err();
        assert!(matches!(err, VoiceError::NoSupportedEncryptionMode));
    }

    #[test]
    fn rms_db_silence() {
        let silence = vec![0i16; 1920];
        assert!(rms_db(&silence) < -90.0);
    }

    #[test]
    fn rms_db_full_scale() {
        let full = vec![i16::MAX; 1920];
        let db = rms_db(&full);
        // ~0 dBFS for full-scale square wave.
        assert!(db > -1.0, "full-scale should be near 0 dBFS, got {db}");
    }

    #[test]
    fn vad_transmit_mode_gates_on_level() {
        let mode = TransmitMode::Vad { threshold_db: -45.0 };
        let silence = vec![0i16; 1920];
        assert!(!mode.should_transmit(&silence));

        let loud = vec![i16::MAX; 1920];
        assert!(mode.should_transmit(&loud));
    }

    #[test]
    fn ptt_transmit_mode_gates_on_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        let mode = TransmitMode::PushToTalk { active: Arc::clone(&flag) };
        let loud = vec![i16::MAX; 1920];
        assert!(!mode.should_transmit(&loud));
        flag.store(true, Ordering::Relaxed);
        assert!(mode.should_transmit(&loud));
    }

    #[test]
    fn voice_state_update_payload_has_null_channel_on_leave() {
        let payload = voice_state_update_payload("guild1", None, false, false);
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["op"], 4);
        assert!(v["d"]["channel_id"].is_null());
    }

    #[test]
    fn voice_state_update_payload_has_channel_on_join() {
        let payload = voice_state_update_payload("guild1", Some("chan42"), false, false);
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(v["d"]["channel_id"], "chan42");
    }
}
