//! Discord video transport — Phase E of `docs/plans/plan-voice-video-calls.md`.
//!
//! # Protocol
//!
//! Discord video uses the same voice WebSocket (op 12 Video signaling, op 14
//! Client Connect) and the same UDP socket as audio (per anti-ban requirements —
//! see `plan-discord-anti-ban.md`). H.264 RTP frames are packetized per RFC 6184
//! (FU-A fragmentation for large NALs) and encrypted with the same AEAD key.
//!
//! # Design
//!
//! - `DiscordVideoTransport` is created by `DiscordClient::start_video` / `start_screen_share`.
//! - It sends op 12 Video to announce the video SSRC, then starts the encode/send loop.
//! - Incoming video RTP is depacketized and decoded via the host-bridge.
//! - Decoded frames are pushed to per-user broadcast channels for the UI.
//!
//! # WASM safety
//!
//! This module is `#[cfg(feature = "voice")]` (inside the `voice` module).
//! WASM builds never enable `voice`, so this code never compiles for wasm32.

// lint-allow-unused: H.264 RTP/NAL byte-slicing with length guards in video transport
#![allow(clippy::indexing_slicing)]
// lint-allow-unused: Video transport: panics gated behind Option returns or explicit early-returns
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Codec/DSP math: as-casts and arithmetic in video transport are intentional
#![allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::arithmetic_side_effects,
    clippy::default_numeric_fallback,
    clippy::unnecessary_cast,
    clippy::future_not_send,
    clippy::significant_drop_tightening,
    clippy::redundant_closure_for_method_calls,
    clippy::wildcard_imports,
    clippy::map_err_ignore,
    clippy::map_unwrap_or,
    clippy::manual_is_multiple_of,
    clippy::manual_let_else,
    clippy::single_match_else,
    clippy::let_underscore_must_use,
    clippy::missing_const_for_fn,
    clippy::cognitive_complexity,
)]
// lint-allow-unused: H.264/RTP functions have inherent complexity in NAL fragmentation
#![allow(clippy::too_many_lines)]

use std::sync::{
    atomic::{AtomicU16, AtomicU32, Ordering},
    Arc,
};

use super::rtcp;

use base64::Engine as _;
use chacha20poly1305::{aead::KeyInit, XChaCha20Poly1305};
use tokio::{net::UdpSocket, sync::{broadcast, mpsc, RwLock}};
use tracing::{info, warn};

use poly_host_bridge::video_client::{
    EncodeH264Request, VideoBridgeClient,
};
use poly_video_backend::types::{VideoFrame, VideoPixelFormat};

// ── Constants ──────────────────────────────────────────────────────────────────

/// RTP payload type for H.264 video (Discord convention: PT 102).
const RTP_PT_H264: u8 = 102;

/// RTP header size.
const RTP_HEADER_SIZE: usize = 12;

/// Maximum RTP payload (UDP MTU 1500 − IP/UDP headers).
const VIDEO_MTU: usize = 1100;

/// Keyframe interval: send a forced IDR every 60 frames (~2s at 30fps).
const KEYFRAME_INTERVAL_FRAMES: u32 = 60;

/// Video SSRC offset from audio SSRC (Discord convention: video_ssrc = audio_ssrc + 1).
const VIDEO_SSRC_OFFSET: u32 = 1;

/// RTP clock rate for H.264 (RFC 6184).
const VIDEO_CLOCK_HZ: u32 = 90_000;

/// Nominal capture rate — matches the `max_framerate` we advertise in op 12.
const VIDEO_NOMINAL_FPS: u32 = 30;

/// RTP timestamp step per **access unit** (encoded frame), not per NAL unit.
///
/// RFC 3550 / RFC 6184 require every RTP packet belonging to one access unit to
/// carry the same timestamp; advancing per NAL made a SPS+PPS+IDR keyframe look
/// like three separate pictures and inflated the 90 kHz clock by 2-3× real time.
const VIDEO_TS_INCREMENT: u32 = VIDEO_CLOCK_HZ.div_euclid(VIDEO_NOMINAL_FPS);

// ── Error ──────────────────────────────────────────────────────────────────────

/// Errors from the Discord video transport.
#[derive(Debug, thiserror::Error)]
pub enum VideoTransportError {
    #[error("video bridge error: {0}")]
    Bridge(String),
    #[error("UDP send failed: {0}")]
    Udp(#[from] std::io::Error),
    #[error("AEAD error")]
    Aead,
    #[error("voice WS channel closed")]
    WsChannelClosed,
}

// ── DiscordVideoTransport ──────────────────────────────────────────────────────

/// A live Discord video transport layered on top of an existing voice connection.
///
/// Created by [`super::DiscordVoiceConnection::enable_video`].
/// The transport shares the same UDP socket and AEAD key as the audio transport.
///
/// Drop to stop the encode/send loop and release the encoder session.
pub struct DiscordVideoTransport {
    /// Video SSRC used in RTP headers.
    pub video_ssrc: u32,
    /// Host-bridge session ID for the H.264 encoder.
    pub session_id: String,
    /// Whether this is screen-share (vs camera).
    pub is_screen_share: bool,
    /// Per-user broadcast channels for decoded remote frames.
    /// Key: remote user_id from the SSRC→user map.
    pub remote_frame_channels: Arc<RwLock<std::collections::HashMap<String, broadcast::Sender<VideoFrame>>>>,
    /// Signal to stop the transport loops.
    stop_tx: mpsc::Sender<()>,
    /// E.9: current bitrate target in bps, read by the encode loop on every frame.
    ///
    /// This is the *same* `Arc<AtomicU32>` as the voice connection's
    /// `BandwidthController::target_bps` whenever a controller was passed to
    /// [`Self::start`], so every REMB/TWCC update parsed by the decode loop is
    /// visible to the encoder immediately.  Only when no controller is supplied
    /// does it fall back to a private atomic pinned at
    /// `rtcp::DEFAULT_BITRATE_BPS`.
    pub bw_target: Arc<AtomicU32>,
}

impl DiscordVideoTransport {
    /// Start the video transport on an existing voice connection.
    ///
    /// `frame_rx` yields BGRA `VideoFrame`s from the local camera / screen capture.
    /// The transport encodes each frame to H.264 via the host-bridge, packetizes
    /// NAL units as RTP (single-NAL or FU-A), encrypts with the AEAD key, and
    /// sends on the shared UDP socket.
    ///
    /// `bandwidth_ctrl` is the voice connection's `BandwidthController`: its
    /// `target_bps` `Arc<AtomicU32>` is shared (not copied) into the encode
    /// loop, so REMB/TWCC feedback parsed by the decode loop reaches the
    /// encoder live.  `aead_nonce` is the connection's shared `_rtpsize` nonce
    /// counter — audio and video use one session key and therefore must not
    /// draw nonces from independent counters.
    ///
    /// Returns a `DiscordVideoTransport` handle. Drop to stop.
    // lint-allow-unused: video transport requires all these params by protocol design
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        audio_ssrc: u32,
        is_screen_share: bool,
        udp: Arc<UdpSocket>,
        secret_key: [u8; 32],
        encryption_mode: String,
        ws_out_tx: mpsc::Sender<serde_json::Value>,
        bridge_base_url: String,
        mut frame_rx: mpsc::Receiver<VideoFrame>,
        // E.9: shared bandwidth controller.  The encode loop reads `target_bps`
        // on every frame and forwards it to the host-bridge encoder for dynamic
        // bitrate adaptation via REMB/TWCC RTCP feedback.
        bandwidth_ctrl: Option<Arc<rtcp::BandwidthController>>,
        aead_nonce: Arc<AtomicU32>,
    ) -> Result<Self, VideoTransportError> {
        let video_ssrc = audio_ssrc + VIDEO_SSRC_OFFSET;
        let session_id = format!("discord-video-{video_ssrc}");
        let remote_frame_channels: Arc<RwLock<std::collections::HashMap<String, broadcast::Sender<VideoFrame>>>> =
            Arc::new(RwLock::new(std::collections::HashMap::new()));

        // Send op 12 Video — announce video SSRC to Discord.
        let op12 = build_op12_video(audio_ssrc, video_ssrc, is_screen_share);
        ws_out_tx
            .send(op12)
            .await
            .map_err(|_| VideoTransportError::WsChannelClosed)?;

        // Send op 14 Client Connect — SSRC mapping.
        let op14 = build_op14_client_connect(audio_ssrc, video_ssrc);
        ws_out_tx
            .send(op14)
            .await
            .map_err(|_| VideoTransportError::WsChannelClosed)?;

        info!(
            target: "poly_discord::voice::video",
            video_ssrc,
            is_screen_share,
            "video transport started — op 12 + op 14 sent"
        );

        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);

        // ── Encode/send loop ───────────────────────────────────────────────────
        let bridge_client = VideoBridgeClient::new(bridge_base_url.clone());
        let session_id_clone = session_id.clone();
        let udp_enc = Arc::clone(&udp);
        let enc_mode = encryption_mode.clone();
        let enc_key = secret_key;
        // E.9: the shared bitrate target.
        // When a BandwidthController is provided, share its target AtomicU32 so that
        // RTCP feedback (REMB/TWCC) from the decode loop flows directly to the encode loop.
        // When no controller is provided (legacy `start` path), create a fresh AtomicU32
        // initialised at DEFAULT_BITRATE_BPS — the encode loop will use 1 Mbps until a
        // controller is linked via `DiscordVoiceConnection::link_video_bandwidth_ctrl`.
        let bw_target_arc: Arc<AtomicU32> = bandwidth_ctrl
            .as_ref()
            .map(|ctrl| ctrl.share_target())
            .unwrap_or_else(|| Arc::new(AtomicU32::new(rtcp::DEFAULT_BITRATE_BPS)));
        // Clone for the encode task (bw_target_arc is also stored on the struct below).
        let bw_target_for_loop = Arc::clone(&bw_target_arc);
        let aead_nonce_for_loop = Arc::clone(&aead_nonce);

        tokio::spawn(async move {
            let sequence = Arc::new(AtomicU16::new(0));
            let timestamp = Arc::new(AtomicU32::new(0));
            let frame_counter = Arc::new(AtomicU32::new(0));

            let cipher = match XChaCha20Poly1305::new_from_slice(&enc_key) {
                Ok(c) => c,
                Err(e) => {
                    warn!(target: "poly_discord::voice::video", "cipher init failed: {e}");
                    return;
                }
            };

            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    frame = frame_rx.recv() => {
                        let Some(frame) = frame else { break };

                        let fc = frame_counter.fetch_add(1, Ordering::Relaxed);
                        let force_keyframe = fc % KEYFRAME_INTERVAL_FRAMES == 0;

                        // Encode via host-bridge.
                        if frame.format != VideoPixelFormat::Bgra {
                            warn!(target: "poly_discord::voice::video", "expected BGRA frame, got {:?}", frame.format);
                            continue;
                        }
                        let data_b64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);
                        // E.9: read the current bandwidth target and forward it to the
                        // host-bridge encoder so it can adapt the H.264 bitrate dynamically.
                        // The AtomicU32 is updated by the RTCP decode path on every REMB/TWCC
                        // packet and by the WS ramp-up timer every 2 seconds.
                        let current_target_bps: Option<u32> =
                            Some(bw_target_for_loop.load(Ordering::Relaxed));
                        let encode_req = EncodeH264Request {
                            width: frame.width,
                            height: frame.height,
                            format: "bgra".into(),
                            data_b64,
                            force_keyframe,
                            session_id: session_id_clone.clone(),
                            target_bps: current_target_bps,
                        };

                        let resp = match bridge_client.encode(encode_req).await {
                            Ok(r) => r,
                            Err(e) => {
                                warn!(target: "poly_discord::voice::video", "encode_h264 failed: {e}");
                                continue;
                            }
                        };

                        // Decode NAL units from base64.  All NALs of one encoder
                        // response belong to the SAME access unit, so they share
                        // one RTP timestamp and only the very last packet of the
                        // very last NAL carries the marker bit.
                        let nals: Vec<Vec<u8>> = resp
                            .nal_units_b64
                            .iter()
                            .filter_map(|nal_b64| {
                                base64::engine::general_purpose::STANDARD.decode(nal_b64).ok()
                            })
                            .collect();
                        if nals.is_empty() {
                            continue;
                        }
                        let ts = timestamp.fetch_add(VIDEO_TS_INCREMENT, Ordering::Relaxed);
                        let last_nal_idx = nals.len().saturating_sub(1);

                        for (nal_idx, nal) in nals.iter().enumerate() {
                            // Packetize: single NAL if small, FU-A if large.
                            let packets = rtp_packetize_h264(nal, VIDEO_MTU);
                            let last_idx = packets.len().saturating_sub(1);

                            for (i, payload) in packets.into_iter().enumerate() {
                                let seq = sequence.fetch_add(1, Ordering::Relaxed);
                                // Marker = end of the access unit, not end of a NAL.
                                let is_last = nal_idx == last_nal_idx && i == last_idx;
                                let header = build_video_rtp_header(seq, ts, video_ssrc, is_last);

                                // Nonce counter is shared with the audio transport:
                                // both streams use the same voice session key.
                                let nonce_counter = aead_nonce_for_loop.fetch_add(1, Ordering::Relaxed);
                                let encrypted = match encrypt_video_rtp(
                                    &cipher, &header, &payload, &enc_mode, nonce_counter,
                                ) {
                                    Ok(e) => e,
                                    Err(_) => {
                                        warn!(target: "poly_discord::voice::video", "AEAD encrypt failed");
                                        continue;
                                    }
                                };

                                let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + encrypted.len());
                                packet.extend_from_slice(&header);
                                packet.extend_from_slice(&encrypted);

                                if let Err(e) = udp_enc.send(&packet).await {
                                    warn!(target: "poly_discord::voice::video", "UDP send: {e}");
                                }
                            }
                        }
                    }
                }
            }

            // Clean up encoder session on bridge.
            let cleanup_client = VideoBridgeClient::new(bridge_base_url);
            let _ = cleanup_client.close_session(&session_id_clone).await;
        });

        Ok(Self {
            video_ssrc,
            session_id,
            is_screen_share,
            remote_frame_channels,
            stop_tx,
            bw_target: bw_target_arc,
        })
    }

    /// Stop the video transport — closes the encode loop and sends op 12 with empty streams.
    pub async fn stop(self, ws_out_tx: &mpsc::Sender<serde_json::Value>) {
        let _ = self.stop_tx.send(()).await;

        // Send op 12 with empty streams to signal video off.
        let op12_stop = serde_json::json!({
            "op": 12,
            "d": {
                "audio_ssrc": self.video_ssrc - VIDEO_SSRC_OFFSET,
                "video_ssrc": 0,
                "rtx_ssrc": 0,
                "streams": []
            }
        });
        let _ = ws_out_tx.send(op12_stop).await;
    }
}

// ── op 12 / op 14 builders ─────────────────────────────────────────────────────

fn build_op12_video(
    audio_ssrc: u32,
    video_ssrc: u32,
    is_screen_share: bool,
) -> serde_json::Value {
    let rid = if is_screen_share { "200" } else { "100" };
    serde_json::json!({
        "op": 12,
        "d": {
            "audio_ssrc": audio_ssrc,
            "video_ssrc": video_ssrc,
            "rtx_ssrc": video_ssrc + 1,
            "streams": [{
                "type": "video",
                "rid": rid,
                "ssrc": video_ssrc,
                "max_bitrate": 2_500_000u32,
                "max_framerate": 30u32,
                "max_resolution": {
                    "type": "fixed",
                    "width": 1280u32,
                    "height": 720u32,
                }
            }]
        }
    })
}

fn build_op14_client_connect(audio_ssrc: u32, video_ssrc: u32) -> serde_json::Value {
    serde_json::json!({
        "op": 14,
        "d": {
            "streams": [
                { "ssrc": audio_ssrc, "type": "audio", "quality": 100u32 },
                { "ssrc": video_ssrc, "type": "video", "quality": 100u32 },
            ]
        }
    })
}

// ── RTP packetization (RFC 6184) ──────────────────────────────────────────────

/// Packetize a single H.264 NAL unit into one or more RTP payloads.
///
/// - NAL ≤ MTU → single NAL unit packet (no fragmentation header).
/// - NAL > MTU → FU-A fragmented packets.
fn rtp_packetize_h264(nal: &[u8], mtu: usize) -> Vec<Vec<u8>> {
    if nal.is_empty() {
        return vec![];
    }

    if nal.len() <= mtu {
        // Single NAL unit — pass through as-is.
        return vec![nal.to_vec()];
    }

    // FU-A fragmentation.
    // FU indicator byte: F=0, NRI from NAL header, type=28 (FU-A)
    let fu_indicator = (nal[0] & 0xe0) | 28u8;
    let nal_type = nal[0] & 0x1f;
    let nal_data = &nal[1..]; // skip NAL header byte
    let max_payload = mtu - 2; // 2 bytes for FU indicator + FU header

    let mut packets = Vec::new();
    let mut offset = 0;
    let total = nal_data.len();

    while offset < total {
        let end = (offset + max_payload).min(total);
        let is_start = offset == 0;
        let is_end = end == total;

        // FU header: S bit | E bit | R bit | nal_type
        let fu_header = ((is_start as u8) << 7) | ((is_end as u8) << 6) | nal_type;

        let mut pkt = Vec::with_capacity(2 + (end - offset));
        pkt.push(fu_indicator);
        pkt.push(fu_header);
        pkt.extend_from_slice(&nal_data[offset..end]);
        packets.push(pkt);

        offset = end;
    }

    packets
}

// ── RTP header builder (for video) ────────────────────────────────────────────

fn build_video_rtp_header(sequence: u16, timestamp: u32, ssrc: u32, marker: bool) -> [u8; RTP_HEADER_SIZE] {
    let mut header = [0u8; RTP_HEADER_SIZE];
    header[0] = 0x80; // V=2, P=0, X=0, CC=0
    // Marker bit: set on last packet of a frame.
    header[1] = RTP_PT_H264 | (if marker { 0x80 } else { 0x00 });
    header[2] = (sequence >> 8) as u8;
    header[3] = sequence as u8;
    header[4] = (timestamp >> 24) as u8;
    header[5] = (timestamp >> 16) as u8;
    header[6] = (timestamp >> 8) as u8;
    header[7] = timestamp as u8;
    header[8] = (ssrc >> 24) as u8;
    header[9] = (ssrc >> 16) as u8;
    header[10] = (ssrc >> 8) as u8;
    header[11] = ssrc as u8;
    header
}

// ── AEAD helpers ──────────────────────────────────────────────────────────────

/// Encrypt one video RTP payload.
///
/// Delegates to [`super::aead::encrypt_rtp`] so video and audio cannot drift on
/// the wire format — the previous private copy here derived the nonce from the
/// RTP header instead of using the `_rtpsize` counter.
fn encrypt_video_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    plaintext: &[u8],
    mode: &str,
    nonce_counter: u32,
) -> Result<Vec<u8>, VideoTransportError> {
    super::aead::encrypt_rtp(cipher, rtp_header, plaintext, mode, nonce_counter)
        .map_err(|_| VideoTransportError::Aead)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn op12_video_has_correct_structure() {
        let op12 = build_op12_video(1234, 1235, false);
        assert_eq!(op12["op"], 12);
        let d = &op12["d"];
        assert_eq!(d["audio_ssrc"], 1234u32);
        assert_eq!(d["video_ssrc"], 1235u32);
        let streams = d["streams"].as_array().unwrap();
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0]["type"], "video");
        assert_eq!(streams[0]["rid"], "100"); // camera
    }

    #[test]
    fn op12_screen_share_uses_rid_200() {
        let op12 = build_op12_video(100, 101, true);
        let streams = op12["d"]["streams"].as_array().unwrap();
        assert_eq!(streams[0]["rid"], "200");
    }

    #[test]
    fn op12_stop_has_empty_streams() {
        let video_ssrc = 1235u32;
        let op12_stop = serde_json::json!({
            "op": 12,
            "d": {
                "audio_ssrc": video_ssrc - VIDEO_SSRC_OFFSET,
                "video_ssrc": 0u32,
                "rtx_ssrc": 0u32,
                "streams": []
            }
        });
        let streams = op12_stop["d"]["streams"].as_array().unwrap();
        assert!(streams.is_empty());
    }

    #[test]
    fn rtp_packetize_small_nal_passthrough() {
        let nal = vec![0x65u8; 100]; // IDR NAL
        let packets = rtp_packetize_h264(&nal, 1100);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], nal);
    }

    #[test]
    fn rtp_packetize_large_nal_fu_a() {
        let nal = vec![0x65u8; 2000]; // large IDR
        let packets = rtp_packetize_h264(&nal, 1100);
        assert!(packets.len() >= 2);
        // Each fragment has FU indicator + FU header = 2 bytes prefix.
        assert_eq!(packets[0][1] & 0x80, 0x80, "start bit set on first fragment");
        assert_eq!(
            packets.last().unwrap()[1] & 0x40,
            0x40,
            "end bit set on last fragment"
        );
    }

    #[test]
    fn video_rtp_header_payload_type() {
        let hdr = build_video_rtp_header(1, 0, 999, false);
        assert_eq!(hdr[1], RTP_PT_H264, "PT = 102");
        assert_eq!(u32::from_be_bytes([hdr[8], hdr[9], hdr[10], hdr[11]]), 999, "SSRC");
    }

    #[test]
    fn video_rtp_header_marker_bit() {
        let hdr_marked = build_video_rtp_header(1, 0, 1, true);
        assert_eq!(hdr_marked[1] & 0x80, 0x80, "marker bit set");
        let hdr_unmarked = build_video_rtp_header(1, 0, 1, false);
        assert_eq!(hdr_unmarked[1] & 0x80, 0x00, "marker bit clear");
    }
}
