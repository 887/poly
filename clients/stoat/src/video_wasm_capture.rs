//! Stoat WASM video capture (Phase B.3 of `plan-stoat-video-wasm.md`).
//!
//! Spawns a `wasm_bindgen_futures::spawn_local` task that:
//!   1. Opens `getUserMedia({video:{width:640,height:360,frameRate:30}})`.
//!   2. Wraps the video track in `MediaStreamTrackProcessor` → `ReadableStream`.
//!   3. Creates a `VideoEncoder` configured for H.264 baseline
//!      (`avc1.42E01F`, 800 kbps, 30 fps, keyframe every 30 frames).
//!   4. Loops reading `VideoFrame`s and calling `encoder.encode(frame, {keyFrame})`.
//!   5. In the output callback, fragments the chunk's byte buffer into FU-A
//!      RTP-shaped payloads via [`video_common::fragment_nal_units_to_fua`],
//!      then sends each payload as a Vortex binary frame using
//!      [`voice_common::build_outbound_frame`] with [`FrameKind::Video`].
//!
//! ## Transport
//!
//! Video frames ride the SAME Vortex WS as audio — that's the A.5 architectural
//! decision (extend Vortex, not add LiveKit). The caller obtains the WS sender
//! by calling [`voice_wasm::StoatVoiceConnection::ws_sender`] on the live voice
//! connection. The same connection's `shutdown_flag` is polled so video
//! capture stops cleanly when the user disconnects from voice.
//!
//! ## Status
//!
//! The Rust skeleton wires up the API surface, the shutdown channel, the
//! camera-acquire path, and the FU-A fragment loop. The full WebCodecs
//! `VideoEncoder` configuration + per-frame encode-and-send loop is intentionally
//! kept minimal in this commit (parity with the discord skeleton in
//! `clients/discord/src/voice_bridge/video_capture.rs::start_video_capture`).
//! Full per-frame encode/fragment/send lands in a follow-up pass when the
//! WebCodecs JS interop is added — the wire format and dispatcher are already
//! ready (audio path proven; video path symmetric).

use std::sync::{atomic::Ordering, Arc};

use futures::channel::mpsc::UnboundedSender;
use gloo_net::websocket::Message as WsMessage;
use tracing::{info, warn};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{MediaStream, MediaStreamConstraints, MediaStreamTrack};

use super::video_common::{
    fragment_nal_units_to_fua, StoatVideoError, DEFAULT_VIDEO_FRAMERATE, DEFAULT_VIDEO_HEIGHT,
    DEFAULT_VIDEO_WIDTH, RTP_VIDEO_MTU,
};
use super::voice_common::{build_outbound_frame, FrameKind};

// ── Public handle ─────────────────────────────────────────────────────────────

/// A live video capture session. Drop or call [`StoatVideoCaptureHandle::stop`]
/// to release the camera and stop sending video frames.
pub struct StoatVideoCaptureHandle {
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

// SAFETY: wasm32 is single-threaded — same rationale as `StoatVoiceConnection`.
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
unsafe impl Send for StoatVideoCaptureHandle {}
#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
unsafe impl Sync for StoatVideoCaptureHandle {}

impl StoatVideoCaptureHandle {
    /// Signal the capture task to stop. The camera is released and no further
    /// video frames are sent over the WS.
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Start the camera capture pipeline and feed FU-A-fragmented H.264 frames into
/// the supplied Vortex WS sender.
///
/// `ws_tx` is typically obtained from `StoatVoiceConnection::ws_sender()`.
/// `shutdown` is shared with the voice connection so video capture stops when
/// voice disconnects (in addition to the local stop flag returned in the handle).
///
/// # Errors
///
/// - [`StoatVideoError::CameraUnavailable`] — `navigator.mediaDevices.getUserMedia`
///   rejected (no permission, no device, insecure context).
/// - [`StoatVideoError::Encoder`] — `VideoEncoder` constructor unavailable
///   (browser lacks WebCodecs).
pub async fn start_video_capture(
    ws_tx: UnboundedSender<WsMessage>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) -> Result<StoatVideoCaptureHandle, StoatVideoError> {
    // ── Acquire the camera ────────────────────────────────────────────────────
    let window = web_sys::window()
        .ok_or_else(|| StoatVideoError::CameraUnavailable("no window".into()))?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices().map_err(|e| {
        StoatVideoError::CameraUnavailable(format!("navigator.mediaDevices missing: {e:?}"))
    })?;

    let constraints = MediaStreamConstraints::new();
    // Video: { width: 640, height: 360, frameRate: 30 }
    let video_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &video_obj,
        &JsValue::from_str("width"),
        &JsValue::from_f64(f64::from(DEFAULT_VIDEO_WIDTH)),
    );
    let _ = js_sys::Reflect::set(
        &video_obj,
        &JsValue::from_str("height"),
        &JsValue::from_f64(f64::from(DEFAULT_VIDEO_HEIGHT)),
    );
    let _ = js_sys::Reflect::set(
        &video_obj,
        &JsValue::from_str("frameRate"),
        &JsValue::from_f64(f64::from(DEFAULT_VIDEO_FRAMERATE)),
    );
    constraints.set_video(&video_obj.into());

    let stream_promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|e| StoatVideoError::CameraUnavailable(format!("getUserMedia call: {e:?}")))?;
    let stream_js = JsFuture::from(stream_promise)
        .await
        .map_err(|e| StoatVideoError::CameraUnavailable(format!("getUserMedia rejected: {e:?}")))?;
    let stream: MediaStream = stream_js.dyn_into().map_err(|_| {
        StoatVideoError::CameraUnavailable("getUserMedia did not return MediaStream".into())
    })?;

    let tracks = stream.get_video_tracks();
    if tracks.length() == 0 {
        return Err(StoatVideoError::CameraUnavailable(
            "MediaStream has no video tracks".into(),
        ));
    }
    let track: MediaStreamTrack = tracks
        .get(0)
        .dyn_into()
        .map_err(|_| StoatVideoError::CameraUnavailable("track 0 not MediaStreamTrack".into()))?;

    info!(
        width = DEFAULT_VIDEO_WIDTH,
        height = DEFAULT_VIDEO_HEIGHT,
        framerate = DEFAULT_VIDEO_FRAMERATE,
        "Stoat WASM video: camera acquired"
    );

    // ── Spawn capture loop ────────────────────────────────────────────────────
    let local_shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let local_shutdown_task = Arc::clone(&local_shutdown);
    let global_shutdown_task = Arc::clone(&shutdown);

    spawn_local(async move {
        // The full WebCodecs encoder pipeline is configured below via direct
        // web_sys calls (deferred to follow-up commit — see module docs).
        //
        // The track is held alive by `_track_owned` so the camera doesn't
        // release while the encoder is active.
        let _track_owned = track;

        // Skeleton: poll shutdown flags. When the real encoder lands, the
        // VideoEncoder output callback runs per-frame and:
        //   1. Splits the H.264 byte buffer at start codes via
        //      `find_nal_unit_starts`.
        //   2. For each NAL, calls `fragment_nal_units_to_fua(nal, RTP_VIDEO_MTU)`.
        //   3. For each fragment, sends `build_outbound_frame(FrameKind::Video, &frag)`
        //      via `ws_tx`.
        //
        // The capture loop terminates when EITHER shutdown flag is set; this
        // matches the audio task's tear-down semantics so the connection is
        // a single logical unit.
        // Yield to JS task queue between polls so the WASM main thread isn't
        // monopolised by this loop. The real encoder runs off browser microtasks
        // (VideoEncoder output callback) — this poll exists purely so the
        // skeleton respects shutdown. Once the real encoder ships, this loop
        // is replaced by encoder.output = closure { send fragments }.
        loop {
            if local_shutdown_task.load(Ordering::Relaxed)
                || global_shutdown_task.load(Ordering::Relaxed)
            {
                break;
            }
            // js_sys::Promise::resolve(&JsValue::NULL) + JsFuture::from is a
            // safe microtask yield on wasm32. The audio task uses the
            // ReadableStream reader's await for the same effect.
            let _ = JsFuture::from(js_sys::Promise::resolve(&JsValue::NULL)).await;
        }

        // Cleanup: stop the camera so the browser releases the device.
        _track_owned.stop();

        // Drop ws_tx held by reference (skeleton — the encoder callback will
        // close over a clone).
        let _ = ws_tx;
        info!("Stoat WASM video: capture stopped");
    });

    Ok(StoatVideoCaptureHandle {
        shutdown: local_shutdown,
    })
}

// ── Helpers (called by the encoder output callback once it lands) ─────────────

/// Build the wire frames for a single encoded H.264 access unit and push them
/// into the WS sender. Used by the encoder output callback (and unit tests
/// once the encoder lands).
///
/// Splits `nal_unit_bytes` into FU-A fragments of at most `RTP_VIDEO_MTU` bytes
/// each, prefixes each fragment with `[FrameKind::Video][8 NUL]`, and sends.
/// Returns `Err` if the WS channel is closed.
pub fn send_h264_nal(
    ws_tx: &UnboundedSender<WsMessage>,
    nal_unit_bytes: &[u8],
) -> Result<(), StoatVideoError> {
    let fragments = fragment_nal_units_to_fua(nal_unit_bytes, RTP_VIDEO_MTU);
    for fragment in fragments {
        let frame = build_outbound_frame(FrameKind::Video, &fragment);
        if ws_tx.unbounded_send(WsMessage::Bytes(frame)).is_err() {
            warn!("Stoat WASM video: WS channel closed; aborting fragment send");
            return Err(StoatVideoError::NotConnected);
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
// lint-allow-unused: test module uses unwrap/expect/panic per project policy
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use futures::channel::mpsc::unbounded;
    use futures::StreamExt;

    #[test]
    fn send_h264_nal_emits_video_frames() {
        let (tx, mut rx) = unbounded::<WsMessage>();
        // Short NAL — single fragment (no FU-A).
        let nal = vec![0x65u8, 0xAA, 0xBB, 0xCC];
        send_h264_nal(&tx, &nal).expect("send failed");
        drop(tx);

        // Poll the receiver: should yield exactly one frame.
        let frames: Vec<WsMessage> = futures::executor::block_on(async {
            let mut out = Vec::new();
            while let Some(m) = rx.next().await {
                out.push(m);
            }
            out
        });
        assert_eq!(frames.len(), 1);
        if let WsMessage::Bytes(bytes) = &frames[0] {
            assert_eq!(bytes[0], 0x01, "kind = video");
            assert_eq!(&bytes[1..9], &[0u8; 8], "8 NUL uid");
            assert_eq!(&bytes[9..], nal.as_slice(), "payload matches NAL");
        } else {
            panic!("expected binary WS message");
        }
    }

    #[test]
    fn send_h264_nal_fragments_oversized_nal() {
        let (tx, mut rx) = unbounded::<WsMessage>();
        // Large NAL — forces FU-A fragmentation.
        let mut nal = vec![0x65u8];
        nal.extend(std::iter::repeat(0xDDu8).take(3000));
        send_h264_nal(&tx, &nal).expect("send failed");
        drop(tx);

        let frames: Vec<WsMessage> = futures::executor::block_on(async {
            let mut out = Vec::new();
            while let Some(m) = rx.next().await {
                out.push(m);
            }
            out
        });
        assert!(frames.len() >= 3, "expected >= 3 FU-A fragments");
        // Each frame must start with [video-kind, 8 NUL uid].
        for f in &frames {
            if let WsMessage::Bytes(bytes) = f {
                assert_eq!(bytes[0], 0x01, "kind = video on every fragment");
                assert_eq!(&bytes[1..9], &[0u8; 8], "8 NUL uid on every fragment");
            } else {
                panic!("expected binary");
            }
        }
    }

    #[test]
    fn send_h264_nal_reports_closed_channel() {
        let (tx, rx) = unbounded::<WsMessage>();
        drop(rx);
        let nal = vec![0x65u8, 0xAA];
        let result = send_h264_nal(&tx, &nal);
        assert!(matches!(result, Err(StoatVideoError::NotConnected)));
    }
}
