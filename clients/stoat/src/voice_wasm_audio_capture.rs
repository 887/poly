//! Wasm-only mic capture for Stoat voice (Phase B.3).
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
//!     │  - copy planar Float32 → downmix to mono → resample to 48 kHz → i16
//!     ▼
//! 960-sample mono frames (20 ms @ 48 kHz)  — Vec<i16> items on the Stream
//! ```
//!
//! Output: `impl futures::Stream<Item = Vec<i16>>` yielded from `open_mic_stream`.
//! Each `Vec<i16>` contains exactly 960 samples (mono, 48 kHz).
//!
//! Adapted from `clients/discord/src/voice_bridge/audio_capture.rs` (Phase X.2).
//! Stoat is mono (1 channel), not stereo, so we downmix L+R rather than
//! building a stereo interleaved frame.
//!
//! ## Browser support
//!
//! `MediaStreamTrackProcessor` is Chromium-only (Chrome ≥ 94, Edge, Electron).
//! Poly only ships against Chromium-based shells so no Firefox fallback is
//! needed. If the constructor throws the function returns
//! `Err(StoatVoiceError::AudioInit(…))`.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use futures::channel::mpsc;
use js_sys::{Float32Array, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioData, AudioDataCopyToOptions, MediaStream, MediaStreamConstraints,
    MediaStreamTrack, MediaStreamTrackProcessor, MediaStreamTrackProcessorInit,
    ReadableStreamDefaultReader,
};

use super::voice_common::{StoatVoiceError, OPUS_FRAME_SAMPLES};
use super::voice_noise_filter::{apply_rnnoise, NoiseFilter};

// ── Public entry point ────────────────────────────────────────────────────────

/// Open the default microphone and return a stream of 960-sample mono i16 PCM
/// frames (20 ms at 48 kHz).
///
/// # Noise cancellation (B.8)
///
/// When `noise_cancel_enabled` is `true` at frame time, each mono f32 frame is
/// processed through an [`nnnoiseless::DenoiseState`] (RNNoise) before the
/// float32→i16 conversion.  The filter is applied in-place on the f32 buffer so
/// no extra allocation is needed.  Toggling the `AtomicBool` takes effect on the
/// very next 480-sample chunk — there is no audio gap or reconnect required.
///
/// The stream runs until dropped. Dropping the stream stops the mic track and
/// releases the browser device lock.
///
/// Failure cases (returned before the stream is created):
/// - `navigator.mediaDevices` missing (insecure context or headless w/o flag)
/// - `getUserMedia({audio:true})` rejected (permission denied / no device)
/// - No audio track on the returned `MediaStream`
/// - `MediaStreamTrackProcessor` constructor throws
pub async fn open_mic_stream(
    noise_cancel_enabled: Arc<AtomicBool>,
) -> Result<impl futures::Stream<Item = Vec<i16>> + 'static, StoatVoiceError> {
    // 1. Acquire mic stream.
    let window =
        web_sys::window().ok_or_else(|| StoatVoiceError::AudioInit("no window".into()))?;
    let navigator = window.navigator();
    let media_devices = navigator
        .media_devices()
        .map_err(|e| StoatVoiceError::AudioInit(format!("navigator.mediaDevices missing: {e:?}")))?;

    let constraints = MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::TRUE);
    let stream_promise = media_devices
        .get_user_media_with_constraints(&constraints)
        .map_err(|e| StoatVoiceError::AudioInit(format!("getUserMedia call failed: {e:?}")))?;
    let stream_js = JsFuture::from(stream_promise)
        .await
        .map_err(|e| StoatVoiceError::AudioInit(format!("getUserMedia rejected: {e:?}")))?;
    let stream: MediaStream = stream_js
        .dyn_into()
        .map_err(|_| StoatVoiceError::AudioInit("getUserMedia did not return a MediaStream".into()))?;

    // 2. Take the first audio track.
    let tracks = stream.get_audio_tracks();
    if tracks.length() == 0 {
        return Err(StoatVoiceError::AudioInit(
            "MediaStream has no audio tracks".into(),
        ));
    }
    let track: MediaStreamTrack = tracks
        .get(0)
        .dyn_into()
        .map_err(|_| StoatVoiceError::AudioInit("track 0 is not a MediaStreamTrack".into()))?;

    // 3. Build the processor + reader.
    let init = MediaStreamTrackProcessorInit::new(&track);
    let processor = MediaStreamTrackProcessor::new(&init).map_err(|e| {
        StoatVoiceError::AudioInit(format!("MediaStreamTrackProcessor unavailable: {e:?}"))
    })?;
    let readable = processor.readable();
    let reader: ReadableStreamDefaultReader = readable
        .get_reader()
        .dyn_into()
        .map_err(|_| {
            StoatVoiceError::AudioInit(
                "ReadableStream.getReader did not return a default reader".into(),
            )
        })?;

    // 4. Create a bounded channel. The buffer of 8 frames means the spawned
    //    task can stay slightly ahead of the consumer without unbounded growth.
    let (mut tx, rx) = mpsc::channel::<Vec<i16>>(8);

    wasm_bindgen_futures::spawn_local(async move {
        let _track_owned = track; // keep mic track alive for the loop's lifetime
        let mut frame_buf: Vec<i16> = Vec::with_capacity(OPUS_FRAME_SAMPLES * 2);
        // B.8 — one RNNoise state per voice session. Allocated once; the
        // recurrent model state is preserved across chunks so the denoiser
        // can track background noise over the session lifetime.
        let mut noise_filter = NoiseFilter::new();

        loop {
            // Stop if the consumer dropped the receiver.
            if tx.is_closed() {
                break;
            }

            let chunk = match JsFuture::from(reader.read()).await {
                Ok(c) => c,
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("stoat audio_capture: reader.read rejected: {e:?}").into(),
                    );
                    break;
                }
            };

            let (done, value) = read_chunk_parts(&chunk);
            if done {
                break;
            }
            let Some(audio_data) = value else {
                continue;
            };

            // B.8 — pass the per-session filter + live toggle flag.
            let nc_enabled = noise_cancel_enabled.load(Ordering::Relaxed);
            process_audio_data(&audio_data, &mut frame_buf, &mut tx, nc_enabled, &mut noise_filter);
            audio_data.close();

            // If the consumer fell behind and closed, stop.
            if tx.is_closed() {
                break;
            }
        }

        // Cleanup: stop the mic track and cancel the reader so the browser
        // releases the device immediately.
        let _ = reader.cancel();
        _track_owned.stop();
    });

    Ok(rx)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Extract `{done, value}` from a `ReadableStream` chunk object.
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

/// Decode one `AudioData` chunk into mono i16 PCM and push complete
/// 960-sample frames into `tx`. Partial frames are buffered in `frame_buf`.
///
/// # Noise cancellation (B.8)
///
/// When `noise_cancel` is `true`, the mono f32 buffer is scaled to i16 range
/// (`[-32768, 32767]`), processed through the RNNoise denoiser (`filter`), then
/// scaled back to `[-1.0, 1.0]` before the existing `float32_to_i16` step.
/// This matches the value range expected by `nnnoiseless::DenoiseState::process_frame`.
fn process_audio_data(
    audio_data: &AudioData,
    frame_buf: &mut Vec<i16>,
    tx: &mut mpsc::Sender<Vec<i16>>,
    noise_cancel: bool,
    filter: &mut NoiseFilter,
) {
    let sample_rate = audio_data.sample_rate() as u32;
    let channels = audio_data.number_of_channels();
    let frames = audio_data.number_of_frames() as usize;
    if frames == 0 {
        return;
    }

    // Pull plane 0 (left channel / mono source).
    let plane0 = match copy_plane_f32(audio_data, 0, frames) {
        Ok(p) => p,
        Err(e) => {
            web_sys::console::warn_1(
                &format!("stoat audio_capture: copy_plane_f32(0) error: {e}").into(),
            );
            return;
        }
    };

    // Optionally pull plane 1 (right channel) for stereo downmix.
    let plane1 = if channels >= 2 {
        copy_plane_f32(audio_data, 1, frames).ok()
    } else {
        None
    };

    // Downmix to mono f32.
    let mut mono: Vec<f32> = Vec::with_capacity(frames);
    match plane1 {
        Some(right) => {
            for i in 0..frames {
                mono.push((plane0[i] + right[i]) * 0.5);
            }
        }
        None => {
            mono.extend_from_slice(&plane0);
        }
    }

    // Resample to 48 kHz if the source rate differs.
    let mut resampled = if sample_rate == 48_000 {
        mono
    } else {
        resample_mono_linear(&mono, sample_rate, 48_000)
    };

    // B.8 — RNNoise noise cancellation (when enabled).
    //
    // nnnoiseless expects samples in i16 scale ([-32768, 32767]), not [-1.0, 1.0].
    // We scale up, filter, then scale back down so the existing float32_to_i16
    // helper continues to work unmodified.
    if noise_cancel {
        const I16_MAX_F: f32 = i16::MAX as f32; // 32767.0
        for s in &mut resampled {
            *s *= I16_MAX_F;
        }
        apply_rnnoise(&mut resampled, filter);
        for s in &mut resampled {
            *s /= I16_MAX_F;
        }
    }

    // Float32 → i16.
    let pcm_i16 = float32_to_i16(&resampled);
    frame_buf.extend_from_slice(&pcm_i16);

    // Flush complete 960-sample frames.
    for chunk in frame_buf.chunks_exact(OPUS_FRAME_SAMPLES) {
        if tx.try_send(chunk.to_vec()).is_err() {
            // Consumer is behind or closed — stop flushing this batch.
            break;
        }
    }
    // Retain the leftover partial frame for the next AudioData chunk.
    let leftover = frame_buf.len() % OPUS_FRAME_SAMPLES;
    let keep_from = frame_buf.len() - leftover;
    frame_buf.drain(..keep_from);
}

/// Copy one planar channel of an `AudioData` chunk into a `Vec<f32>`.
fn copy_plane_f32(
    audio_data: &AudioData,
    plane_index: u32,
    frames: usize,
) -> Result<Vec<f32>, String> {
    let opts = AudioDataCopyToOptions::new(plane_index);
    let buf = Float32Array::new_with_length(frames as u32);
    audio_data
        .copy_to_with_buffer_source(&buf, &opts)
        .map_err(|e| format!("AudioData.copyTo: {e:?}"))?;
    let mut out = vec![0.0_f32; frames];
    buf.copy_to(&mut out);
    Ok(out)
}

// ── DSP helpers (unit-testable on all targets) ────────────────────────────────

/// Linear-interpolate mono `input` from `src_rate` to `dst_rate`.
/// Good enough for a voice path — Opus tolerates linear resampling fine at
/// voice source rates.
pub(crate) fn resample_mono_linear(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || input.is_empty() {
        return input.to_vec();
    }
    let in_frames = input.len();
    let ratio = src_rate as f64 / dst_rate as f64;
    let out_frames = ((in_frames as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_frames);
    for i in 0..out_frames {
        let src_pos = i as f64 * ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let idx_next = (idx + 1).min(in_frames - 1);
        let sample = input[idx] * (1.0 - frac) + input[idx_next] * frac;
        out.push(sample);
    }
    out
}

/// Convert f32 samples in \[-1.0, 1.0\] to i16 PCM. Out-of-range values are
/// clamped before scaling.
pub(crate) fn float32_to_i16(input: &[f32]) -> Vec<i16> {
    input
        .iter()
        .map(|&s| {
            let clamped = s.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32) as i16
        })
        .collect()
}

// ── Unit tests ────────────────────────────────────────────────────────────────

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
        // 0.5 → ~16383, -0.5 → ~-16383
        assert!((out[3] as i32 - (i16::MAX as i32 / 2)).abs() <= 1);
        assert!((out[4] as i32 - (-(i16::MAX as i32) / 2)).abs() <= 1);
        // Clamping: 2.0 → +max, -2.0 → -max.
        assert_eq!(out[5], i16::MAX);
        assert_eq!(out[6], -i16::MAX);
    }

    #[test]
    fn resample_mono_passthrough_same_rate() {
        let input = vec![0.1_f32, -0.1, 0.2, -0.2, 0.3, -0.3];
        let out = resample_mono_linear(&input, 48_000, 48_000);
        assert_eq!(out, input);
    }

    #[test]
    fn resample_mono_downsamples_length() {
        // 96 kHz → 48 kHz should halve the frame count.
        let input: Vec<f32> = (0..200).map(|i| i as f32 / 200.0).collect();
        let out = resample_mono_linear(&input, 96_000, 48_000);
        assert_eq!(out.len(), 100);
    }

    #[test]
    fn resample_mono_upsamples_length() {
        // 24 kHz → 48 kHz should double the frame count.
        let input: Vec<f32> = (0..50).map(|i| i as f32 / 50.0).collect();
        let out = resample_mono_linear(&input, 24_000, 48_000);
        assert_eq!(out.len(), 100);
    }

    #[test]
    fn resample_mono_empty_input() {
        let out = resample_mono_linear(&[], 24_000, 48_000);
        assert!(out.is_empty());
    }
}
