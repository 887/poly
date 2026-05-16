//! Wasm-only audio capture loop (Phase X.2).
//!
//! Pipeline:
//!
//! ```text
//! navigator.mediaDevices.getUserMedia({audio: true})
//!     │
//!     ▼
//! MediaStreamTrack (audio)
//!     │
//!     ▼
//! MediaStreamTrackProcessor.readable  (ReadableStream<AudioData>)
//!     │
//!     ▼  per-chunk AudioData (WebCodecs)
//!     │  - copy planar/interleaved Float32 → resample to 48 kHz stereo i16
//!     ▼
//! 1920-sample stereo frames (20 ms @ 48 kHz, stereo)
//!     │
//!     ▼
//! /host/codec/opus/encoder/encode → /host/aead/encrypt → RTP wrap → /host/udp/send
//! ```
//!
//! Runs as a `wasm_bindgen_futures::spawn_local` task. Shutdown is signalled
//! by dropping the returned `oneshot::Sender<()>` — the loop races each
//! `ReadableStreamDefaultReader::read` against the receiver and exits on
//! either branch resolving.
//!
//! ## Browser support
//!
//! `MediaStreamTrackProcessor` is Chromium-only (Chrome ≥ 94, Edge, Electron).
//! Poly only ships against Chromium-based shells (poly-web Chromium,
//! poly-desktop-electron) so no Firefox fallback is needed. If the constructor
//! throws (older Chromium / disabled flag) the loop returns
//! `Err("MediaStreamTrackProcessor unavailable: …")`.
//!
//! ## Runtime verification
//!
//! End-to-end coverage runs in Phase X.4 cross-shell smoke test using
//! Chromium's `--use-fake-device-for-media-stream` so getUserMedia resolves
//! without a real microphone. Unit tests in this file cover the
//! Float32 → i16 conversion + linear resampling helpers only.

// The browser-facing parts of this module only compile for wasm32; the
// pure DSP helpers + their unit tests build on every target so
// `cargo test` exercises them natively.

#[cfg(target_arch = "wasm32")]
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::{AtomicU16, AtomicU32, Ordering};

#[cfg(target_arch = "wasm32")]
use futures::channel::oneshot;
#[cfg(target_arch = "wasm32")]
use futures::future::{select, Either};
#[cfg(target_arch = "wasm32")]
use futures::FutureExt;
#[cfg(target_arch = "wasm32")]
use js_sys::{Float32Array, Object, Reflect};
#[cfg(target_arch = "wasm32")]
use poly_host_bridge::{
    aead_client::AeadClient, codec_opus_client::OpusClient, udp_client::UdpClient,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{
    AudioData, AudioDataCopyToOptions, MediaStream, MediaStreamConstraints,
    MediaStreamTrack, MediaStreamTrackProcessor, MediaStreamTrackProcessorInit,
    ReadableStreamDefaultReader,
};

#[cfg(target_arch = "wasm32")]
use crate::voice_bridge::{
    build_rtp_header, xchacha_nonce_from_rtp, OPUS_FRAME_SAMPLES, RTP_HEADER_SIZE,
};

/// Snapshot of the session fields the capture loop needs. Avoids holding
/// the session mutex across awaits in the hot send path.
#[cfg(target_arch = "wasm32")]
pub struct CaptureParams {
    pub udp: Arc<UdpClient>,
    pub opus: Arc<OpusClient>,
    pub aead: Arc<AeadClient>,
    pub udp_session: String,
    pub encoder_session: String,
    pub aead_session: String,
    pub local_ssrc: u32,
    pub rtp_sequence: Arc<AtomicU16>,
    pub rtp_timestamp: Arc<AtomicU32>,
}

/// Spawn the capture loop. Returns Ok with the shutdown sender on success —
/// the loop runs in the background. Drop the sender to shut the loop down
/// on its next iteration.
///
/// Failure cases (returned synchronously, before spawning):
///   - `navigator.mediaDevices` missing (insecure context, headless w/o flag)
///   - `getUserMedia({audio:true})` rejection (permission denied / no device)
///   - no audio track on the returned MediaStream
///   - `MediaStreamTrackProcessor` constructor throws
#[cfg(target_arch = "wasm32")]
pub async fn start_audio_capture(
    params: CaptureParams,
) -> Result<oneshot::Sender<()>, String> {
    // 1. Acquire mic stream.
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let navigator = window.navigator();
    let media_devices = navigator
        .media_devices()
        .map_err(|e| format!("navigator.mediaDevices missing: {e:?}"))?;

    let constraints = MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::TRUE);
    let stream_promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|e| format!("getUserMedia call failed: {e:?}"))?;
    let stream_js = JsFuture::from(stream_promise)
        .await
        .map_err(|e| format!("getUserMedia rejected: {e:?}"))?;
    let stream: MediaStream = stream_js
        .dyn_into()
        .map_err(|_| "getUserMedia did not return a MediaStream".to_string())?;

    // 2. Take the first audio track.
    let tracks = stream.get_audio_tracks();
    if tracks.length() == 0 {
        return Err("MediaStream has no audio tracks".into());
    }
    let track: MediaStreamTrack = tracks
        .get(0)
        .dyn_into()
        .map_err(|_| "track 0 is not a MediaStreamTrack".to_string())?;

    // 3. Build the processor + reader.
    let init = MediaStreamTrackProcessorInit::new(&track);
    let processor = MediaStreamTrackProcessor::new(&init)
        .map_err(|e| format!("MediaStreamTrackProcessor unavailable: {e:?}"))?;
    let readable = processor.readable();
    let reader: ReadableStreamDefaultReader = readable
        .get_reader()
        .dyn_into()
        .map_err(|_| "ReadableStream.getReader did not return a default reader".to_string())?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // 4. Spawn the loop.
    wasm_bindgen_futures::spawn_local(async move {
        let _track_owned = track; // keep alive for the lifetime of the loop
        let mut shutdown = shutdown_rx.fuse();
        let mut frame_buf: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);

        loop {
            let read_fut = JsFuture::from(reader.read()).fuse();
            futures::pin_mut!(read_fut);

            match select(&mut shutdown, read_fut).await {
                Either::Left(_) => {
                    // Shutdown signalled (sender dropped or sent ()).
                    break;
                }
                Either::Right((Ok(chunk), _)) => {
                    let (done, value) = read_chunk_parts(&chunk);
                    if done {
                        break;
                    }
                    let Some(audio_data) = value else { continue };

                    if let Err(e) = process_audio_data(
                        &audio_data,
                        &mut frame_buf,
                        &params,
                    )
                    .await
                    {
                        // Don't tear the loop down — log + continue. A
                        // single bad packet shouldn't kill the call.
                        web_sys::console::warn_1(
                            &format!("audio_capture: process error: {e}").into(),
                        );
                    }
                    audio_data.close();
                }
                Either::Right((Err(e), _)) => {
                    web_sys::console::warn_1(
                        &format!("audio_capture: reader.read rejected: {e:?}").into(),
                    );
                    break;
                }
            }
        }

        // Cleanup: stop the mic track and cancel the reader so the browser
        // releases the device immediately.
        let _ = reader.cancel();
        _track_owned.stop();
    });

    Ok(shutdown_tx)
}

/// Extract `{done, value}` from a ReadableStream chunk object.
#[cfg(target_arch = "wasm32")]
fn read_chunk_parts(chunk: &JsValue) -> (bool, Option<AudioData>) {
    let obj: &Object = match chunk.dyn_ref::<Object>() {
        Some(o) => o,
        None => return (true, None),
    };
    let done = Reflect::get(obj, &JsValue::from_str("done"))
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let value = Reflect::get(obj, &JsValue::from_str("value"))
        .ok()
        .and_then(|v| v.dyn_into::<AudioData>().ok());
    (done, value)
}

/// Decode one `AudioData` chunk into i16 PCM samples, append to `frame_buf`,
/// and flush full 1920-sample frames through the encode + send path.
#[cfg(target_arch = "wasm32")]
async fn process_audio_data(
    audio_data: &AudioData,
    frame_buf: &mut Vec<i16>,
    params: &CaptureParams,
) -> Result<(), String> {
    let sample_rate = audio_data.sample_rate() as u32;
    let channels = audio_data.number_of_channels();
    let frames = audio_data.number_of_frames();
    if frames == 0 {
        return Ok(());
    }

    // Pull plane 0 (interleaved or first channel) as f32.
    let plane0 = copy_plane_f32(audio_data, 0, frames as usize)?;

    // If multichannel and not interleaved, also pull plane 1 for L/R.
    let plane1 = if channels >= 2 {
        copy_plane_f32(audio_data, 1, frames as usize).ok()
    } else {
        None
    };

    // Build stereo f32 sequence: [L, R, L, R, ...].
    let mut stereo: Vec<f32> = Vec::with_capacity(frames as usize * 2);
    match plane1 {
        Some(right) => {
            for i in 0..(frames as usize) {
                stereo.push(plane0[i]);
                stereo.push(right[i]);
            }
        }
        None => {
            // Mono → duplicate.
            for s in plane0.iter() {
                stereo.push(*s);
                stereo.push(*s);
            }
        }
    }

    // Resample to 48 kHz stereo if needed.
    let resampled = if sample_rate == 48_000 {
        stereo
    } else {
        resample_stereo_linear(&stereo, sample_rate, 48_000)
    };

    // Float32 → i16 LE PCM. Clamp to [-1.0, 1.0].
    let pcm_i16 = float32_to_i16(&resampled);
    frame_buf.extend_from_slice(&pcm_i16);

    // Flush every full 1920-sample stereo frame.
    while frame_buf.len() >= OPUS_FRAME_SAMPLES {
        // Drain exactly OPUS_FRAME_SAMPLES (1920 stereo samples = 960 per channel).
        let frame: Vec<i16> = frame_buf.drain(..OPUS_FRAME_SAMPLES).collect();
        send_frame(&frame, params).await?;
    }
    Ok(())
}

/// Encode + encrypt + UDP-send one 1920-sample stereo frame using the
/// snapshotted primitives. Replicates the body of
/// `DiscordVoiceBridgeClient::send_audio_frame` so the capture task does
/// not need to re-acquire the session mutex per frame.
#[cfg(target_arch = "wasm32")]
async fn send_frame(pcm: &[i16], p: &CaptureParams) -> Result<(), String> {
    let opus_packet = p
        .opus
        .encode(&p.encoder_session, pcm)
        .await
        .map_err(|e| format!("opus.encode: {e}"))?;

    let sequence = p.rtp_sequence.fetch_add(1, Ordering::Relaxed);
    let timestamp = p
        .rtp_timestamp
        .fetch_add((OPUS_FRAME_SAMPLES / 2) as u32, Ordering::Relaxed);
    let rtp_header = build_rtp_header(sequence, timestamp, p.local_ssrc);
    let nonce = xchacha_nonce_from_rtp(&rtp_header);

    let ciphertext = p
        .aead
        .encrypt(&p.aead_session, &nonce, &opus_packet, Some(&rtp_header))
        .await
        .map_err(|e| format!("aead.encrypt: {e}"))?;

    let mut packet = Vec::with_capacity(RTP_HEADER_SIZE + ciphertext.len());
    packet.extend_from_slice(&rtp_header);
    packet.extend_from_slice(&ciphertext);

    p.udp
        .send(&p.udp_session, &packet, None)
        .await
        .map_err(|e| format!("udp.send: {e}"))?;
    Ok(())
}

/// Copy one planar channel of an `AudioData` chunk into a `Vec<f32>`.
#[cfg(target_arch = "wasm32")]
fn copy_plane_f32(
    audio_data: &AudioData,
    plane_index: u32,
    frames: usize,
) -> Result<Vec<f32>, String> {
    let opts = AudioDataCopyToOptions::new(plane_index);
    let buf = Float32Array::new_with_length(frames as u32);
    // `copy_to_with_buffer_source` returns a Promise; the AudioData.copyTo
    // sync overload writes directly into the supplied ArrayBufferView and
    // returns the byte count synchronously. web-sys exposes both shapes —
    // pick the synchronous one via the BufferSource overload because
    // AudioData.copyTo is synchronous per spec.
    audio_data
        .copy_to_with_buffer_source(&buf, &opts)
        .map_err(|e| format!("AudioData.copyTo: {e:?}"))?;
    let mut out = vec![0.0_f32; frames];
    buf.copy_to(&mut out);
    Ok(out)
}

// ── Helpers (unit-testable) ──────────────────────────────────────────────

/// Linear-interpolate stereo `input` from `src_rate` to `dst_rate`. Input is
/// interleaved L/R; output is the same layout. Good enough for a voice path —
/// production-grade audio would use a polyphase filter, but Opus tolerates
/// linear resampling fine at the source.
pub(crate) fn resample_stereo_linear(
    input: &[f32],
    src_rate: u32,
    dst_rate: u32,
) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }
    let in_frames = input.len() / 2;
    if in_frames == 0 {
        return Vec::new();
    }
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_frames = ((in_frames as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let idx_next = (idx + 1).min(in_frames - 1);
        let l = input[idx * 2] * (1.0 - frac) + input[idx_next * 2] * frac;
        let r = input[idx * 2 + 1] * (1.0 - frac) + input[idx_next * 2 + 1] * frac;
        out.push(l);
        out.push(r);
    }
    out
}

/// Convert f32 samples in [-1.0, 1.0] to i16 LE PCM. Out-of-range values
/// are clamped before scaling.
pub(crate) fn float32_to_i16(input: &[f32]) -> Vec<i16> {
    input
        .iter()
        .map(|&s| {
            let clamped = s.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32) as i16
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn float32_to_i16_clamps_and_scales() {
        let input = [0.0_f32, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0];
        let out = float32_to_i16(&input);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], i16::MAX);
        assert_eq!(out[2], -i16::MAX);
        assert!((out[3] as i32 - (i16::MAX as i32 / 2)).abs() <= 1);
        assert!((out[4] as i32 - (-(i16::MAX as i32) / 2)).abs() <= 1);
        // Clamping: 2.0 → +max, -2.0 → -max.
        assert_eq!(out[5], i16::MAX);
        assert_eq!(out[6], -i16::MAX);
    }

    #[test]
    fn resample_passthrough_same_rate() {
        let input = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3];
        let out = resample_stereo_linear(&input, 48_000, 48_000);
        assert_eq!(out, input);
    }

    #[test]
    fn resample_downsamples_length() {
        // 96 kHz → 48 kHz should halve the frame count.
        let mut input = Vec::new();
        for i in 0..200 {
            input.push(i as f32 / 200.0);
            input.push(-(i as f32) / 200.0);
        }
        let out = resample_stereo_linear(&input, 96_000, 48_000);
        // 200 input frames at ratio 2.0 → 100 output frames → 200 samples.
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn resample_upsamples_length() {
        // 24 kHz → 48 kHz should double the frame count.
        let mut input = Vec::new();
        for i in 0..50 {
            input.push(i as f32 / 50.0);
            input.push(-(i as f32) / 50.0);
        }
        let out = resample_stereo_linear(&input, 24_000, 48_000);
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn resample_empty_input() {
        let out = resample_stereo_linear(&[], 24_000, 48_000);
        assert!(out.is_empty());
    }
}
