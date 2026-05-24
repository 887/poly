//! Extracted from voice_bridge.rs as part of SOLID B.2 split.
//!
//! H.264 video playback / RFC 6184 FU-A reassembly.
//! Pure structural move — no behaviour change.

use super::*;

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

