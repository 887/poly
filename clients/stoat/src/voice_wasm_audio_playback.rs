//! WASM-only speaker playback for Stoat (Vortex) voice (Phase B.4).
//!
//! Exposes two public functions called by the decode path in `voice_wasm.rs`:
//!
//! - [`push_pcm`] — schedule a 20 ms i16 PCM mono frame for a remote user.
//! - [`drop_user`] — tear down the per-user [`AudioContext`] on participant-left.
//!
//! ## Design
//!
//! A `thread_local! { static PUMPS }` map (keyed by user_id `String`) holds
//! one [`UserPump`] per active remote participant. The WASM environment is
//! single-threaded, so `RefCell` is the correct interior-mutability primitive
//! here — `Mutex` would compile but is unnecessary overhead.
//!
//! Each [`UserPump`] owns exactly one `AudioContext`. Browsers impose a cap of
//! roughly 6 simultaneous `AudioContext` instances per origin; in a voice
//! channel with more than 6 remote participants the 7th context creation will
//! fail. The failure is logged and the frame silently dropped — a beep-less
//! absence is better than a panic. Consider using a single shared context with
//! per-user gain-node routing if the cap becomes a practical concern (open
//! issue for Phase E).
//!
//! Frames are scheduled gap-free via `AudioBufferSourceNode::start_with_when`.
//! A jitter prefix (`JITTER_PREFIX_S = 60 ms`) is added to the first frame so
//! late-arriving frames find an empty queue rather than fighting the scheduler.
//! If the cursor falls behind the current clock (network gap > 60 ms), the
//! cursor resets to `now + JITTER_PREFIX_S` — choosing a small glitch over
//! an ever-growing scheduling backlog.

// This entire file is wasm32-only; the module declaration in lib.rs is
// already gated with #[cfg(target_arch = "wasm32")].

use std::cell::RefCell;
use std::collections::HashMap;

// ── Constants ────────────────────────────────────────────────────────────────

/// 48 kHz mono, 20 ms per frame.
const FRAME_SAMPLES: usize = 960;
/// Sample rate passed to `AudioContext` and `AudioBuffer`.
const SAMPLE_RATE: f32 = 48_000.0;
/// Frame duration in seconds (20 ms).
const FRAME_DURATION_S: f64 = 0.020;
/// Initial scheduling headroom to absorb jitter before playback begins.
const JITTER_PREFIX_S: f64 = 0.060;

// ── Thread-local state ───────────────────────────────────────────────────────

thread_local! {
    /// Per-user playback state. Keyed by the stoat user_id string (derived
    /// from the 8-byte ASCII null-padded prefix on each Vortex binary frame).
    static PUMPS: RefCell<HashMap<String, UserPump>> = RefCell::new(HashMap::new());
}

// ── UserPump ─────────────────────────────────────────────────────────────────

/// Playback state for a single remote participant.
struct UserPump {
    /// One `AudioContext` per user. Browsers cap at ~6 active contexts — see
    /// module-level doc for the known limitation.
    ctx: web_sys::AudioContext,
    /// `AudioContext` clock time (in seconds) at which the next 20 ms frame
    /// should begin. Initialised to `ctx.currentTime() + JITTER_PREFIX_S` on
    /// the first frame for that user.
    next_start_time: f64,
}

impl UserPump {
    /// Create a new pump. Returns `Err` if `AudioContext::new()` fails (e.g.
    /// the browser context cap is reached).
    fn new() -> Result<Self, String> {
        let ctx = web_sys::AudioContext::new()
            .map_err(|e| format!("AudioContext::new failed: {e:?}"))?;
        let initial = ctx.current_time() + JITTER_PREFIX_S;
        Ok(Self { ctx, next_start_time: initial })
    }

    /// Convert i16 PCM → f32, create an `AudioBuffer`, and schedule it for
    /// gap-free playback.
    fn schedule_frame(&mut self, pcm: &[i16]) -> Result<(), String> {
        // i16 → f32 in [-1.0, 1.0].
        let float_samples: Vec<f32> =
            pcm.iter().map(|&s| f32::from(s) / 32_768.0).collect();

        // Create a mono AudioBuffer.
        let buffer = self
            .ctx
            .create_buffer(1, FRAME_SAMPLES as u32, SAMPLE_RATE)
            .map_err(|e| format!("create_buffer failed: {e:?}"))?;

        // copy_to_channel takes &mut [f32] — reborrow from owned Vec.
        let mut channel_data = float_samples;
        buffer
            .copy_to_channel(&mut channel_data, 0)
            .map_err(|e| format!("copy_to_channel failed: {e:?}"))?;

        // Create source node, attach buffer, connect to destination.
        let source = self
            .ctx
            .create_buffer_source()
            .map_err(|e| format!("create_buffer_source failed: {e:?}"))?;
        source.set_buffer(Some(&buffer));

        let dest = self.ctx.destination();
        let node: &web_sys::AudioNode = source.as_ref();
        node.connect_with_audio_node(dest.as_ref())
            .map_err(|e| format!("connect_with_audio_node failed: {e:?}"))?;

        // If we've fallen behind the clock (network gap), reset the cursor to
        // avoid scheduling frames in the past (which the scheduler silently
        // drops, causing an audible gap anyway).
        let now = self.ctx.current_time();
        if self.next_start_time < now {
            self.next_start_time = now + JITTER_PREFIX_S;
        }

        source
            .start_with_when(self.next_start_time)
            .map_err(|e| format!("start_with_when failed: {e:?}"))?;

        self.next_start_time += FRAME_DURATION_S;
        Ok(())
    }

    /// Best-effort close. The returned Promise is not awaited — the
    /// `AudioContext` will be GC'd once it leaves scope.
    fn close(&mut self) {
        let _ = self.ctx.close();
    }
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Push an i16 PCM mono frame (960 samples @ 48 kHz = 20 ms) for a remote
/// user. Lazily creates a per-user `AudioContext` + scheduling cursor on first
/// call for that `user_id`.
///
/// On any error (e.g. `AudioContext` creation failure when the browser cap is
/// hit, or `AudioBuffer` API failure) the frame is silently dropped and the
/// error is logged via `tracing::warn!`. These functions never panic and never
/// return errors — the voice path must be resilient to individual frame loss.
pub fn push_pcm(user_id: &str, pcm: Vec<i16>) {
    PUMPS.with(|pumps| {
        let mut map = pumps.borrow_mut();

        // Lazily initialise the pump for this user.
        if !map.contains_key(user_id) {
            match UserPump::new() {
                Ok(pump) => {
                    map.insert(user_id.to_owned(), pump);
                }
                Err(e) => {
                    tracing::warn!(
                        target: "poly_stoat::voice_wasm_audio_playback",
                        user_id,
                        error = %e,
                        "failed to create AudioContext for user — frame dropped"
                    );
                    return;
                }
            }
        }

        // SAFETY: inserted just above if missing.
        let pump = map.get_mut(user_id).expect("pump present — inserted above");
        if let Err(e) = pump.schedule_frame(&pcm) {
            tracing::warn!(
                target: "poly_stoat::voice_wasm_audio_playback",
                user_id,
                error = %e,
                "failed to schedule PCM frame — frame dropped"
            );
        }
    });
}

/// Drop the per-user playback state when a `VoiceParticipantLeft` event
/// arrives. Closes the `AudioContext` (best-effort) and removes the entry.
pub fn drop_user(user_id: &str) {
    PUMPS.with(|pumps| {
        if let Some(mut pump) = pumps.borrow_mut().remove(user_id) {
            pump.close();
        }
    });
}
