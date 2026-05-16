//! Wasm-only audio playback loop (Phase X.3 of plan-voice-media-plane-e2e).
//!
//! For each connected call:
//! - Opens `/host/udp/recv_stream/{udp_session}` SSE via
//!   `UdpClient::recv_stream_boxed`.
//! - Each datagram: parse RTP header (12 bytes) → extract SSRC + sequence +
//!   timestamp → derive nonce per AEAD mode → AEAD decrypt with AAD = RTP
//!   header → Opus decode → push PCM into per-SSRC AudioContext queue.
//! - Per-SSRC playback: each unique SSRC gets its own `AudioContext` plus a
//!   chained `AudioBufferSourceNode` per 20ms frame, scheduled gap-free via
//!   `start_at(next_start_time)`.
//! - Sends `RemoteSpeakingEvent` on the provided mpsc whenever a decoded
//!   frame's RMS exceeds the speaking threshold so the UI can light up
//!   speaking indicators.
//!
//! Wired into `DiscordVoiceBridgeClient::start_audio_playback` which is
//! invoked automatically at the end of `connect_voice` so playback runs as
//! soon as the call connects. The capture loop (Phase X.2) stays opt-in.
//!
//! ## Compile-target gating
//!
//! The runtime pipeline is `#[cfg(target_arch = "wasm32")]` because it
//! depends on `web-sys` AudioContext. The pure helpers (RTP parsing, nonce
//! derivation, RMS dB, i16 → f32) are compiled on all targets so they can
//! be unit-tested natively.

#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::HashMap;
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
use poly_host_bridge::{
    aead_client::AeadClient, codec_opus_client::OpusClient, udp_client::UdpClient,
};

// ── Public event type ─────────────────────────────────────────────────────────

/// Decoded-audio energy notification — emitted on every Opus frame that
/// crosses the speaking-indicator threshold so the UI layer can map it to
/// `ClientEvent::VoiceSpeakingUpdate { user_id, ssrc, rms_db }`.
#[derive(Debug, Clone)]
pub struct RemoteSpeakingEvent {
    pub user_id: String,
    pub ssrc: u32,
    pub rms_db: f32,
}

// ── Constants ────────────────────────────────────────────────────────────────

/// RTP header size (no CSRCs, no extension).
const RTP_HEADER_SIZE: usize = 12;
/// Per-frame samples per channel @ 48 kHz, 20 ms.
const FRAME_SAMPLES_PER_CHANNEL: usize = 960;
/// Opus frame duration (s).
const FRAME_DURATION_S: f64 = 0.020;
/// Per-SSRC jitter buffer prefix on first packet (s).
const JITTER_PREFIX_S: f64 = 0.060;
/// Sample rate (Hz).
const SAMPLE_RATE_HZ: f32 = 48_000.0;
/// RMS-dB threshold above which a frame counts as "speaking".
const SPEAKING_THRESHOLD_DB: f32 = -45.0;

// ── Pure helpers (testable on native) ────────────────────────────────────────

/// Parsed RTP header — sequence/timestamp/SSRC plus payload offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtpHeaderInfo {
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    /// Offset of the payload (ciphertext) inside the original packet, after
    /// any CSRCs / extension header are accounted for.
    pub payload_offset: usize,
}

/// Parse the leading RTP header bytes of an incoming voice packet.
///
/// Validates version=2. Walks past CSRCs (4 bytes each) and any extension
/// header. Returns `None` on malformed / too-short input.
#[must_use]
pub fn parse_rtp_header(packet: &[u8]) -> Option<RtpHeaderInfo> {
    if packet.len() < RTP_HEADER_SIZE {
        return None;
    }
    if (packet[0] >> 6) & 0x3 != 2 {
        return None;
    }
    let has_ext = (packet[0] >> 4) & 0x1 == 1;
    let csrc_count = (packet[0] & 0x0F) as usize;
    let sequence = u16::from_be_bytes([packet[2], packet[3]]);
    let timestamp =
        u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
    let ssrc =
        u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let mut offset = RTP_HEADER_SIZE + csrc_count * 4;
    if offset > packet.len() {
        return None;
    }
    if has_ext {
        if offset + 4 > packet.len() {
            return None;
        }
        let ext_len = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]])
            as usize;
        offset += 4 + ext_len * 4;
    }
    if offset > packet.len() {
        return None;
    }
    Some(RtpHeaderInfo { sequence, timestamp, ssrc, payload_offset: offset })
}

/// AEAD-mode-specific nonce derivation from the RTP header bytes.
///
/// - `aead_xchacha20_poly1305_rtpsize`: 24-byte nonce = RTP header zero-padded
///   to 24 bytes.
/// - `aead_aes256_gcm_rtpsize`: 12-byte nonce = first 12 bytes of the RTP
///   header.
///
/// Unrecognised modes fall back to the XChaCha20 pattern (caller is expected
/// to negotiate one of the two supported modes during the WS handshake).
#[must_use]
pub fn derive_nonce(mode: &str, rtp_header: &[u8]) -> Vec<u8> {
    if mode == "aead_aes256_gcm_rtpsize" {
        let mut nonce = [0u8; 12];
        let len = rtp_header.len().min(12);
        nonce[..len].copy_from_slice(&rtp_header[..len]);
        return nonce.to_vec();
    }
    // XChaCha20-Poly1305 path (default / RTP-size).
    let mut nonce = [0u8; 24];
    let len = rtp_header.len().min(24);
    nonce[..len].copy_from_slice(&rtp_header[..len]);
    nonce.to_vec()
}

/// Convert interleaved stereo i16 PCM to f32 normalised to `[-1.0, 1.0]`.
///
/// Returns a single interleaved Vec<f32> the same length as the input. The
/// per-channel split for `AudioBuffer::copy_to_channel` is done by the
/// playback loop separately.
#[must_use]
pub fn i16_to_f32(pcm: &[i16]) -> Vec<f32> {
    pcm.iter().map(|s| f32::from(*s) / 32_768.0).collect()
}

/// Compute the RMS amplitude of an i16 PCM block expressed in dBFS.
///
/// Returns `f32::NEG_INFINITY` for a digitally-silent frame.
#[must_use]
pub fn rms_db_i16(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return f32::NEG_INFINITY;
    }
    let mut sumsq: f64 = 0.0;
    for &s in pcm {
        let v = f64::from(s) / 32_768.0;
        sumsq += v * v;
    }
    let rms = (sumsq / pcm.len() as f64).sqrt();
    if rms <= 0.0 {
        return f32::NEG_INFINITY;
    }
    (20.0 * rms.log10()) as f32
}

// ── Public spawn entry point ─────────────────────────────────────────────────

/// Spawn the playback loop. Returns the shutdown sender so the caller can
/// store it on `VoiceBridgeSession.playback_shutdown`.
///
/// The returned `oneshot::Sender<()>` is the shutdown signal — dropping it
/// makes the receiver fire `Canceled`, the loop observes that and exits.
/// On wasm32 the loop is spawned via `wasm_bindgen_futures::spawn_local`.
///
/// `local_ssrc` is the SSRC assigned to *us* by op 2 READY — incoming
/// packets bearing it are dropped so we never play back our own mic.
///
/// `on_remote_speaking` receives a `RemoteSpeakingEvent` for every decoded
/// frame whose RMS exceeds the speaking threshold (`-45 dB`). The mpsc
/// pattern was chosen over a `Signal<HashMap<…>>` to keep this crate free
/// of any `dioxus` / `crates/core` UI dependency — the consumer (e.g. the
/// core voice-event router) maps it to `ClientEvent::VoiceSpeakingUpdate`.
///
/// `_aead_mode` is currently unused — nonce derivation defaults to
/// `xchacha20_poly1305_rtpsize` which is the mode the bridge handshake
/// negotiates today. The parameter is reserved so callers can pass the
/// negotiated mode through once `aes256_gcm_rtpsize` ships end-to-end.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub async fn start_audio_playback(
    udp: Arc<UdpClient>,
    opus: Arc<OpusClient>,
    aead: Arc<AeadClient>,
    udp_session: String,
    decoder_session: String,
    aead_session: String,
    ssrc_to_user: Arc<tokio::sync::RwLock<HashMap<u32, String>>>,
    local_ssrc: u32,
    on_remote_speaking: futures::channel::mpsc::UnboundedSender<RemoteSpeakingEvent>,
) -> Result<futures::channel::oneshot::Sender<()>, String> {
    use futures::StreamExt;

    let (shutdown_tx, mut shutdown_rx) = futures::channel::oneshot::channel::<()>();

    let task = async move {
        let mut dgram_stream = udp.recv_stream_boxed(udp_session);
        let mut per_ssrc: HashMap<u32, SsrcPlayback> = HashMap::new();

        loop {
            // Race the next datagram against the shutdown signal so we can
            // exit promptly when disconnect_voice drops its end of the
            // oneshot.
            use futures::future::{select, Either};
            let next_dgram = dgram_stream.next();
            futures::pin_mut!(next_dgram);
            match select(&mut shutdown_rx, next_dgram).await {
                Either::Left(_) => break, // shutdown signalled
                Either::Right((None, _)) => break, // SSE stream ended
                Either::Right((Some(dgram), _)) => {
                    if let Err(e) = handle_datagram(
                        &dgram,
                        &aead,
                        &opus,
                        &aead_session,
                        &decoder_session,
                        local_ssrc,
                        &ssrc_to_user,
                        &mut per_ssrc,
                        &on_remote_speaking,
                    )
                    .await
                    {
                        tracing::trace!(
                            target: "poly_discord::voice_bridge::audio_playback",
                            error = %e,
                            "datagram dropped"
                        );
                    }
                }
            }
        }

        // Tear down all per-SSRC AudioContexts on exit.
        for (_ssrc, mut pb) in per_ssrc.drain() {
            pb.close();
        }
    };

    wasm_bindgen_futures::spawn_local(task);
    Ok(shutdown_tx)
}

/// Native stub — Phase X.3 ships the WASM playback path only. The native
/// voice path (in `clients/discord/src/voice/`) drives playback through
/// CPAL directly without the host-bridge primitives, so this entry point
/// is unused on native and returns an error to make accidental calls loud.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub async fn start_audio_playback(
    _udp: Arc<poly_host_bridge::udp_client::UdpClient>,
    _opus: Arc<poly_host_bridge::codec_opus_client::OpusClient>,
    _aead: Arc<poly_host_bridge::aead_client::AeadClient>,
    _udp_session: String,
    _decoder_session: String,
    _aead_session: String,
    _ssrc_to_user: Arc<tokio::sync::RwLock<HashMap<u32, String>>>,
    _local_ssrc: u32,
    _on_remote_speaking: futures::channel::mpsc::UnboundedSender<RemoteSpeakingEvent>,
) -> Result<futures::channel::oneshot::Sender<()>, String> {
    Err("audio_playback::start_audio_playback is WASM-only; native uses CPAL".into())
}

// ── Per-SSRC AudioContext bookkeeping (WASM only) ────────────────────────────

#[cfg(target_arch = "wasm32")]
struct SsrcPlayback {
    ctx: web_sys::AudioContext,
    /// AudioContext clock time at which the next 20ms frame should start.
    /// Initialised to `ctx.currentTime() + JITTER_PREFIX_S` on first frame.
    next_start_time: f64,
}

#[cfg(target_arch = "wasm32")]
impl SsrcPlayback {
    fn new() -> Result<Self, String> {
        let ctx = web_sys::AudioContext::new()
            .map_err(|e| format!("AudioContext::new failed: {e:?}"))?;
        let initial = ctx.current_time() + JITTER_PREFIX_S;
        Ok(Self { ctx, next_start_time: initial })
    }

    /// Schedule a 20ms PCM frame for gap-free playback.
    ///
    /// `pcm_interleaved` is interleaved stereo f32, length =
    /// `FRAME_SAMPLES_PER_CHANNEL * 2`.
    fn schedule_frame(&mut self, pcm_interleaved: &[f32]) -> Result<(), String> {
        debug_assert_eq!(pcm_interleaved.len(), FRAME_SAMPLES_PER_CHANNEL * 2);

        // Build a stereo AudioBuffer.
        let buffer = self
            .ctx
            .create_buffer(2, FRAME_SAMPLES_PER_CHANNEL as u32, SAMPLE_RATE_HZ)
            .map_err(|e| format!("create_buffer failed: {e:?}"))?;

        // De-interleave into per-channel f32 slices.
        let mut left = Vec::with_capacity(FRAME_SAMPLES_PER_CHANNEL);
        let mut right = Vec::with_capacity(FRAME_SAMPLES_PER_CHANNEL);
        for pair in pcm_interleaved.chunks_exact(2) {
            left.push(pair[0]);
            right.push(pair[1]);
        }

        buffer
            .copy_to_channel(&mut left, 0)
            .map_err(|e| format!("copy_to_channel(L) failed: {e:?}"))?;
        buffer
            .copy_to_channel(&mut right, 1)
            .map_err(|e| format!("copy_to_channel(R) failed: {e:?}"))?;

        let source = self
            .ctx
            .create_buffer_source()
            .map_err(|e| format!("create_buffer_source failed: {e:?}"))?;
        source.set_buffer(Some(&buffer));
        let dest = self.ctx.destination();
        let node: &web_sys::AudioNode = source.as_ref();
        node.connect_with_audio_node(dest.as_ref())
            .map_err(|e| format!("connect_with_audio_node failed: {e:?}"))?;

        // Clamp next_start_time to the current clock — if we fell behind
        // (e.g. ran out of buffered audio for >60ms), restart the jitter
        // prefix instead of scheduling in the past.
        let now = self.ctx.current_time();
        if self.next_start_time < now {
            self.next_start_time = now + JITTER_PREFIX_S;
        }

        source
            .start_with_when(self.next_start_time)
            .map_err(|e| format!("start_with_when failed: {e:?}"))?;
        self.next_start_time += FRAME_DURATION_S;
        Ok(())
    }

    fn close(&mut self) {
        // Best-effort: close() returns a Promise we don't need to await on
        // teardown — the AudioContext will be GC'd once it leaves scope.
        let _ = self.ctx.close();
    }
}

// ── Datagram handler ─────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
async fn handle_datagram(
    dgram: &poly_host_bridge::udp_client::UdpDatagram,
    aead: &Arc<AeadClient>,
    opus: &Arc<OpusClient>,
    aead_session: &str,
    decoder_session: &str,
    local_ssrc: u32,
    ssrc_to_user: &Arc<tokio::sync::RwLock<HashMap<u32, String>>>,
    per_ssrc: &mut HashMap<u32, SsrcPlayback>,
    on_remote_speaking: &futures::channel::mpsc::UnboundedSender<RemoteSpeakingEvent>,
) -> Result<(), String> {
    use base64::Engine as _;
    let packet = base64::engine::general_purpose::STANDARD
        .decode(dgram.data.as_bytes())
        .map_err(|e| format!("base64 decode: {e}"))?;

    let info = parse_rtp_header(&packet).ok_or("bad RTP header")?;
    if info.ssrc == local_ssrc {
        // Defensive — should not happen post-fan-out, but drop our own
        // packets if the mock bounces them back.
        return Ok(());
    }

    let rtp_header = &packet[..info.payload_offset];
    let ciphertext = &packet[info.payload_offset..];
    let nonce = derive_nonce("aead_xchacha20_poly1305_rtpsize", rtp_header);

    let plaintext = aead
        .decrypt(aead_session, &nonce, ciphertext, Some(rtp_header))
        .await
        .map_err(|e| format!("AEAD decrypt: {e}"))?;

    let pcm_i16 = opus
        .decode(decoder_session, &plaintext)
        .await
        .map_err(|e| format!("Opus decode: {e}"))?;

    // Speaking indicator.
    let rms = rms_db_i16(&pcm_i16);
    if rms > SPEAKING_THRESHOLD_DB {
        let user_id = ssrc_to_user
            .read()
            .await
            .get(&info.ssrc)
            .cloned()
            .unwrap_or_else(|| format!("user_{}", info.ssrc));
        let _ = on_remote_speaking.unbounded_send(RemoteSpeakingEvent {
            user_id,
            ssrc: info.ssrc,
            rms_db: rms,
        });
    }

    // Convert and enqueue.
    let pcm_f32 = i16_to_f32(&pcm_i16);
    let playback = match per_ssrc.get_mut(&info.ssrc) {
        Some(p) => p,
        None => {
            let pb = SsrcPlayback::new()?;
            per_ssrc.insert(info.ssrc, pb);
            per_ssrc
                .get_mut(&info.ssrc)
                .expect("just inserted above")
        }
    };
    playback.schedule_frame(&pcm_f32)?;
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn build_rtp_header(sequence: u16, timestamp: u32, ssrc: u32) -> [u8; 12] {
        let mut h = [0u8; 12];
        h[0] = 0x80;
        h[1] = 0x78;
        h[2..4].copy_from_slice(&sequence.to_be_bytes());
        h[4..8].copy_from_slice(&timestamp.to_be_bytes());
        h[8..12].copy_from_slice(&ssrc.to_be_bytes());
        h
    }

    #[test]
    fn parse_rtp_header_extracts_fields() {
        let h = build_rtp_header(0x1234, 0xDEAD_BEEF, 0xCAFE_BABE);
        let info = parse_rtp_header(&h).unwrap();
        assert_eq!(info.sequence, 0x1234);
        assert_eq!(info.timestamp, 0xDEAD_BEEF);
        assert_eq!(info.ssrc, 0xCAFE_BABE);
        assert_eq!(info.payload_offset, 12);
    }

    #[test]
    fn parse_rtp_header_rejects_short() {
        assert!(parse_rtp_header(&[0u8; 5]).is_none());
    }

    #[test]
    fn parse_rtp_header_rejects_wrong_version() {
        let mut h = build_rtp_header(0, 0, 0);
        h[0] = 0x00; // version = 0
        assert!(parse_rtp_header(&h).is_none());
    }

    #[test]
    fn parse_rtp_header_skips_csrcs() {
        // version=2, CSRC count = 2 → extra 8 bytes of header
        let mut h = vec![0x82u8, 0x78, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3];
        h.extend_from_slice(&[0u8; 8]); // two CSRCs
        h.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // payload
        let info = parse_rtp_header(&h).unwrap();
        assert_eq!(info.ssrc, 3);
        assert_eq!(info.payload_offset, 12 + 8);
    }

    #[test]
    fn derive_nonce_xchacha_pads_to_24() {
        let h = build_rtp_header(1, 2, 3);
        let nonce = derive_nonce("aead_xchacha20_poly1305_rtpsize", &h);
        assert_eq!(nonce.len(), 24);
        assert_eq!(&nonce[..12], &h[..]);
        assert!(nonce[12..].iter().all(|&b| b == 0));
    }

    #[test]
    fn derive_nonce_aes256_gcm_is_12_bytes() {
        let h = build_rtp_header(1, 2, 3);
        let nonce = derive_nonce("aead_aes256_gcm_rtpsize", &h);
        assert_eq!(nonce.len(), 12);
        assert_eq!(&nonce[..], &h[..]);
    }

    #[test]
    fn derive_nonce_unknown_mode_falls_back_to_xchacha() {
        let h = build_rtp_header(1, 2, 3);
        let nonce = derive_nonce("bogus_mode", &h);
        assert_eq!(nonce.len(), 24);
    }

    #[test]
    fn i16_to_f32_normalises_full_scale() {
        let pcm = [i16::MAX, 0, i16::MIN];
        let f = i16_to_f32(&pcm);
        assert!((f[0] - 0.999_969_5).abs() < 1e-4);
        assert!((f[1]).abs() < 1e-6);
        assert!((f[2] - -1.0).abs() < 1e-4);
    }

    #[test]
    fn rms_db_silence_is_neg_infinity() {
        let pcm = [0i16; 100];
        assert_eq!(rms_db_i16(&pcm), f32::NEG_INFINITY);
    }

    #[test]
    fn rms_db_empty_is_neg_infinity() {
        assert_eq!(rms_db_i16(&[]), f32::NEG_INFINITY);
    }

    #[test]
    fn rms_db_full_scale_is_near_zero_dbfs() {
        // A constant full-scale signal has RMS = 1.0 → 0 dBFS.
        let pcm = vec![i16::MAX; 480];
        let db = rms_db_i16(&pcm);
        // i16::MAX / 32768 = 0.99997 → RMS = same → 20*log10 ≈ -0.0003 dB
        assert!(db > -0.1 && db <= 0.0, "expected near 0 dBFS, got {db}");
    }

    #[test]
    fn rms_db_half_scale_is_near_minus_6_dbfs() {
        let half = i16::MAX / 2;
        let pcm = vec![half; 480];
        let db = rms_db_i16(&pcm);
        // half-amplitude → -6 dB
        assert!((db - -6.0).abs() < 0.1, "expected ~-6 dBFS, got {db}");
    }
}
