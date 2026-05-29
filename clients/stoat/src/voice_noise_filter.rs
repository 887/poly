//! RNNoise-based noise-cancellation filter for the Stoat voice pipeline (B.8).
//!
//! This module is **cfg-free** — it compiles on both native and `wasm32-unknown-unknown`.
//! `nnnoiseless` is a pure-Rust port of Mozilla's RNNoise library and has no C
//! dependencies, so it is safe to link on WASM targets.
//!
//! # Integration point
//!

// lint-allow-unused: noise filter is wired in voice_wasm_audio_capture.rs
// and voice_wasm.rs (both wasm32-only); native builds see it as unused.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

//! The filter sits between the raw mic samples (Float32 planar, 48 kHz mono,
//! values already in [-32768, 32767] scale) and the final i16 conversion in
//! `voice_wasm_audio_capture::process_audio_data`.
//!
//! # Frame size
//!
//! `DenoiseState::FRAME_SIZE` = 480 samples (10 ms at 48 kHz).
//! Our Opus frames are 960 samples (20 ms), so `apply_rnnoise_inplace` processes
//! two 480-sample chunks per Opus frame.  The caller is responsible for feeding
//! complete `DENOISE_FRAME` multiples; partial frames pass through unfiltered.
//!
//! # Value range
//!
//! nnnoiseless expects input in the range `[-32768.0, 32767.0]` (i16 scale), NOT
//! the `[-1.0, 1.0]` range common for floating-point audio.  The audio-capture
//! module keeps mono samples in f32 with i16 scale before the `float32_to_i16`
//! step, which makes the call sites straightforward.
//!
//! # Runtime toggle
//!
//! The caller holds an `Arc<AtomicBool>` (`noise_cancel_enabled`) and checks it
//! on every frame.  When the user toggles noise cancellation off, the filter is
//! bypassed from the very next frame with no gap in the audio stream.
//!
//! # DECISION(B.8)
//!
//! RNNoise is wired via `nnnoiseless` (pure Rust, wasm32-compatible).
//! The DSP runs inline in the WASM audio-capture task — no AudioWorklet, no
//! additional host-bridge route, no extra thread.  The `DenoiseState` is
//! allocated once per voice session and kept behind a `Box` to avoid stack
//! pressure from the large internal buffers.

use nnnoiseless::DenoiseState;

/// Frame size expected by RNNoise / nnnoiseless (480 samples = 10 ms at 48 kHz).
pub const DENOISE_FRAME: usize = DenoiseState::FRAME_SIZE;

/// Stateful RNNoise denoiser for a mono 48 kHz audio stream.
///
/// Allocates one `DenoiseState` on creation.  Reuse the same instance across
/// calls within a single voice session so the model's recurrent state is
/// preserved between frames.
pub struct NoiseFilter {
    state: Box<DenoiseState<'static>>,
    /// Scratch buffer reused per `process_chunk` call.
    out_buf: [f32; DENOISE_FRAME],
    /// Whether this is the first processed frame.  nnnoiseless has a one-frame
    /// look-ahead latency: the very first output frame contains fade-in
    /// artifacts.  We replace it with silence (zeros) rather than discarding
    /// it, so the output stream length is always equal to the input stream.
    first_frame: bool,
}

impl NoiseFilter {
    /// Create a new [`NoiseFilter`] ready to process mono 48 kHz PCM.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            out_buf: [0.0; DENOISE_FRAME],
            first_frame: true,
        }
    }

    /// Process one 480-sample chunk in-place.
    ///
    /// `samples` must have length exactly [`DENOISE_FRAME`] (480).
    /// Values are expected in the range `[-32768.0, 32767.0]` (i16 scale).
    ///
    /// On the first call the output is zeroed out to avoid the one-frame
    /// look-ahead artifact.
    pub fn process_chunk(&mut self, samples: &mut [f32]) {
        debug_assert_eq!(
            samples.len(),
            DENOISE_FRAME,
            "process_chunk: expected {DENOISE_FRAME} samples, got {}",
            samples.len()
        );

        self.state.process_frame(&mut self.out_buf, samples);

        if self.first_frame {
            // Replace look-ahead-artifact frame with silence.
            samples.fill(0.0);
            self.first_frame = false;
        } else {
            samples.copy_from_slice(&self.out_buf);
        }
    }
}

impl Default for NoiseFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply RNNoise filtering to a mutable slice of mono f32 PCM samples.
///
/// - `samples`: mono PCM in i16 scale (`[-32768, 32767]`), 48 kHz.
/// - `filter`: an already-initialised [`NoiseFilter`] for this voice session.
///
/// Processes complete 480-sample chunks in-place.  Any trailing partial
/// chunk (< 480 samples) passes through unmodified.
///
/// # Usage in the capture loop
///
/// ```ignore
/// if noise_cancel_enabled.load(Ordering::Relaxed) {
///     apply_rnnoise(&mut mono_f32_i16_scale, &mut filter);
/// }
/// ```
// Arithmetic is safe: chunk indices are bounded by `complete_chunks * DENOISE_FRAME <= len`.
// Integer division is intentional: we process only complete DENOISE_FRAME-sized chunks.
// Slicing is safe: `start..end` fits within `0..len` by construction.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::integer_division,
    clippy::indexing_slicing
)]
pub fn apply_rnnoise(samples: &mut [f32], filter: &mut NoiseFilter) {
    let len = samples.len();
    let complete_chunks = len / DENOISE_FRAME;

    for i in 0..complete_chunks {
        let start = i * DENOISE_FRAME;
        let end = start + DENOISE_FRAME;
        filter.process_chunk(&mut samples[start..end]);
    }
    // Trailing partial chunk is left as-is.
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn filter_preserves_length() {
        let mut filter = NoiseFilter::new();
        // 960 samples = two 480-sample chunks (one Opus frame at 48 kHz mono)
        let mut samples: Vec<f32> = (0..960)
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin() * 16384.0)
            .collect();
        let original_len = samples.len();
        apply_rnnoise(&mut samples, &mut filter);
        assert_eq!(
            samples.len(),
            original_len,
            "filter must not change Vec length"
        );
    }

    #[test]
    fn filter_first_frame_is_zeroed() {
        let mut filter = NoiseFilter::new();
        // Non-zero input: a 480-sample sine burst.
        let mut samples: Vec<f32> = (0..DENOISE_FRAME)
            .map(|i| (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin() * 16384.0)
            .collect();
        apply_rnnoise(&mut samples, &mut filter);
        // The first frame output should be replaced with zeros due to look-ahead.
        assert!(
            samples.iter().all(|&s| s == 0.0),
            "first output frame must be zeroed (look-ahead artifact suppression)"
        );
    }

    #[test]
    fn filter_partial_chunk_passthrough() {
        let mut filter = NoiseFilter::new();
        // 300 samples < 480 — should pass through unchanged.
        let input: Vec<f32> = (0..300).map(|i| i as f32).collect();
        let mut samples = input.clone();
        apply_rnnoise(&mut samples, &mut filter);
        assert_eq!(samples, input, "partial chunk must pass through unmodified");
    }

    #[test]
    fn filter_silence_stays_silent_after_warmup() {
        let mut filter = NoiseFilter::new();
        // Warm up with one frame of silence.
        let mut warmup: Vec<f32> = vec![0.0; DENOISE_FRAME];
        apply_rnnoise(&mut warmup, &mut filter);
        // Second frame of silence should come out near-silent.
        let mut silence: Vec<f32> = vec![0.0; DENOISE_FRAME];
        apply_rnnoise(&mut silence, &mut filter);
        let max_abs = silence.iter().map(|&s| s.abs()).fold(0.0_f32, f32::max);
        // Allow a small epsilon — the model may produce tiny non-zero output on silence.
        assert!(
            max_abs < 1.0,
            "silence through denoiser should stay near-silent, max_abs = {max_abs}"
        );
    }

    #[test]
    fn noise_filter_default_is_valid() {
        let filter = NoiseFilter::default();
        assert!(filter.first_frame, "default filter must start with first_frame=true");
    }
}
