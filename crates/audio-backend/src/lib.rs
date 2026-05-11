//! `poly-audio-backend` — protocol-agnostic audio I/O abstraction.
//!
//! # Design
//!
//! One [`AudioBackend`] trait drives all audio operations across shells:
//!
//! | Shell | Feature | Impl |
//! |---|---|---|
//! | Wry (apps/desktop) | `native` | [`cpal_backend::CpalBackend`] |
//! | apps/web (browser) | `web` | [`web_backend::WebAudioBackend`] |
//! | Electron renderer | `web` | [`web_backend::WebAudioBackend`] |
//!
//! Electron's Chromium renderer has the same Web Audio API as the browser
//! shell. The native main process could use cpal via NAPI but that adds
//! build complexity; the renderer-side WebAudio path is simpler and already
//! handles mic permissions via Chromium's native dialog.
//! (Decision recorded in `docs/plans/plan-voice-video-calls.md` Phase A.7.)
//!
//! # PCM format
//!
//! All streams use signed 16-bit PCM (`i16`) at the sample rate and channel
//! count specified in [`AudioFormat`]. The default (and Discord voice
//! requirement) is 48 kHz stereo; Stoat voice uses 48 kHz mono as a safe
//! starting point. The backend implementation is responsible for any
//! hardware-to-format resampling.
//!
//! # AEC / noise suppression (Phase A.8)
//!
//! On **web/Electron**: rely on `getUserMedia` constraints
//! (`echoCancellation: true, noiseSuppression: true`) — the browser
//! handles it for free.
//!
//! On **native cpal**: there is no built-in AEC. Phase J (device picker
//! follow-up) will integrate `nnnoiseless` for noise suppression; full
//! AEC is deferred.
//!
//! # Device persistence (Phase A.6)
//!
//! Last-used input/output device IDs are stored in `poly_kv` under:
//! - `voice.last_input_device.<account_id>`
//! - `voice.last_output_device.<account_id>`
//!
//! The storage/restore logic is the responsibility of the call site
//! (voice channel connect helpers in the discord/stoat clients); this
//! crate only provides the types and the [`AudioDevice`] ID required.
//!
//! # Open question: cpal blocking callback model
//!
//! cpal delivers PCM input frames via a non-async callback invoked on a
//! real-time audio thread. The [`AudioInputStream`] trait yields frames as
//! a `futures::Stream<Item = Vec<i16>>`. The `CpalBackend` bridges these
//! two models using a `tokio::sync::mpsc` channel: the cpal callback sends
//! raw frames into the channel, and the stream polls the channel receiver.
//! This introduces one allocation per frame; Phase B can optimise to a
//! lock-free SPSC ring buffer if needed.
//!
//! # Open question: hotplug events
//!
//! cpal (≤0.16) does not expose device-change notifications. The
//! `CpalBackend` exposes re-enumeration via [`AudioBackend::list_input_devices`]
//! / [`AudioBackend::list_output_devices`]; callers should poll every ~2s
//! for native v1. Web has `navigator.mediaDevices.ondevicechange` — the
//! `WebAudioBackend` subscribes to that event and invalidates its cached
//! device list automatically (Phase J.5 integration).

pub mod error;
pub mod kv_keys;
pub mod types;

#[cfg(feature = "native")]
pub mod cpal_backend;

#[cfg(all(target_arch = "wasm32", feature = "web"))]
pub mod web_backend;

/// Test support: `FakeAudioBackend` compiles on all targets (native + WASM)
/// and lets unit tests exercise the trait surface without real hardware.
pub mod fake_backend;

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::fake_backend::FakeAudioBackend;
    use futures::StreamExt;

    // Helper: run an async test body in a single-threaded tokio runtime
    // (compiles on both native and wasm32 via the `tokio::test` macro).

    #[tokio::test]
    async fn fake_backend_list_devices() {
        let backend = FakeAudioBackend::new();

        let inputs = backend.list_input_devices().await.unwrap();
        let outputs = backend.list_output_devices().await.unwrap();

        assert_eq!(inputs.len(), 1, "expected 1 fake input device");
        assert_eq!(inputs[0].id, "fake-mic");
        assert_eq!(inputs[0].kind, AudioDeviceKind::Input);
        assert!(inputs[0].is_default);

        assert_eq!(outputs.len(), 1, "expected 1 fake output device");
        assert_eq!(outputs[0].id, "fake-speaker");
        assert_eq!(outputs[0].kind, AudioDeviceKind::Output);
        assert!(outputs[0].is_default);
    }

    #[tokio::test]
    async fn fake_backend_open_input_returns_stream() {
        let backend = FakeAudioBackend::new();

        // Open the default input device (empty string → default).
        let mut stream = backend
            .open_input("", AudioFormat::default())
            .await
            .unwrap();

        // The fake stream is empty (silence); next() should return None.
        assert!(stream.next().await.is_none());

        let snap = backend.state_snapshot();
        assert_eq!(snap.open_input_calls, 1);
        assert_eq!(snap.current_input.unwrap().id, "fake-mic");
    }

    #[tokio::test]
    async fn fake_backend_open_output_and_push() {
        let backend = FakeAudioBackend::new();

        let output = backend
            .open_output("", AudioFormat::default())
            .await
            .unwrap();

        // Push a 1920-sample (20 ms 48 kHz stereo) silence frame.
        let frame: Vec<i16> = vec![0i16; 1920];
        output.push(&frame).await.unwrap();
        output.close().await.unwrap();

        let snap = backend.state_snapshot();
        assert_eq!(snap.open_output_calls, 1);
        assert_eq!(snap.output_samples_pushed, 1920);
    }

    #[tokio::test]
    async fn fake_backend_current_devices_start_none() {
        let backend = FakeAudioBackend::new();
        // Before any open_* call, current device is None.
        assert!(backend.current_input_device().is_none());
        assert!(backend.current_output_device().is_none());
    }

    #[tokio::test]
    async fn fake_backend_switch_input_updates_current() {
        let backend = FakeAudioBackend::new();

        // Open once to establish current.
        let _stream = backend.open_input("fake-mic", AudioFormat::default()).await.unwrap();
        assert_eq!(backend.current_input_device().unwrap().id, "fake-mic");

        // Switch to the same device (no-op in terms of stream but updates state).
        backend.switch_input("fake-mic").await.unwrap();
        assert_eq!(backend.current_input_device().unwrap().id, "fake-mic");
    }

    #[tokio::test]
    async fn fake_backend_switch_to_unknown_device_errors() {
        let backend = FakeAudioBackend::new();

        let result = backend.switch_input("nonexistent-device-42").await;
        assert!(
            matches!(result, Err(AudioError::DeviceNotFound(_))),
            "expected DeviceNotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn audio_format_frame_samples() {
        // Discord: 48 kHz stereo, 20 ms → 1920 i16 samples.
        let discord_20ms = AudioFormat::DISCORD_VOICE.frame_samples(20);
        assert_eq!(discord_20ms, 1920, "Discord 20ms stereo should be 1920 samples");

        // Stoat: 48 kHz mono, 20 ms → 960 i16 samples.
        let stoat_20ms = AudioFormat::STOAT_VOICE.frame_samples(20);
        assert_eq!(stoat_20ms, 960, "Stoat 20ms mono should be 960 samples");
    }

    #[tokio::test]
    async fn audio_device_constructors() {
        let dev = AudioDevice::new("dev-id", "My Device", AudioDeviceKind::Input);
        assert!(!dev.is_default);
        assert_eq!(dev.id, "dev-id");

        let default_dev =
            AudioDevice::new_default("default-id", "Default Device", AudioDeviceKind::Output);
        assert!(default_dev.is_default);
        assert_eq!(default_dev.kind, AudioDeviceKind::Output);
    }

    #[tokio::test]
    async fn fake_backend_open_input_unknown_device_errors() {
        let backend = FakeAudioBackend::new();
        let result = backend.open_input("no-such-mic", AudioFormat::default()).await;
        assert!(
            matches!(result, Err(AudioError::DeviceNotFound(_))),
            "expected DeviceNotFound"
        );
    }
}

pub use error::AudioError;
pub use types::{AudioDevice, AudioDeviceKind, AudioFormat, SampleRate};

use futures::Stream;
use std::pin::Pin;

/// A stream of PCM frames from a microphone / input device.
///
/// Each `Vec<i16>` is one frame of interleaved samples:
/// - Mono (1 channel): `samples[n]` is sample n.
/// - Stereo (2 channels): `samples[2n]` is left, `samples[2n+1]` is right.
///
/// Frame duration is determined by the backend implementation. For Discord
/// voice, 20 ms frames at 48 kHz stereo = 1920 i16 samples per vec.
/// Downstream Opus encoders (Phase B) slice these into 20 ms packets.
pub type AudioInputFrame = Vec<i16>;

/// A pinned, heap-allocated stream of PCM input frames.
pub type BoxInputStream = Pin<Box<dyn Stream<Item = AudioInputFrame> + Send>>;

/// An audio output sink. Call [`AudioOutputStream::push`] to render PCM.
///
/// The implementation handles buffering, resampling, and device I/O
/// internally. `push` is non-blocking from the caller's perspective; frames
/// are queued to the audio thread's ring buffer.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AudioOutputStream: Send + Sync {
    /// Push a slice of interleaved PCM samples for playback.
    ///
    /// `frame` layout matches the format used to open the stream
    /// (see [`AudioFormat`]).
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::DeviceLost`] if the output device has been
    /// removed since the stream was opened.
    async fn push(&self, frame: &[i16]) -> Result<(), AudioError>;

    /// Signal that no more frames will be pushed and the stream should
    /// drain / flush any buffered audio before closing.
    async fn close(&self) -> Result<(), AudioError>;
}

/// The primary audio abstraction.
///
/// Implementations are expected to be cheaply cloneable (e.g. `Arc`-wrapped)
/// so they can be shared between the encode and decode loops in Phase B.
///
/// On WASM the trait is `?Send`; on native it is `Send + Sync`.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait AudioBackend {
    // ── Device enumeration ───────────────────────────────────────────────

    /// List all available microphone / line-in devices.
    ///
    /// IDs MUST be stable across enumerations (used as KV keys for
    /// "remember last device" — see Phase A.6 / Phase J.4).
    async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError>;

    /// List all available speaker / headphone output devices.
    async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError>;

    // ── Current device accessors ─────────────────────────────────────────

    /// Return the currently-selected input device, if any.
    fn current_input_device(&self) -> Option<AudioDevice>;

    /// Return the currently-selected output device, if any.
    fn current_output_device(&self) -> Option<AudioDevice>;

    // ── Stream lifecycle ─────────────────────────────────────────────────

    /// Open a PCM capture stream from `device_id`.
    ///
    /// Returns a [`BoxInputStream`] that yields interleaved `i16` frames
    /// in the given `format`. The stream runs until dropped.
    ///
    /// Passing `""` (empty string) selects the system default input device.
    async fn open_input(
        &self,
        device_id: &str,
        format: AudioFormat,
    ) -> Result<BoxInputStream, AudioError>;

    /// Open a PCM playback stream to `device_id`.
    ///
    /// Passing `""` selects the system default output device.
    async fn open_output(
        &self,
        device_id: &str,
        format: AudioFormat,
    ) -> Result<Box<dyn AudioOutputStream>, AudioError>;

    // ── Mid-call device switching ────────────────────────────────────────

    /// Switch the active input device without dropping the encode pipeline.
    ///
    /// Implementations MUST seamlessly hand off the PCM stream to the new
    /// device. If the underlying platform cannot do this atomically, a brief
    /// silence is acceptable, but the `BoxInputStream` returned by the
    /// previous [`open_input`] call MUST continue yielding frames without
    /// the caller needing to reopen.
    ///
    /// Phase J.3 exercises this from the device-picker UI.
    ///
    /// [`open_input`]: AudioBackend::open_input
    async fn switch_input(&self, device_id: &str) -> Result<(), AudioError>;

    /// Switch the active output device without dropping the decode pipeline.
    async fn switch_output(&self, device_id: &str) -> Result<(), AudioError>;
}
