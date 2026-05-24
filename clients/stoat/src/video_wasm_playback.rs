//! Stoat WASM video playback (Phase B.4 of `plan-stoat-video-wasm.md`).
//!
//! Exposes one public function called by the decode path in `voice_wasm.rs`:
//!
//! - [`push_h264`] — schedule an H.264 NAL fragment for a remote user. The
//!   dispatcher buffers FU-A fragments per-user, reassembles complete NAL units
//!   via [`crate::video_common::reassemble_fua`], hands the assembled NAL to
//!   the per-user [`VideoDecoder`], and the decoded `VideoFrame` is drawn to a
//!   `<canvas>` element identified by
//!   [`crate::video_common::canvas_id_for(user_id)`].
//!
//! ## Design
//!
//! A `thread_local! { static PUMPS }` map (keyed by user_id) holds one
//! [`UserVideoPump`] per active remote participant — same pattern as
//! `voice_wasm_audio_playback.rs`.
//!
//! ## Status
//!
//! The Rust skeleton owns the per-user state map and FU-A reassembly buffer.
//! The actual `VideoDecoder` configuration + per-frame canvas draw is
//! intentionally kept minimal in this commit (parity with the discord skeleton
//! in `clients/discord/src/voice_bridge/video_playback.rs`). The full
//! decoder/draw pipeline lands in a follow-up pass when the WebCodecs JS
//! interop is added. The wire format and dispatcher are already ready (audio
//! path proven; video path symmetric).

// This entire file is wasm32-only; the module declaration in lib.rs is gated
// with #[cfg(target_arch = "wasm32")].

use std::cell::RefCell;
use std::collections::HashMap;

use super::video_common::{canvas_id_for, reassemble_fua};

// ── Thread-local state ────────────────────────────────────────────────────────

thread_local! {
    /// Per-user playback state. Keyed by the stoat user_id string (derived from
    /// the 8-byte ASCII null-padded prefix on each Vortex binary frame).
    static PUMPS: RefCell<HashMap<String, UserVideoPump>> = RefCell::new(HashMap::new());
}

// ── UserVideoPump ─────────────────────────────────────────────────────────────

/// Playback state for a single remote participant's video stream.
///
/// Buffers FU-A fragments until an `E`-bit fragment arrives, then hands the
/// reassembled NAL to the `VideoDecoder` and draws the resulting `VideoFrame`
/// to the canvas identified by [`canvas_id_for`].
struct UserVideoPump {
    /// Buffer of in-flight FU-A fragments for a single NAL unit. Cleared after
    /// each successful reassembly.
    pending_fragments: Vec<Vec<u8>>,
    /// Canvas DOM id the decoded frames should draw into. Stored once at pump
    /// creation so the per-frame path doesn't recompute the format string.
    canvas_id: String,
}

impl UserVideoPump {
    fn new(user_id: &str) -> Self {
        Self {
            pending_fragments: Vec::new(),
            canvas_id: canvas_id_for(user_id),
        }
    }

    /// Append a fragment. If the fragment carries the FU-A E bit (or is a
    /// standalone non-fragmented NAL), reassemble and return the complete NAL.
    fn append(&mut self, fragment: Vec<u8>) -> Option<Vec<u8>> {
        if fragment.is_empty() {
            return None;
        }
        // Detect whether this is an FU-A fragment (FU-indicator type = 28) or
        // a standalone NAL (any other type). Bits 0..4 of the indicator byte
        // are the NAL type per RFC 6184 §5.3 (and FU-A is type 28).
        let nal_type_in_indicator = fragment[0] & 0x1F;
        if nal_type_in_indicator != 28 {
            // Standalone NAL — emit immediately. Clear any in-flight FU buffer.
            self.pending_fragments.clear();
            return Some(fragment);
        }
        // FU-A fragment.
        self.pending_fragments.push(fragment);
        // If the last appended fragment has the E bit set, reassemble.
        let last = self.pending_fragments.last()?;
        if last.len() >= 2 && (last[1] & 0x40) != 0 {
            let nal = reassemble_fua(&self.pending_fragments);
            self.pending_fragments.clear();
            return nal;
        }
        None
    }

    /// Hand a complete NAL unit to the decoder + canvas draw pipeline.
    ///
    /// Skeleton — the full `VideoDecoder` configuration + canvas draw lands in
    /// a follow-up commit. This logs the assembled NAL for now so the dispatcher
    /// surface area is testable.
    fn decode_and_draw(&self, nal: &[u8]) {
        // The real path:
        //   1. If first frame ever for this pump: create VideoDecoder, configure
        //      with avc1.42E01F, attach output callback that draws VideoFrame to
        //      canvas via 2d ctx.drawImage().
        //   2. Wrap NAL in EncodedVideoChunk { type: idr ? "key" : "delta",
        //      timestamp: monotonic_micros, data: nal }.
        //   3. decoder.decode(chunk).
        //
        // Skeleton logs the NAL length and target canvas id so integration
        // checks can verify the dispatcher path runs end-to-end.
        tracing::debug!(
            target: "poly_stoat::video_wasm_playback",
            canvas_id = %self.canvas_id,
            nal_len = nal.len(),
            "video NAL ready for decode (skeleton — decoder wiring deferred)"
        );
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Push an H.264 NAL fragment for a remote user. Lazily creates a per-user
/// `UserVideoPump` on first call for that `user_id`.
///
/// The fragment may be:
/// - A standalone NAL unit (RFC 6184 NAL type 1..23) — decoded immediately.
/// - An FU-A fragment (type 28) — buffered until the matching `E`-bit
///   fragment arrives, then the complete NAL is reassembled and decoded.
///
/// On any error, the fragment is silently dropped with a `tracing::warn!`.
/// The video path must be resilient to individual frame loss — losing a frame
/// is always preferable to panicking.
pub fn push_h264(user_id: &str, nal_or_fragment: Vec<u8>) {
    PUMPS.with(|pumps| {
        let mut map = pumps.borrow_mut();
        let pump = map
            .entry(user_id.to_owned())
            .or_insert_with(|| UserVideoPump::new(user_id));
        if let Some(nal) = pump.append(nal_or_fragment) {
            pump.decode_and_draw(&nal);
        }
    });
}

/// Drop the per-user video playback state when a `VoiceParticipantLeft` event
/// arrives (mirrors `voice_wasm_audio_playback::drop_user`).
pub fn drop_user(user_id: &str) {
    PUMPS.with(|pumps| {
        if pumps.borrow_mut().remove(user_id).is_some() {
            tracing::debug!(
                target: "poly_stoat::video_wasm_playback",
                user_id,
                "video pump dropped for user"
            );
        }
        // Not an error if absent — user may have never published video.
    });
}
