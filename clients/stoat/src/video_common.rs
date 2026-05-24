//! Shared video constants and codec-layer helpers for Stoat video transport.
//!
//! This module is **cfg-free** — it compiles on both native and
//! `wasm32-unknown-unknown`. It contains only pure-Rust definitions that
//! depend on `std` + `thiserror` and nothing platform-specific (no `web_sys`,
//! no `tokio` runtime, no LiveKit SDK).
//!
//! ## Scope
//!
//! The transport question for Stoat video (Vortex-extension vs LiveKit-SFU vs
//! deferred) is unresolved — see `docs/plans/plan-stoat-video-wasm.md` Phase A.
//! Regardless of which transport eventually ships, **H.264 RTP packetization
//! per RFC 6184 is reusable** (Vortex-extension would carry RTP-shaped frames
//! over WS; LiveKit's SFU also negotiates H.264 as one of its codecs). The
//! helpers here are ported verbatim from
//! `clients/discord/src/voice_bridge/video_capture.rs` +
//! `video_playback.rs` so the codec/packetization layer is ready when the
//! transport answer materializes.
//!
//! ## Why duplicate rather than share
//!
//! Same rationale as `voice_common.rs` (Phase B.3/B.4 decision in
//! `plan-stoat-voice-wasm.md`): one extra reuse confirms the API surface
//! before extraction into `clients/common/`. When matrix or teams adds video,
//! the three callers will justify a shared `clients/common/wasm_video.rs`.

// ── Constants ─────────────────────────────────────────────────────────────────

/// Max RTP payload size we'll let a single packet carry. 1200 B leaves
/// headroom under the typical 1500-byte path MTU for IP + UDP + RTP +
/// AEAD-tag overhead. Mirrors discord's `RTP_VIDEO_MTU`.
pub const RTP_VIDEO_MTU: usize = 1200;

/// H.264 RTP payload type. 101 is a reasonable dynamic-PT default. The
/// concrete value is only load-bearing for transports that actually wrap
/// RTP (LiveKit hides this inside the SDK).
pub const RTP_PAYLOAD_TYPE_H264: u8 = 101;

/// Default capture resolution — 640×360 matches discord's WebCodecs config
/// and is the lowest-risk default for first-ship video over WS / SFU.
pub const DEFAULT_VIDEO_WIDTH: u32 = 640;
/// Default capture resolution height.
pub const DEFAULT_VIDEO_HEIGHT: u32 = 360;
/// Default capture frame rate (fps).
pub const DEFAULT_VIDEO_FRAMERATE: u32 = 30;
/// Default keyframe interval (frames). One IDR per second @ 30 fps.
pub const DEFAULT_VIDEO_KEYFRAME_INTERVAL: u32 = 30;
/// Default target bitrate (bits/sec). 800 kbps matches discord's default.
pub const DEFAULT_VIDEO_BITRATE_BPS: u32 = 800_000;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors produced by the Stoat video pipeline.
///
/// Separate from `StoatVoiceError` so that audio and video failure modes can
/// surface independently in the UI (camera-denied is not a voice failure).
#[derive(Debug, thiserror::Error)]
pub enum StoatVideoError {
    #[error("camera permission denied or unavailable: {0}")]
    CameraUnavailable(String),

    #[error("video encoder error: {0}")]
    Encoder(String),

    #[error("video decoder error: {0}")]
    Decoder(String),

    #[error("video transport not yet implemented (Stoat upstream gap — see plan-stoat-video-wasm.md)")]
    TransportNotImplemented,

    #[error("video session is not active")]
    NotConnected,
}

// ── NAL parsing + FU-A fragmentation (RFC 6184) ───────────────────────────────

/// Walk a raw H.264 byte stream and return the start indices of every
/// NAL unit (the byte AFTER the 0x000001 / 0x00000001 start code).
/// Pure function — used by capture loops and unit tests.
///
/// Ported verbatim from
/// `clients/discord/src/voice_bridge/video_capture.rs::find_nal_unit_starts`.
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
///
/// Ported verbatim from
/// `clients/discord/src/voice_bridge/video_capture.rs::fragment_nal_units_to_fua`.
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

/// Reassemble a single complete NAL unit from a sequence of FU-A
/// fragments. Returns `None` if the fragments are malformed or do not
/// terminate with an E-bit fragment.
///
/// Each input slice must include the 2-byte FU header (FU-indicator +
/// FU-header) followed by the fragment payload.
///
/// Ported verbatim from
/// `clients/discord/src/voice_bridge/video_playback.rs::reassemble_fua`.
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

/// Canvas ID convention for the per-participant video tile.
///
/// Mirrors the discord convention in
/// `clients/discord/src/voice_bridge/video_playback.rs::canvas_id_for`
/// and the `VideoTilePlaceholder` ID format in
/// `crates/core/src/ui/account/common/voice_view.rs`.
#[must_use]
pub fn canvas_id_for(participant_id: &str) -> String {
    format!("poly-video-tile-{participant_id}")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
// lint-allow-unused: test module uses unwrap/expect/panic per project policy
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn find_nal_starts_handles_three_and_four_byte_codes() {
        let buf: Vec<u8> = vec![
            0, 0, 0, 1, 0x67, 0x42, // SPS NAL (4-byte start)
            0, 0, 1, 0x68, 0xCE, // PPS NAL (3-byte start)
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
        assert!(frags.len() >= 3, "expected >=3 fragments for 3001-byte NAL");
        // FU-indicator: F|NRI from 0x65 (= 0x60), Type=28 -> 0x7C.
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
        // Round-trip: sum of payloads (minus 2-byte FU headers) should equal
        // original NAL body length (original length minus 1-byte NAL header).
        let total: usize = frags.iter().map(|f| f.len() - 2).sum();
        assert_eq!(total, nal.len() - 1, "FU-A payloads reassemble to NAL body");
    }

    #[test]
    fn reassemble_round_trips_fragmented_nal() {
        let mut nal = vec![0x65u8]; // IDR slice header
        nal.extend(std::iter::repeat(0xABu8).take(2500));
        let frags = fragment_nal_units_to_fua(&nal, 800);
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

    #[test]
    fn default_video_constants_are_reasonable() {
        assert_eq!(DEFAULT_VIDEO_WIDTH, 640);
        assert_eq!(DEFAULT_VIDEO_HEIGHT, 360);
        assert_eq!(DEFAULT_VIDEO_FRAMERATE, 30);
        assert_eq!(DEFAULT_VIDEO_KEYFRAME_INTERVAL, 30);
        assert_eq!(DEFAULT_VIDEO_BITRATE_BPS, 800_000);
        assert_eq!(RTP_VIDEO_MTU, 1200);
        assert_eq!(RTP_PAYLOAD_TYPE_H264, 101);
    }
}
