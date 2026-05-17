//! RTCP bandwidth feedback — Phase E.9 of `docs/plans/plan-voice-video-calls.md`.
//!
//! # Background
//!
//! Discord's SFU sends RTCP feedback packets on the same UDP socket as RTP.
//! Two feedback mechanisms are relevant for video bitrate adaptation:
//!
//! - **REMB** (Receiver Estimated Maximum Bitrate) — RFC 4585 + Google extension.
//!   The SFU sends a PSFB (PT=206, FMT=15) packet containing an estimated
//!   bitrate cap the sender should respect. This is a direct, explicit signal.
//!
//! - **TWCC** (Transport-Wide Congestion Control) — draft-holmer-rmcat-transport-wide-cc-extensions.
//!   The SFU sends a RTPFB (PT=205, FMT=15) packet reporting arrival timestamps
//!   for a range of RTP sequence numbers. We derive a bitrate estimate from the
//!   inter-packet delay variation (jitter) without implementing the full GCC
//!   algorithm — a simple packet-loss / delay ratio is sufficient for v1.
//!
//! # Design decisions (no webrtc-rs)
//!
//! webrtc-rs was explicitly deferred in the plan (same decision gate as E.5).
//! This module implements minimal hand-rolled parsers that extract the
//! bitrate signal without pulling in webrtc-rs.  The congestion controller is a
//! proportional-integral controller with hysteresis: the bitrate ramps up slowly
//! (5% per second) but reacts instantly to REMB and aggressively to TWCC loss.
//!
//! # Hard caps
//!
//! | Mode        | Default max bps | Note                                     |
//! |-------------|----------------|------------------------------------------|
//! | Camera      | 2 500 000      | Discord normal video cap                  |
//! | Screen share| 2 500 000      | Same cap; Discord SFU enforces its own    |
//! | REMB floor  |   150 000      | Never drop below 150 kbps                 |

#![allow(clippy::indexing_slicing)] // Packet byte-slicing with length guards

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tracing::{debug, warn};

// ── Constants ──────────────────────────────────────────────────────────────────

/// Maximum video bitrate in bps (Discord cap for normal video + screen share).
pub const MAX_BITRATE_BPS: u32 = 2_500_000;

/// Minimum bitrate floor in bps — never drop below this even under heavy congestion.
pub const MIN_BITRATE_BPS: u32 = 150_000;

/// Default starting bitrate before any REMB/TWCC feedback arrives.
pub const DEFAULT_BITRATE_BPS: u32 = 1_000_000;

/// Slow ramp-up rate: fraction of current bitrate added per feedback interval
/// (applied when no congestion is detected).  5% per update.
const RAMP_UP_FACTOR: f64 = 0.05;

/// Hysteresis band around the current target: REMB values within this fraction
/// of the current target are ignored (avoids micro-adjustments).
const HYSTERESIS_BAND: f64 = 0.05; // 5%

/// RTCP compound packet: every RTCP packet starts with a fixed 4-byte header.
const RTCP_HEADER_SIZE: usize = 4;

/// RTCP payload type for RTPFB (transport-layer feedback) — RFC 4585.
const PT_RTPFB: u8 = 205;
/// RTCP payload type for PSFB (payload-specific feedback) — RFC 4585.
const PT_PSFB: u8 = 206;

/// FMT (feedback message type) for REMB inside PSFB — Google extension.
const FMT_REMB: u8 = 15;
/// FMT for TWCC inside RTPFB — draft-holmer-rmcat-transport-wide-cc-extensions.
const FMT_TWCC: u8 = 15;

// ── RTCP packet classification ─────────────────────────────────────────────────

/// Feedback signal extracted from an RTCP packet.
#[derive(Debug, Clone, PartialEq)]
pub enum RtcpFeedback {
    /// REMB — explicit bitrate cap from the SFU.
    Remb { bps: u32 },
    /// TWCC — inferred bitrate suggestion from packet-arrival timing.
    /// The value here is the adjusted target after applying our congestion model.
    Twcc { suggested_bps: u32 },
}

// ── RTCP packet detection ──────────────────────────────────────────────────────

/// Heuristic to decide whether a UDP datagram is RTCP rather than RTP.
///
/// RTP packets for Discord audio use PT=120 (0x78) and video PT=102 (0x66).
/// RTCP payloads types are in the range 192–223 (RFC 5761).
/// We also check the version field (must be 2) and require the packet length
/// to satisfy the RTCP `length` field.
pub fn is_rtcp_packet(buf: &[u8]) -> bool {
    if buf.len() < RTCP_HEADER_SIZE {
        return false;
    }
    // Bit 7-6 of byte 0: version must be 2.
    let version = (buf[0] >> 6) & 0x3;
    if version != 2 {
        return false;
    }
    // Byte 1: payload type (no marker bit in RTCP — that bit is padding flag instead).
    let pt = buf[1] & 0x7F;
    // RTCP payload types: 192–223 (RFC 5761 / IANA).
    if !(192..=223).contains(&pt) {
        return false;
    }
    // `length` field (bytes 2-3): number of 32-bit words in the packet MINUS 1.
    // The total length in bytes is (length + 1) * 4.
    let declared_len = (u16::from_be_bytes([buf[2], buf[3]]) as usize + 1) * 4;
    // The buffer must be at least as large as declared.  Allow compound packets
    // (buf.len() can be larger).
    buf.len() >= declared_len
}

/// Try to parse RTCP feedback from a compound RTCP datagram.
///
/// Discord sends RTCP compound packets (multiple RTCP messages concatenated).
/// We walk each sub-packet and return the first meaningful feedback signal.
///
/// Returns `None` if the packet contains no REMB or TWCC feedback.
pub fn parse_rtcp_feedback(buf: &[u8], current_bps: u32) -> Option<RtcpFeedback> {
    let mut offset = 0;
    while offset + RTCP_HEADER_SIZE <= buf.len() {
        let pt = buf[offset + 1] & 0x7F;
        let fmt = (buf[offset] & 0x1F) as u8;
        let len_words = u16::from_be_bytes([buf[offset + 2], buf[offset + 3]]) as usize;
        let pkt_bytes = (len_words + 1) * 4;

        if offset + pkt_bytes > buf.len() {
            break; // malformed sub-packet
        }

        let sub = &buf[offset..offset + pkt_bytes];

        match (pt, fmt) {
            (PT_PSFB, FMT_REMB) => {
                if let Some(bps) = parse_remb(sub) {
                    return Some(RtcpFeedback::Remb { bps });
                }
            }
            (PT_RTPFB, FMT_TWCC) => {
                if let Some(suggested) = parse_twcc_simple(sub, current_bps) {
                    return Some(RtcpFeedback::Twcc { suggested_bps: suggested });
                }
            }
            _ => {}
        }

        offset += pkt_bytes;
    }
    None
}

// ── REMB parser ────────────────────────────────────────────────────────────────

/// Parse a REMB (RFC 4585 + Google extension) packet and return the bitrate in bps.
///
/// REMB packet layout (after the standard 12-byte RTCP header):
/// - bytes 0..4:  `"REMB"` ASCII magic (0x52454D42)
/// - byte  4:     number of SSRCs (N)
/// - bytes 5..7:  mantissa exponent packed bitrate (big-endian):
///                  exponent = bits 23..18 (6 bits)
///                  mantissa = bits 17..0  (18 bits)
///                  bitrate  = mantissa << exponent
/// - bytes 8..8+4N: SSRC list (N × 4 bytes each)
///
/// The standard 12-byte RTCP header is:
///   [V=2|P|FMT=15][PT=206][length_words][SSRC_sender][SSRC_media=0]
fn parse_remb(sub: &[u8]) -> Option<u32> {
    // Need at least 12-byte header + 4-byte magic + 4-byte bitrate field.
    if sub.len() < 20 {
        return None;
    }
    // Check "REMB" magic at offset 12.
    if &sub[12..16] != b"REMB" {
        return None;
    }
    // Bitrate at offsets 16-18 (3 bytes, big-endian):
    // byte 16 = exponent (bits 23..18 in 3-byte word) + high mantissa bits
    // byte 17..18 = remaining mantissa bits
    //
    // Layout: [EXP:6][MAN:18] packed big-endian across 3 bytes (indices 17..19).
    // Byte 16 = num_ssrcs, bytes 17-19 = bitrate field.
    if sub.len() < 20 {
        return None;
    }
    // num_ssrcs = sub[16], bitrate bytes = sub[17..20].
    let b0 = sub[17] as u32;
    let b1 = sub[18] as u32;
    let b2 = sub[19] as u32;
    let exp = (b0 >> 2) & 0x3F; // top 6 bits of b0
    let mantissa = ((b0 & 0x03) << 16) | (b1 << 8) | b2;
    let bps = mantissa << exp;
    Some(bps)
}

// ── TWCC parser (simplified) ───────────────────────────────────────────────────

/// Minimal TWCC feedback analysis.
///
/// A full GCC (Google Congestion Control) implementation is out of scope.
/// Instead we use a simple heuristic: count the number of "not received"
/// status symbols and compute a loss fraction. If loss > 5%, reduce the
/// bitrate estimate proportionally; otherwise allow the ramp-up path to
/// handle recovery.
///
/// TWCC packet (draft-holmer-rmcat-transport-wide-cc-extensions-01) layout
/// after the standard 8-byte RTPFB header:
/// - 2 bytes: base sequence number
/// - 2 bytes: packet status count (N)
/// - 3 bytes: reference time (24-bit, not used here)
/// - 1 byte:  feedback packet count
/// - variable: status chunks (2-byte each, various encodings)
///
/// We only read the status count and count "not received" bits to estimate loss.
fn parse_twcc_simple(sub: &[u8], current_bps: u32) -> Option<u32> {
    // Standard RTPFB header = 12 bytes: [header4][SSRC_sender4][SSRC_media4].
    // TWCC body starts at byte 12.
    if sub.len() < 16 {
        return None; // too short to have any status chunks
    }

    let packet_count = u16::from_be_bytes([sub[14], sub[15]]) as usize;
    if packet_count == 0 {
        return None;
    }

    // Status chunks start at byte 20 (12 RTPFB header + 2 base_seq + 2 count + 3 ref_time + 1 fb_count).
    let chunks_start = 20;
    if sub.len() < chunks_start {
        return None;
    }

    // Walk status chunks and count "not received" symbols.
    // Two chunk types:
    //   - Run-length chunk (bit 15 = 0): [0][symbol:1][count:14]
    //     symbol 0 = not received, symbol 1 = received small delta
    //   - Status vector chunk (bit 15 = 1): [1][symbol_size:1][14 symbols]
    //     symbol_size 0 → 1-bit symbols (0=not-rcvd, 1=rcvd), 14 symbols per chunk
    //     symbol_size 1 → 2-bit symbols (0=not-rcvd, 1=rcvd-small, 2=rcvd-large, 3=?), 7 symbols per chunk

    let mut not_received: usize = 0;
    let mut covered: usize = 0;
    let mut pos = chunks_start;

    while covered < packet_count && pos + 2 <= sub.len() {
        let chunk = u16::from_be_bytes([sub[pos], sub[pos + 1]]);
        pos += 2;

        if chunk & 0x8000 == 0 {
            // Run-length chunk.
            let symbol = (chunk >> 14) & 0x1;
            let count = (chunk & 0x1FFF) as usize;
            let count = count.min(packet_count - covered);
            if symbol == 0 {
                not_received += count;
            }
            covered += count;
        } else {
            // Status vector chunk.
            let symbol_size = (chunk >> 14) & 0x1;
            if symbol_size == 0 {
                // 1-bit symbols, 14 per chunk.
                for i in (0..14).rev() {
                    if covered >= packet_count {
                        break;
                    }
                    if (chunk >> i) & 0x1 == 0 {
                        not_received += 1;
                    }
                    covered += 1;
                }
            } else {
                // 2-bit symbols, 7 per chunk.
                for i in (0..7).rev() {
                    if covered >= packet_count {
                        break;
                    }
                    let sym = (chunk >> (i * 2)) & 0x3;
                    if sym == 0 {
                        not_received += 1;
                    }
                    covered += 1;
                }
            }
        }
    }

    if covered == 0 {
        return None;
    }

    let loss_fraction = not_received as f64 / covered as f64;

    if loss_fraction > 0.05 {
        // Reduce bitrate proportional to loss: lose 1% → 1% bitrate cut.
        // Cap reduction at 50% per feedback packet to avoid oscillation.
        let reduction = (1.0 - loss_fraction.min(0.5)) as f64;
        let suggested = ((current_bps as f64) * reduction) as u32;
        let suggested = suggested.max(MIN_BITRATE_BPS);
        debug!(
            target: "poly_discord::voice::rtcp",
            loss_pct = (loss_fraction * 100.0) as u32,
            current_bps,
            suggested,
            "TWCC loss detected — throttling bitrate"
        );
        Some(suggested)
    } else {
        // No meaningful congestion — return None to let ramp-up take over.
        None
    }
}

// ── Bandwidth controller ────────────────────────────────────────────────────────

/// Congestion controller for video bitrate.
///
/// Maintains the current target bitrate and applies feedback from REMB/TWCC.
///
/// Thread-safe: the current bitrate is stored in an `AtomicU32` so the encode
/// loop can read it without holding a lock.
///
/// # Hysteresis
///
/// REMB values within `HYSTERESIS_BAND` (5%) of the current target are silently
/// ignored to avoid encoder thrash from SFU jitter.  After a congestion event
/// the bitrate is clamped immediately; recovery ramps at `RAMP_UP_FACTOR`
/// (5%) per call to [`BandwidthController::ramp_up`].
pub struct BandwidthController {
    /// Current target bitrate in bps, accessible from the encode task.
    pub target_bps: Arc<AtomicU32>,
    /// Configured maximum (hard cap from Discord stream spec).
    max_bps: u32,
}

impl BandwidthController {
    /// Create a new controller starting at `DEFAULT_BITRATE_BPS`.
    pub fn new(max_bps: u32) -> Self {
        let target = DEFAULT_BITRATE_BPS.min(max_bps).max(MIN_BITRATE_BPS);
        Self {
            target_bps: Arc::new(AtomicU32::new(target)),
            max_bps,
        }
    }

    /// Create a new controller that shares the target `AtomicU32` with an existing handle.
    ///
    /// Used by the encode task to read the current target without a separate
    /// channel — the task calls `target_bps.load(Relaxed)` on each frame.
    pub fn share_target(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.target_bps)
    }

    /// Apply an RTCP feedback signal and update the target bitrate.
    ///
    /// Returns the new target bitrate in bps (for logging only).
    pub fn apply_feedback(&self, feedback: RtcpFeedback) -> u32 {
        let current = self.target_bps.load(Ordering::Relaxed);
        let new_target = match feedback {
            RtcpFeedback::Remb { bps } => {
                // Hard REMB: clamp to SFU's estimate.
                let remb_cap = bps.clamp(MIN_BITRATE_BPS, self.max_bps);
                // Hysteresis: ignore if within ±5% of current.
                let delta_frac = (remb_cap as f64 - current as f64).abs() / current as f64;
                if delta_frac < HYSTERESIS_BAND {
                    return current; // no change
                }
                remb_cap
            }
            RtcpFeedback::Twcc { suggested_bps } => {
                // TWCC: floor at min, ceiling at max.
                suggested_bps.clamp(MIN_BITRATE_BPS, self.max_bps)
            }
        };

        self.target_bps.store(new_target, Ordering::Relaxed);
        debug!(
            target: "poly_discord::voice::rtcp",
            previous_bps = current,
            new_bps = new_target,
            "bandwidth target updated"
        );
        new_target
    }

    /// Slow ramp-up: increase bitrate by `RAMP_UP_FACTOR` toward `max_bps`.
    ///
    /// Call this periodically (e.g. once per second) when no congestion feedback
    /// has arrived, so the encoder can recover from earlier throttling.
    pub fn ramp_up(&self) -> u32 {
        let current = self.target_bps.load(Ordering::Relaxed);
        if current >= self.max_bps {
            return current;
        }
        let bumped = ((current as f64) * (1.0 + RAMP_UP_FACTOR)) as u32;
        let new_target = bumped.min(self.max_bps);
        self.target_bps.store(new_target, Ordering::Relaxed);
        new_target
    }
}

// ── RTCP dispatch helper used by udp_decode_loop ──────────────────────────────

/// Process one RTCP datagram against a `BandwidthController`.
///
/// Called from `udp_decode_loop` in `mod.rs` whenever `is_rtcp_packet` returns
/// `true`.  Logs at `debug` on meaningful rate changes and `warn` on parse
/// failures.
pub fn handle_rtcp_datagram(buf: &[u8], ctrl: &BandwidthController) {
    let current = ctrl.target_bps.load(Ordering::Relaxed);
    match parse_rtcp_feedback(buf, current) {
        Some(fb) => {
            let new_bps = ctrl.apply_feedback(fb);
            if new_bps != current {
                warn!(
                    target: "poly_discord::voice::rtcp",
                    old_bps = current,
                    new_bps,
                    "RTCP feedback applied — video bitrate adjusted"
                );
            }
        }
        None => {
            // SR/RR or other RTCP — no action.
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    /// Build a minimal REMB packet.
    ///
    /// Standard RTCP header (12 bytes) + REMB body.
    fn make_remb_packet(bps: u32) -> Vec<u8> {
        // Encode bps as 6-bit exponent + 18-bit mantissa.
        let mut exp = 0u32;
        let mut mantissa = bps;
        while mantissa >= (1 << 18) {
            mantissa >>= 1;
            exp += 1;
        }
        // sub[17], sub[18], sub[19]
        let b0 = ((exp & 0x3F) << 2) | ((mantissa >> 16) & 0x03);
        let b1 = (mantissa >> 8) & 0xFF;
        let b2 = mantissa & 0xFF;

        let mut pkt = vec![
            // byte 0: V=2, P=0, FMT=15 → 0b10_0_01111 = 0x8F
            0x8F,
            // byte 1: PT=206 (PSFB)
            PT_PSFB,
            // bytes 2-3: length = (pkt_len/4) - 1
            // pkt_len = 20 → length = 4
            0x00, 0x04,
            // bytes 4-7: SSRC_sender (dummy)
            0x00, 0x00, 0x00, 0x01,
            // bytes 8-11: SSRC_media = 0
            0x00, 0x00, 0x00, 0x00,
            // bytes 12-15: "REMB"
            b'R', b'E', b'M', b'B',
            // byte 16: num_ssrcs = 1
            0x01,
            // bytes 17-19: bitrate
            b0 as u8, b1 as u8, b2 as u8,
            // bytes 20-23: SSRC 0
            0x00, 0x00, 0x00, 0x00,
        ];
        // Pad to 24 bytes (length field says 4 words × 4 = 20 bytes, but we
        // need 24 to include the trailing SSRC; fix the length field).
        // length = (24/4) - 1 = 5
        pkt[2] = 0x00;
        pkt[3] = 0x05;
        pkt
    }

    #[test]
    fn remb_packet_roundtrip_1mbps() {
        let pkt = make_remb_packet(1_000_000);
        assert!(is_rtcp_packet(&pkt));
        let fb = parse_rtcp_feedback(&pkt, DEFAULT_BITRATE_BPS).unwrap();
        match fb {
            RtcpFeedback::Remb { bps } => {
                // Mantissa encoding loses up to ~2 LSBs of precision.
                let diff = (bps as i64 - 1_000_000).abs();
                assert!(diff < 16_000, "expected ~1 Mbps, got {bps}");
            }
            other => panic!("expected REMB, got {other:?}"),
        }
    }

    #[test]
    fn remb_packet_roundtrip_2mbps() {
        let pkt = make_remb_packet(2_000_000);
        let fb = parse_rtcp_feedback(&pkt, DEFAULT_BITRATE_BPS).unwrap();
        match fb {
            RtcpFeedback::Remb { bps } => {
                let diff = (bps as i64 - 2_000_000).abs();
                assert!(diff < 32_000, "expected ~2 Mbps, got {bps}");
            }
            other => panic!("expected REMB, got {other:?}"),
        }
    }

    #[test]
    fn is_rtcp_rejects_rtp_packet() {
        // Simulate a Discord Opus RTP packet: V=2, PT=120 (0x78).
        let rtp = [0x80u8, 0x78, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
                   0x00, 0x00, 0x00, 0x01, 0x00, 0x00];
        assert!(!is_rtcp_packet(&rtp));
    }

    #[test]
    fn is_rtcp_accepts_remb_packet() {
        let pkt = make_remb_packet(1_000_000);
        assert!(is_rtcp_packet(&pkt));
    }

    #[test]
    fn bandwidth_controller_applies_remb() {
        let ctrl = BandwidthController::new(MAX_BITRATE_BPS);
        // Current = 1 Mbps.  REMB says 500 kbps — well outside hysteresis band.
        let new_bps = ctrl.apply_feedback(RtcpFeedback::Remb { bps: 500_000 });
        assert_eq!(new_bps, 500_000);
        assert_eq!(ctrl.target_bps.load(Ordering::Relaxed), 500_000);
    }

    #[test]
    fn bandwidth_controller_hysteresis_suppresses_tiny_change() {
        let ctrl = BandwidthController::new(MAX_BITRATE_BPS);
        let original = ctrl.target_bps.load(Ordering::Relaxed);
        // A REMB 3% above current — within the 5% hysteresis band.
        let close_bps = (original as f64 * 1.03) as u32;
        ctrl.apply_feedback(RtcpFeedback::Remb { bps: close_bps });
        // Should not have changed.
        assert_eq!(ctrl.target_bps.load(Ordering::Relaxed), original);
    }

    #[test]
    fn bandwidth_controller_clamps_below_floor() {
        let ctrl = BandwidthController::new(MAX_BITRATE_BPS);
        let new_bps = ctrl.apply_feedback(RtcpFeedback::Remb { bps: 50_000 });
        assert_eq!(new_bps, MIN_BITRATE_BPS, "should be clamped to floor");
    }

    #[test]
    fn bandwidth_controller_clamps_above_cap() {
        let ctrl = BandwidthController::new(1_000_000);
        let new_bps = ctrl.apply_feedback(RtcpFeedback::Remb { bps: 5_000_000 });
        assert_eq!(new_bps, 1_000_000, "should be clamped to max_bps");
    }

    #[test]
    fn bandwidth_controller_ramp_up_increases_bitrate() {
        let ctrl = BandwidthController::new(MAX_BITRATE_BPS);
        // Reduce to 500k first.
        ctrl.apply_feedback(RtcpFeedback::Remb { bps: 500_000 });
        let after_remb = ctrl.target_bps.load(Ordering::Relaxed);
        let after_ramp = ctrl.ramp_up();
        assert!(after_ramp > after_remb, "ramp_up should increase bitrate");
        // Should be approximately 5% more.
        let expected = (after_remb as f64 * 1.05) as u32;
        assert!((after_ramp as i64 - expected as i64).abs() < 100);
    }

    #[test]
    fn bandwidth_controller_ramp_stops_at_cap() {
        let ctrl = BandwidthController::new(1_000_000);
        // Already at cap.
        ctrl.target_bps.store(1_000_000, Ordering::Relaxed);
        let after = ctrl.ramp_up();
        assert_eq!(after, 1_000_000, "ramp_up should not exceed max_bps");
    }

    #[test]
    fn handle_rtcp_datagram_smoke() {
        let pkt = make_remb_packet(300_000);
        let ctrl = BandwidthController::new(MAX_BITRATE_BPS);
        handle_rtcp_datagram(&pkt, &ctrl);
        let target = ctrl.target_bps.load(Ordering::Relaxed);
        assert!(target <= 400_000, "REMB 300k should throttle below 400k, got {target}");
    }
}
