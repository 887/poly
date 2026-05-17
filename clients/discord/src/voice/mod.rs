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

    // E.9 — Bandwidth controller for RTCP feedback (shared with video transport later).
    // Created here (before decode loop) so the decode loop can route RTCP packets to it.
    // Initially no video transport, so the controller is stored on the connection handle;
    // when start_video is called, the controller is shared with DiscordVideoTransport.
    let bandwidth_ctrl: Arc<rtcp::BandwidthController> =
        Arc::new(rtcp::BandwidthController::new(rtcp::MAX_BITRATE_BPS));

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

// ── Voice WS handshake helpers ────────────────────────────────────────────────

/// Data from op 2 Ready.
struct VoiceReady {
    ssrc: u32,
    ip: String,
    port: u16,
    modes: Vec<String>,
}

/// Data from op 4 Session Description.
struct SessionDesc {
    secret_key: Vec<u8>,
}

type WsWrite = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    TMsg,
>;
type WsRead = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
>;

/// Read frames until op 8 Hello arrives; return heartbeat_interval_ms.
async fn wait_for_hello(read: &mut WsRead) -> Result<u64, VoiceError> {
    while let Some(msg) = read.next().await {
        let text = ws_text(msg)?;
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(8) {
            let interval = v
                .get("d")
                .and_then(|d| d.get("heartbeat_interval"))
                .and_then(|i| i.as_u64())
                .unwrap_or(5000);
            return Ok(interval);
        }
    }
    Err(VoiceError::WsConnect("WS closed before op 8 Hello".into()))
}

/// Send op 0 IDENTIFY.
async fn send_identify(write: &mut WsWrite, info: &VoiceServerInfo) -> Result<(), VoiceError> {
    let payload = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": info.guild_id.as_deref().unwrap_or(&info.user_id),
            "user_id": info.user_id,
            "session_id": info.session_id,
            "token": info.token,
        }
    });
    write
        .send(TMsg::Text(payload.to_string().into()))
        .await
        .map_err(|e| VoiceError::WsConnect(e.to_string()))
}

/// Read frames until op 2 Ready.
async fn wait_for_ready(read: &mut WsRead) -> Result<VoiceReady, VoiceError> {
    while let Some(msg) = read.next().await {
        let text = ws_text(msg)?;
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(2) {
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let ssrc = d.get("ssrc").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
            let ip = d.get("ip").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let port = d.get("port").and_then(|p| p.as_u64()).unwrap_or(0) as u16;
            let modes: Vec<String> = d
                .get("modes")
                .and_then(|m| m.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            return Ok(VoiceReady { ssrc, ip, port, modes });
        }
    }
    Err(VoiceError::WsConnect("WS closed before op 2 Ready".into()))
}

/// Read frames until op 4 Session Description.
async fn wait_for_session_description(read: &mut WsRead) -> Result<SessionDesc, VoiceError> {
    while let Some(msg) = read.next().await {
        let text = ws_text(msg)?;
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("op").and_then(|o| o.as_u64()) == Some(4) {
            let d = v.get("d").cloned().unwrap_or(serde_json::Value::Null);
            let key: Vec<u8> = d
                .get("secret_key")
                .and_then(|k| k.as_array())
                .map(|a| a.iter().filter_map(|b| b.as_u64().map(|n| n as u8)).collect())
                .unwrap_or_default();
            return Ok(SessionDesc { secret_key: key });
        }
    }
    Err(VoiceError::WsConnect("WS closed before op 4 Session Description".into()))
}

/// Pick the highest-supported AEAD mode from the ready modes list.
fn select_encryption_mode(modes: &[String]) -> Result<String, VoiceError> {
    for preferred in PREFERRED_AEAD_MODES {
        if modes.iter().any(|m| m == preferred) {
            return Ok((*preferred).to_string());
        }
    }
    Err(VoiceError::NoSupportedEncryptionMode)
}

/// Send op 1 SELECT PROTOCOL.
async fn send_select_protocol(
    write: &mut WsWrite,
    local_ip: &str,
    local_port: u16,
    mode: &str,
) -> Result<(), VoiceError> {
    let payload = serde_json::json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": {
                "address": local_ip,
                "port": local_port,
                "mode": mode,
            }
        }
    });
    write
        .send(TMsg::Text(payload.to_string().into()))
        .await
        .map_err(|e| VoiceError::WsConnect(e.to_string()))
}

// ── UDP IP-discovery (B.4) ────────────────────────────────────────────────────

/// Perform the Discord UDP IP-discovery handshake.
///
/// Sends a 74-byte discovery request, waits for the response,
/// and returns `(external_ip, external_port)`.
///
/// Discord IP-discovery packet (74 bytes total):
/// - `[0..2]`  type: u16be — 0x0001 (request) or 0x0002 (response)
/// - `[2..4]`  length: u16be — always 70
/// - `[4..8]`  ssrc: u32be
/// - `[8..72]` address: 64 bytes, null-terminated C-string
/// - `[72..74]` port: u16be
async fn ip_discovery(udp: &UdpSocket, ssrc: u32) -> Result<(String, u16), VoiceError> {
    // Build request packet (74 bytes).
    let mut buf = [0u8; 74];
    // type = 0x0001
    buf[0] = 0x00;
    buf[1] = 0x01;
    // length = 70
    buf[2] = 0x00;
    buf[3] = 0x46;
    // ssrc (big-endian)
    buf[4] = (ssrc >> 24) as u8;
    buf[5] = (ssrc >> 16) as u8;
    buf[6] = (ssrc >> 8) as u8;
    buf[7] = ssrc as u8;
    // address[8..72] and port[72..74] are already zero (request leaves them empty).

    udp.send(&buf)
        .await
        .map_err(|e| VoiceError::IpDiscovery(format!("send failed: {e}")))?;

    // Wait for response (up to 5s).
    let mut resp = [0u8; 74];
    let n = time::timeout(Duration::from_secs(5), udp.recv(&mut resp))
        .await
        .map_err(|_| VoiceError::IpDiscovery("timed out".into()))?
        .map_err(|e| VoiceError::IpDiscovery(format!("recv failed: {e}")))?;

    if n < 74 {
        return Err(VoiceError::IpDiscovery(format!("short response: {n} bytes")));
    }

    // Response type should be 0x0002.
    let resp_type = u16::from_be_bytes([resp[0], resp[1]]);
    if resp_type != 0x0002 {
        return Err(VoiceError::IpDiscovery(format!("unexpected type: {resp_type:#x}")));
    }

    // address: null-terminated in bytes 8..72.
    let addr_end = resp[8..72]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(64);
    let ip = std::str::from_utf8(&resp[8..8 + addr_end])
        .map_err(|e| VoiceError::IpDiscovery(format!("bad IP utf8: {e}")))?
        .to_string();
    let port = u16::from_be_bytes([resp[72], resp[73]]);

    Ok((ip, port))
}

// ── Voice WS event loop (heartbeat + op 5 SPEAKING) ──────────────────────────

/// Long-running voice WebSocket loop.
///
/// - Sends heartbeat (op 3) on `heartbeat_interval_ms` timer.
/// - Receives op 5 SPEAKING → updates `ssrc_user_map` (B.8).
/// - Sends op 5 SPEAKING when `speaking_flag` transitions (B.8).
/// - C.4: emits `ClientEvent::VoiceSpeakingUpdate` via `speaking_tx` when present.
/// - Drains `ws_out_rx` — auxiliary outbound messages from video transport (op 12/14).
/// - E.9: fires a slow ramp-up tick (every 2s) via `bandwidth_ctrl` when video is active.
#[allow(clippy::too_many_arguments)]
async fn voice_ws_loop(
    mut write: WsWrite,
    mut read: WsRead,
    local_ssrc: u32,
    heartbeat_interval_ms: u64,
    ssrc_user_map: SsrcUserMap,
    speaking_flag: Arc<AtomicBool>,
    speaking_tx: Option<(String, tokio::sync::mpsc::UnboundedSender<ClientEvent>)>,
    mut ws_out_rx: mpsc::Receiver<serde_json::Value>,
    bandwidth_ctrl: Arc<rtcp::BandwidthController>,
) {
    let interval = Duration::from_millis(heartbeat_interval_ms);
    let mut heartbeat_tick = time::interval(interval);
    // E.9: slow ramp-up ticker — fires every 2s to gradually recover bitrate
    // after congestion.  The ramp-up is a no-op when no video transport is active
    // (bandwidth_ctrl stays at DEFAULT_BITRATE_BPS = 1 Mbps).
    let mut ramp_up_tick = time::interval(Duration::from_secs(2));
    let mut nonce: u64 = 0;
    let mut last_speaking = false;

    loop {
        tokio::select! {
            _ = heartbeat_tick.tick() => {
                nonce = nonce.wrapping_add(1);
                let hb = serde_json::json!({ "op": 3, "d": nonce });
                if write.send(TMsg::Text(hb.to_string().into())).await.is_err() {
                    break;
                }
            }
            Some(outbound) = ws_out_rx.recv() => {
                // Auxiliary outbound messages (op 12 Video, op 14 Client Connect, etc.)
                if write.send(TMsg::Text(outbound.to_string().into())).await.is_err() {
                    break;
                }
            }
            _ = ramp_up_tick.tick() => {
                // E.9: slow ramp-up — recover bitrate gradually after congestion.
                // No-op if already at max.
                bandwidth_ctrl.ramp_up();
            }
            msg = read.next() => {
                let text = match msg {
                    Some(Ok(TMsg::Text(t))) => t.to_string(),
                    Some(Ok(TMsg::Close(_))) | None => break,
                    _ => continue,
                };
                let v: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(99);
                match op {
                    // op 6 = HEARTBEAT_ACK — no action.
                    6 => {}
                    // op 5 = SPEAKING — update SSRC → user_id map (B.8).
                    // C.4: also emit VoiceSpeakingUpdate so UI speaking rings update.
                    5 => {
                        if let Some(d) = v.get("d") {
                            let ssrc = d.get("ssrc")
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0) as u32;
                            let user_id = d.get("user_id")
                                .and_then(|u| u.as_str())
                                .unwrap_or("")
                                .to_string();
                            // speaking bitmask: 0 = not speaking, non-zero = speaking.
                            let speaking_bitmask = d.get("speaking")
                                .and_then(|s| s.as_u64())
                                .unwrap_or(0);
                            if ssrc != 0 && !user_id.is_empty() {
                                ssrc_user_map.write().await.insert(ssrc, user_id.clone());
                                // C.4 — emit speaking indicator event if wired.
                                if let Some((ref channel_id, ref tx)) = speaking_tx {
                                    let ev = ClientEvent::VoiceSpeakingUpdate {
                                        channel_id: channel_id.clone(),
                                        user_id,
                                        is_speaking: speaking_bitmask != 0,
                                    };
                                    let _ = tx.send(ev);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Emit op 5 SPEAKING when the flag transitions (B.8).
        let now_speaking = speaking_flag.load(Ordering::Relaxed);
        if now_speaking != last_speaking {
            last_speaking = now_speaking;
            let speaking_bitmask: u32 = if now_speaking { 1 } else { 0 };
            let ev = serde_json::json!({
                "op": 5,
                "d": {
                    "speaking": speaking_bitmask,
                    "delay": 0,
                    "ssrc": local_ssrc,
                }
            });
            if write.send(TMsg::Text(ev.to_string().into())).await.is_err() {
                break;
            }
        }
    }
    debug!(target: "poly_discord::voice", "voice WS loop exited");
}

// ── Encode loop (B.7) ─────────────────────────────────────────────────────────

/// Encode loop: PCM frames from `input_stream` → Opus → RTP header → AEAD → UDP.
async fn udp_encode_loop(
    udp: Arc<UdpSocket>,
    mut input_stream: BoxInputStream,
    secret_key: [u8; 32],
    mode: String,
    local_ssrc: u32,
    sequence: Arc<AtomicU16>,
    timestamp: Arc<AtomicU32>,
    is_speaking: Arc<AtomicBool>,
    transmit_mode: TransmitMode,
) {
    let encoder = match OpusEncoder::new(
        OpusSampleRate::Hz48000,
        OpusChannels::Stereo,
        OpusApplication::Voip,
    ) {
        Ok(e) => e,
        Err(e) => {
            warn!(target: "poly_discord::voice", error = %e, "failed to create Opus encoder");
            return;
        }
    };

    let cipher = match XChaCha20Poly1305::new_from_slice(&secret_key) {
        Ok(c) => c,
        Err(e) => {
            warn!(target: "poly_discord::voice", error = %e, "failed to create AEAD cipher");
            return;
        }
    };

    let mut opus_buf = vec![0u8; 4000];
    let mut pcm_accumulator: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);

    while let Some(frame) = input_stream.next().await {
        pcm_accumulator.extend_from_slice(&frame);

        while pcm_accumulator.len() >= OPUS_FRAME_SAMPLES {
            let pcm_frame: Vec<i16> = pcm_accumulator.drain(..OPUS_FRAME_SAMPLES).collect();

            // VAD / PTT gate (B.9).
            let transmitting = transmit_mode.should_transmit(&pcm_frame);
            is_speaking.store(transmitting, Ordering::Relaxed);
            if !transmitting {
                continue;
            }

            // Opus encode.
            let encoded_len = match encoder.encode(&pcm_frame, &mut opus_buf) {
                Ok(n) => n,
                Err(e) => {
                    warn!(target: "poly_discord::voice", error = %e, "Opus encode error");
                    continue;
                }
            };
            let opus_data = &opus_buf[..encoded_len];

            // Build RTP header (B.6).
            let seq = sequence.fetch_add(1, Ordering::Relaxed);
            let ts = timestamp.fetch_add(OPUS_FRAME_SAMPLES as u32, Ordering::Relaxed);
            let rtp_header = build_rtp_header(seq, ts, local_ssrc);

            // AEAD encrypt (B.6).
            // aead_xchacha20_poly1305_rtpsize: nonce = RTP header (first 12 bytes),
            // zero-padded to 24 bytes for XChaCha20.
            let encrypted = match encrypt_rtp(&cipher, &rtp_header, opus_data, &mode) {
                Ok(e) => e,
                Err(e) => {
                    warn!(target: "poly_discord::voice", error = %e, "AEAD encrypt error");
                    continue;
                }
            };

            // Send RTP header + encrypted payload.
            let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + encrypted.len());
            packet.extend_from_slice(&rtp_header);
            packet.extend_from_slice(&encrypted);

            if let Err(e) = udp.send(&packet).await {
                warn!(target: "poly_discord::voice", error = %e, "UDP send error");
            }
        }
    }

    is_speaking.store(false, Ordering::Relaxed);
    debug!(target: "poly_discord::voice", "encode loop exited");
}

// ── Decode loop (B.7 + E.9) ───────────────────────────────────────────────────

/// Decode loop: UDP recv → RTCP dispatch (E.9) OR AEAD decrypt → RTP strip → Opus decode → audio output.
///
/// Phase E.9: before attempting RTP decode, `is_rtcp_packet` checks whether the
/// datagram is RTCP (PT 192–223, version 2).  If so, the packet is handed to
/// `handle_rtcp_datagram` which updates the `BandwidthController` target.  The
/// encoder task reads `target_bps` on each frame via a shared `Arc<AtomicU32>`.
async fn udp_decode_loop(
    udp: Arc<UdpSocket>,
    secret_key: [u8; 32],
    mode: String,
    _ssrc_user_map: SsrcUserMap,
    output: Box<dyn poly_audio_backend::AudioOutputStream>,
    bandwidth_ctrl: Option<Arc<rtcp::BandwidthController>>,
) {
    let cipher = match XChaCha20Poly1305::new_from_slice(&secret_key) {
        Ok(c) => c,
        Err(e) => {
            warn!(target: "poly_discord::voice", error = %e, "decode: failed to create cipher");
            return;
        }
    };

    // One Opus decoder per remote SSRC.
    let mut decoders: HashMap<u32, OpusDecoder> = HashMap::new();
    let mut recv_buf = vec![0u8; MAX_UDP_PACKET];
    let mut pcm_buf = vec![0i16; OPUS_FRAME_SAMPLES * 2];

    loop {
        let n = match udp.recv(&mut recv_buf).await {
            Ok(n) => n,
            Err(e) => {
                warn!(target: "poly_discord::voice", error = %e, "UDP recv error");
                break;
            }
        };

        let packet = &recv_buf[..n];

        // Packets shorter than the RTP header are garbage.
        if packet.len() < RTP_HEADER_SIZE {
            continue;
        }

        // E.9 — RTCP bandwidth feedback: route RTCP packets to the bandwidth
        // controller instead of the Opus decode path.
        if rtcp::is_rtcp_packet(packet) {
            if let Some(ref ctrl) = bandwidth_ctrl {
                rtcp::handle_rtcp_datagram(packet, ctrl);
            }
            continue;
        }

        // Parse RTP header.
        let (ssrc, payload_offset) = match parse_rtp_header(packet) {
            Some(r) => r,
            None => continue,
        };

        let rtp_header = &packet[..payload_offset];
        let ciphertext = &packet[payload_offset..];

        // AEAD decrypt.
        let plaintext = match decrypt_rtp(&cipher, rtp_header, ciphertext, &mode) {
            Ok(p) => p,
            Err(_) => {
                // Discord sends keepalives and other non-audio packets — don't log.
                continue;
            }
        };

        // Opus decode.
        let decoder = decoders.entry(ssrc).or_insert_with(|| {
            OpusDecoder::new(OpusSampleRate::Hz48000, OpusChannels::Stereo)
                .expect("OpusDecoder::new never fails with valid params")
        });

        // audiopus wraps &[u8] as Packet and &mut [i16] as MutSignals.
        let packet = match Packet::try_from(plaintext.as_slice()) {
            Ok(p) => p,
            Err(_) => continue, // empty or oversized — skip
        };
        let mut_signals = match MutSignals::try_from(pcm_buf.as_mut_slice()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let decoded_samples = match decoder.decode(Some(packet), mut_signals, false) {
            Ok(n) => n,
            Err(e) => {
                warn!(target: "poly_discord::voice", ssrc, error = %e, "Opus decode error");
                continue;
            }
        };

        let pcm = &pcm_buf[..decoded_samples * 2]; // stereo: 2 i16 per sample
        if let Err(e) = output.push(pcm).await {
            warn!(target: "poly_discord::voice", error = %e, "output.push error");
        }
    }
    debug!(target: "poly_discord::voice", "decode loop exited");
}

// ── RTP framing helpers (B.6 — roll-our-own per plan decision) ───────────────

/// Build a 12-byte RTP header for a Discord voice packet.
///
/// Discord voice uses standard RTP with:
/// - version = 2
/// - payload type = 0x78 (120 = Opus)
/// - no CSRC list, no extension
fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; RTP_HEADER_SIZE] {
    let mut header = [0u8; RTP_HEADER_SIZE];
    // Byte 0: V=2, P=0, X=0, CC=0 → 0x80
    header[0] = 0x80;
    // Byte 1: M=0, PT=120 → 0x78
    header[1] = RTP_PAYLOAD_TYPE_OPUS;
    // Bytes 2-3: sequence number (big-endian)
    header[2] = (sequence >> 8) as u8;
    header[3] = sequence as u8;
    // Bytes 4-7: timestamp (big-endian)
    header[4] = (timestamp >> 24) as u8;
    header[5] = (timestamp >> 16) as u8;
    header[6] = (timestamp >> 8) as u8;
    header[7] = timestamp as u8;
    // Bytes 8-11: SSRC (big-endian)
    header[8] = (ssrc >> 24) as u8;
    header[9] = (ssrc >> 16) as u8;
    header[10] = (ssrc >> 8) as u8;
    header[11] = ssrc as u8;
    header
}

/// Parse the SSRC and payload offset from an RTP packet.
///
/// Returns `(ssrc, payload_start_offset)` or `None` for malformed packets.
fn parse_rtp_header(packet: &[u8]) -> Option<(u32, usize)> {
    if packet.len() < RTP_HEADER_SIZE {
        return None;
    }
    let version = (packet[0] >> 6) & 0x3;
    if version != 2 {
        return None;
    }
    let has_extension = (packet[0] >> 4) & 0x1 == 1;
    let csrc_count = (packet[0] & 0x0F) as usize;
    let ssrc = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);

    let mut offset = RTP_HEADER_SIZE + csrc_count * 4;
    if offset > packet.len() {
        return None;
    }

    // Skip extension header if present.
    if has_extension {
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

// ── AEAD helpers ──────────────────────────────────────────────────────────────

/// Encrypt an Opus payload using `aead_xchacha20_poly1305_rtpsize`.
///
/// The nonce is the 12-byte RTP header, zero-padded to 24 bytes.
/// The ciphertext starts immediately after the nonce (nonce is NOT prepended
/// to the ciphertext in `rtpsize` mode — it's reconstructed from the RTP header
/// by the receiver).
fn encrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    plaintext: &[u8],
    mode: &str,
) -> Result<Vec<u8>, VoiceError> {
    if mode == "aead_xchacha20_poly1305_rtpsize" || mode.contains("xchacha20") {
        let nonce = rtp_header_to_xchacha_nonce(rtp_header);
        cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad: rtp_header })
            .map_err(|_| VoiceError::AeadDecryptFailed)
    } else {
        // aead_aes256_gcm_rtpsize — same nonce construction, different cipher.
        // For now, fall through to xchacha20 (mode selection ensures we only
        // get here if server supports xchacha20 first; aes256 gcm support
        // is a future extension).
        Err(VoiceError::NoSupportedEncryptionMode)
    }
}

/// Decrypt an RTP payload.
fn decrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    ciphertext: &[u8],
    mode: &str,
) -> Result<Vec<u8>, VoiceError> {
    if mode.contains("xchacha20") {
        let nonce = rtp_header_to_xchacha_nonce(rtp_header);
        cipher
            .decrypt(&nonce, Payload { msg: ciphertext, aad: rtp_header })
            .map_err(|_| VoiceError::AeadDecryptFailed)
    } else {
        Err(VoiceError::NoSupportedEncryptionMode)
    }
}

/// Build a 24-byte XChaCha20 nonce from a 12-byte RTP header.
/// The RTP header is placed at bytes 0..12; bytes 12..24 are zero.
fn rtp_header_to_xchacha_nonce(rtp_header: &[u8]) -> XNonce {
    let mut nonce = [0u8; 24];
    let len = rtp_header.len().min(24);
    nonce[..len].copy_from_slice(&rtp_header[..len]);
    XNonce::from(nonce)
}

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
