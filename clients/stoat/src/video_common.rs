//! Shared video constants and codec-layer helpers for Stoat video transport.
//!
//! This module is **cfg-free** — it compiles on both native and
//! `wasm32-unknown-unknown`. It contains only pure-Rust definitions that
//! depend on `std` + `thiserror` and nothing platform-specific (no `web_sys`,
//! no `tokio` runtime, no LiveKit SDK).
//!
//! ## Scope

// Video primitives — most call sites land with the WebCodecs JS interop in
// Phase B.3/B.4 follow-up (encoder output callback + decoder input callback).
// Until then, native sees them as unused (no wasm32-gated callers) and wasm32
// sees the chain die at `send_h264_nal` stub awaiting the encoder.
// lint-allow-unused: video codec primitives — encoder/decoder callback wiring deferred to Phase B.3/B.4 follow-up
#![allow(dead_code)]

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
// Arithmetic is bounds-checked by the while guard; indexing is safe within those bounds.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
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
// Arithmetic: `idx + chunk_size` bounded by `.min(total)` and loop guard `idx < total`;
// `end - idx` non-wrapping because `end = (idx + chunk_size).min(total) >= idx`.
// Indexing: all slices within bounds verified by the nal.is_empty guard and loop guard.
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
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
// Indexing: `first[0]`, `first[1]`, `last[1]`, `f[2..]` — safe because the
// `any(|f| f.len() < 2)` guard below rejects EVERY short fragment before any
// indexing or subtraction happens.
// Arithmetic: the per-fragment `len() - 2` uses `saturating_sub`; `1 + sum` is
// bounded by available memory (overflow would OOM before wrapping).
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
#[must_use]
pub fn reassemble_fua(fragments: &[Vec<u8>]) -> Option<Vec<u8>> {
    if fragments.is_empty() {
        return None;
    }
    // Validate EVERY fragment up-front, not just the first/last. Fragments come
    // straight off the network (`voice_wasm.rs` → `push_h264`), so a 0- or
    // 1-byte MIDDLE fragment is remote-triggerable; it must be rejected before
    // the capacity computation subtracts 2 from its length.
    if fragments.iter().any(|f| f.len() < 2) {
        return None;
    }
    let first = fragments.first()?;
    if first[1] & 0x80 == 0 {
        return None; // first fragment must have S bit
    }
    let last = fragments.last()?;
    if last[1] & 0x40 == 0 {
        return None; // last fragment must have E bit
    }
    let fu_indicator = first[0];
    let nal_type = first[1] & 0x1F;
    let reconstructed_header = (fu_indicator & 0xE0) | nal_type;
    let body_len: usize = fragments.iter().map(|f| f.len().saturating_sub(2)).sum();
    let mut out = Vec::with_capacity(body_len.saturating_add(1));
    out.push(reconstructed_header);
    for f in fragments {
        out.extend_from_slice(&f[2..]);
    }
    Some(out)
}

// ── FU-A reassembly state machine ─────────────────────────────────────────────

/// Hard cap on the number of FU-A fragments buffered for a single in-flight NAL.
///
/// At the 1200-byte [`RTP_VIDEO_MTU`] this is ~300 KB — far above any legitimate
/// H.264 access unit at 640x360 — so hitting it means the terminating `E`-bit
/// fragment was lost, or a peer is deliberately never sending one.
pub const MAX_FU_FRAGMENTS: usize = 256;

/// Buffers the FU-A fragments of ONE in-flight NAL unit and emits the complete
/// NAL when the terminating `E`-bit fragment arrives.
///
/// Lives here (cfg-free) rather than in the wasm32-only playback module so the
/// state machine — which is fed straight from the network and is therefore the
/// crate's most attacker-exposed code — is unit-testable on the host.
#[derive(Debug, Default)]
pub struct FuaReassembler {
    pending: Vec<Vec<u8>>,
}

/// What [`FuaReassembler::append`] did with a fragment.
#[derive(Debug, PartialEq, Eq)]
pub enum FuaAppend {
    /// A complete NAL unit is ready to decode.
    Complete(Vec<u8>),
    /// The fragment was buffered; more are needed.
    Buffered,
    /// The fragment (or the sequence it belongs to) was discarded.
    Discarded(FuaDiscardReason),
}

/// Why a fragment sequence was thrown away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuaDiscardReason {
    /// The fragment carried no bytes at all.
    EmptyFragment,
    /// More than [`MAX_FU_FRAGMENTS`] fragments arrived without an `E` bit.
    BufferOverflow,
    /// An `E`-bit fragment arrived but the buffered sequence was malformed.
    MalformedSequence,
    /// An `S`-bit fragment arrived while a previous NAL was still in flight.
    AbandonedPreviousNal,
}

impl FuaReassembler {
    /// Number of fragments currently buffered. Exposed for assertions/metrics.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Drop any in-flight fragments.
    pub fn reset(&mut self) {
        self.pending.clear();
    }

    /// Feed one wire fragment.
    ///
    /// A non-FU-A NAL (indicator type != 28) is passed straight through and
    /// clears any in-flight buffer. An FU-A fragment is buffered until its
    /// `E`-bit sibling arrives.
    ///
    /// The buffer is bounded by [`MAX_FU_FRAGMENTS`]: without the cap a peer
    /// that only ever sends `S`/middle fragments (or ordinary packet loss of
    /// the `E` fragment) grows it forever until the tab OOMs.
    pub fn append(&mut self, fragment: Vec<u8>) -> FuaAppend {
        // `first()` doubles as the empty-fragment guard, so the indicator byte
        // is read without indexing (CLAUDE.md bans `#[allow(indexing_slicing)]`).
        let Some(&indicator) = fragment.first() else {
            return FuaAppend::Discarded(FuaDiscardReason::EmptyFragment);
        };
        // Bits 0..4 of the indicator byte are the NAL type per RFC 6184 §5.3;
        // FU-A is type 28.
        if indicator & 0x1F != 28 {
            self.pending.clear();
            return FuaAppend::Complete(fragment);
        }

        // An `S`-bit fragment starts a NEW NAL, so anything still buffered
        // belongs to a NAL whose `E` fragment never arrived. Drop it rather than
        // concatenating the stale prefix into the next NAL.
        let starts_new_nal = fragment.get(1).is_some_and(|fu_header| fu_header & 0x80 != 0);
        let abandoned = starts_new_nal && !self.pending.is_empty();
        if abandoned {
            self.pending.clear();
        }

        self.pending.push(fragment);
        if self.pending.len() > MAX_FU_FRAGMENTS {
            self.pending.clear();
            return FuaAppend::Discarded(FuaDiscardReason::BufferOverflow);
        }

        let ends_nal = self
            .pending
            .last()
            .and_then(|last| last.get(1))
            .is_some_and(|fu_header| fu_header & 0x40 != 0);
        if !ends_nal {
            return if abandoned {
                FuaAppend::Discarded(FuaDiscardReason::AbandonedPreviousNal)
            } else {
                FuaAppend::Buffered
            };
        }

        let nal = reassemble_fua(&self.pending);
        self.pending.clear();
        nal.map_or(
            FuaAppend::Discarded(FuaDiscardReason::MalformedSequence),
            FuaAppend::Complete,
        )
    }
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
        nal.extend(std::iter::repeat_n(0xDDu8, 3000));
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
        nal.extend(std::iter::repeat_n(0xABu8, 2500));
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
    fn reassemble_rejects_short_middle_fragment() {
        // A remote peer can interleave a 1-byte "fragment" between a valid
        // S-bit and E-bit fragment. Before the fix this underflowed
        // `f.len() - 2` inside the `Vec::with_capacity` sum (debug: subtract
        // overflow panic; release: ~usize::MAX capacity → alloc abort).
        let bad = vec![
            vec![0x7C, 0x85, 0xAA], // S bit
            vec![0x1C],             // 1-byte middle fragment
            vec![0x7C, 0x45, 0xBB], // E bit
        ];
        assert!(reassemble_fua(&bad).is_none());

        // Zero-length middle fragment is equally rejected.
        let bad_empty = vec![
            vec![0x7C, 0x85, 0xAA],
            Vec::new(),
            vec![0x7C, 0x45, 0xBB],
        ];
        assert!(reassemble_fua(&bad_empty).is_none());
    }

    #[test]
    fn reassemble_rejects_short_first_and_last_fragments() {
        assert!(reassemble_fua(&[vec![0x7C]]).is_none());
        assert!(reassemble_fua(&[vec![0x7C, 0x85, 0xAA], vec![0x7C]]).is_none());
    }

    #[test]
    fn reassembler_round_trips_a_fragmented_nal() {
        let mut nal = vec![0x65u8];
        nal.extend(std::iter::repeat_n(0xABu8, 2500));
        let frags = fragment_nal_units_to_fua(&nal, 800);
        let mut r = FuaReassembler::default();
        let last_idx = frags.len() - 1;
        for (i, f) in frags.into_iter().enumerate() {
            let out = r.append(f);
            if i == last_idx {
                assert_eq!(out, FuaAppend::Complete(nal.clone()));
            } else {
                assert_eq!(out, FuaAppend::Buffered);
            }
        }
        assert_eq!(r.pending_len(), 0, "buffer cleared after reassembly");
    }

    #[test]
    fn reassembler_passes_standalone_nal_through() {
        let mut r = FuaReassembler::default();
        // Type 5 (IDR) — not FU-A.
        let nal = vec![0x65u8, 0xAA];
        assert_eq!(r.append(nal.clone()), FuaAppend::Complete(nal));
    }

    #[test]
    fn reassembler_caps_pending_fragments() {
        let mut r = FuaReassembler::default();
        assert_eq!(r.append(vec![0x7C, 0x85, 0xAA]), FuaAppend::Buffered);
        let mut overflowed = false;
        for _ in 0..(MAX_FU_FRAGMENTS * 3) {
            // Middle fragment: neither S nor E bit.
            if r.append(vec![0x7C, 0x05, 0xBB])
                == FuaAppend::Discarded(FuaDiscardReason::BufferOverflow)
            {
                overflowed = true;
            }
            assert!(
                r.pending_len() <= MAX_FU_FRAGMENTS,
                "FU-A buffer must never exceed the cap"
            );
        }
        assert!(overflowed, "expected at least one overflow discard");
    }

    #[test]
    fn reassembler_drops_stale_nal_on_new_start_bit() {
        let mut r = FuaReassembler::default();
        assert_eq!(r.append(vec![0x7C, 0x85, 0xAA]), FuaAppend::Buffered);
        assert_eq!(r.append(vec![0x7C, 0x05, 0x11]), FuaAppend::Buffered);
        assert_eq!(r.pending_len(), 2);
        // New S bit abandons the previous NAL.
        assert_eq!(
            r.append(vec![0x7C, 0x85, 0xBB]),
            FuaAppend::Discarded(FuaDiscardReason::AbandonedPreviousNal)
        );
        assert_eq!(r.pending_len(), 1);
        // The terminating fragment yields ONLY the new NAL's body — the stale
        // 0xAA/0x11 prefix must not be concatenated in.
        assert_eq!(
            r.append(vec![0x7C, 0x45, 0xCC]),
            FuaAppend::Complete(vec![0x65, 0xBB, 0xCC])
        );
    }

    #[test]
    fn reassembler_rejects_short_middle_fragment_without_panicking() {
        let mut r = FuaReassembler::default();
        assert_eq!(r.append(vec![0x7C, 0x85, 0xAA]), FuaAppend::Buffered);
        // 1-byte FU-A fragment: indicator only, no FU header.
        assert_eq!(r.append(vec![0x1C]), FuaAppend::Buffered);
        // The E-bit fragment triggers reassembly, which must reject rather than
        // underflow `f.len() - 2` in the capacity computation.
        assert_eq!(
            r.append(vec![0x7C, 0x45, 0xBB]),
            FuaAppend::Discarded(FuaDiscardReason::MalformedSequence)
        );
        assert_eq!(r.pending_len(), 0);
    }

    #[test]
    fn reassembler_discards_empty_fragment() {
        let mut r = FuaReassembler::default();
        assert_eq!(
            r.append(Vec::new()),
            FuaAppend::Discarded(FuaDiscardReason::EmptyFragment)
        );
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
