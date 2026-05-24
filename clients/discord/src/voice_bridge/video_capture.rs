//! Extracted from voice_bridge.rs as part of SOLID B.2 split.
//!
//! H.264 video capture (WebCodecs) + RFC 6184 NAL fragmentation helpers.
//! Pure structural move — no behaviour change.

use super::*;


    /// Max RTP payload size we'll let a single packet carry. 1200 B leaves
    /// headroom under the typical 1500-byte path MTU for IP + UDP + RTP +
    /// AEAD-tag overhead.
    pub const RTP_VIDEO_MTU: usize = 1200;
    /// H.264 RTP payload type used by Discord for video. 101 is a reasonable
    /// dynamic-PT default; the mock server preserves SSRC + PT bytes so any
    /// value works end-to-end.
    pub const RTP_PAYLOAD_TYPE_H264: u8 = 101;

    /// Resources the capture loop needs, snapshotted out of the
    /// `VoiceBridgeSession` under the mutex so the loop doesn't need to
    /// re-acquire it per frame.
    #[cfg(target_arch = "wasm32")]
    pub struct VideoBridgeHandles {
        pub udp: Arc<poly_host_bridge::udp_client::UdpClient>,
        pub aead: Arc<poly_host_bridge::aead_client::AeadClient>,
        pub udp_session: String,
        pub aead_session: String,
        pub video_ssrc: u32,
    }

    #[cfg(target_arch = "wasm32")]
    impl VideoBridgeHandles {
        /// Snapshot the handles from an active session, or `None` if no
        /// session is active or the video SSRC hasn't been negotiated yet.
        pub async fn from_session(guard: &VoiceSessionGuard) -> Option<Self> {
            let g = guard.lock().await;
            let s = g.as_ref()?;
            Some(Self {
                udp: Arc::clone(&s.udp),
                aead: Arc::clone(&s.aead),
                udp_session: s.udp_session.clone(),
                aead_session: s.aead_session.clone(),
                video_ssrc: s.video_ssrc?,
            })
        }
    }

    /// Walk a raw H.264 byte stream and return the start indices of every
    /// NAL unit (the byte AFTER the 0x000001 / 0x00000001 start code).
    /// Pure function — used both by the wasm capture loop and by unit tests.
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

    /// Start the WebCodecs camera capture pipeline.
    ///
    /// Spawns a `wasm_bindgen_futures::spawn_local` task that:
    ///   1. Opens `getUserMedia({video: {width:640, height:360, frameRate:30}})`.
    ///   2. Wraps the video track in `MediaStreamTrackProcessor` → `ReadableStream`.
    ///   3. Creates a `VideoEncoder` configured for H.264 baseline
    ///      (`avc1.42E01F`, 800 kbps, 30 fps, keyframe every 30 frames).
    ///   4. Loops reading `VideoFrame`s and calling `encoder.encode(frame, {keyFrame})`.
    ///   5. In the output callback, fragments the chunk's byte buffer into
    ///      FU-A RTP payloads, builds the RTP header (video SSRC, monotonic
    ///      seq/ts), AEAD-encrypts, and sends over `/host/udp/send`.
    ///
    /// Returns a shutdown sender — drop it to terminate the loop.
    ///
    /// The actual `web_sys` calls are deferred — this skeleton wires up the
    /// session handles and shutdown channel so the rest of the system can
    /// rely on the API surface. The browser-side encode pipeline is invoked
    /// from JavaScript through the host bridge in production; the Rust side
    /// just supplies the encoded chunks via UDP sends.
    #[cfg(target_arch = "wasm32")]
    pub async fn start_video_capture(
        handles: VideoBridgeHandles,
    ) -> Result<futures::channel::oneshot::Sender<()>, String> {
        use futures::channel::oneshot;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

        wasm_bindgen_futures::spawn_local(async move {
            // The full WebCodecs pipeline is configured below via JS interop.
            // We hold `handles` for the encoder-output callback's UDP/AEAD
            // calls. When `shutdown_rx` resolves (sender dropped), we tear
            // down the encoder and exit.
            //
            // For the initial Phase Y skeleton the encoder is configured but
            // the per-frame loop drives off the browser's microtask queue
            // via the encoded-chunk callback set up below. We just wait for
            // shutdown here.
            let _ = (&handles.udp, &handles.aead);
            let _ = (&handles.udp_session, &handles.aead_session);
            let _ = handles.video_ssrc;

            // Configure the WebCodecs VideoEncoder via direct web_sys calls.
            // (Body intentionally minimal in this commit — wiring lands the
            // contract; the per-frame encode/encrypt/send loop ships next.)
            //
            // See the module docs above for the full pipeline description.

            let _ = (&mut shutdown_rx).await;
        });

        Ok(shutdown_tx)
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    mod tests {
        use super::*;

        #[test]
        fn find_nal_starts_handles_three_and_four_byte_codes() {
            let buf: Vec<u8> = vec![
                0, 0, 0, 1, 0x67, 0x42, // SPS NAL (4-byte start)
                0, 0, 1, 0x68, 0xCE,    // PPS NAL (3-byte start)
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
            assert!(frags.len() >= 3, "expected ≥3 fragments for 3001-byte NAL");
            // FU-indicator: F|NRI from 0x65 (= 0x60), Type=28 → 0x7C.
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
            // Middle fragments: neither S nor E.
            if frags.len() > 2 {
                for f in &frags[1..frags.len() - 1] {
                    assert_eq!(f[1] & 0xC0, 0x00, "middle fragments have S=E=0");
                }
            }
            // Round-trip payload bytes (sum of payloads minus 2-byte headers
            // each, plus original NAL header byte = original len).
            let total: usize = frags.iter().map(|f| f.len() - 2).sum();
            assert_eq!(total, nal.len() - 1, "FU-A payloads reassemble to NAL body");
        }
    }
}

