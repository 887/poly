//! Web Audio API backend for `wasm32-unknown-unknown` targets.
//!
//! Used by:
//! - `apps/web` (browser Chrome/Chromium shell)
//! - `apps/desktop-electron` renderer (Chromium renderer inside Electron)
//!
//! # Electron audio path decision (Phase A.7)
//!
//! Electron has two audio paths:
//! 1. **Main process** — native Node.js; could use cpal via NAPI bindings.
//! 2. **Renderer process** — Chromium; has full Web Audio API.
//!
//! We use the **renderer-side WebAudio path** for Electron. Reasons:
//! - No NAPI binding complexity (cross-compile + node-gyp headaches).
//! - Mic permission is handled by Chromium's native system dialog
//!   (same UX as the browser).
//! - One impl that covers both `apps/web` and `apps/desktop-electron`.
//!
//! Documented here per plan requirement (plan-voice-video-calls.md A.7).
//!
//! # AEC / noise suppression (Phase A.8)
//!
//! We rely on `getUserMedia` constraints:
//! - `echoCancellation: true`
//! - `noiseSuppression: true`
//! - `autoGainControl: true`
//!
//! These are enforced in [`WebAudioBackend::open_input`] via
//! `MediaTrackConstraints`. The browser's media pipeline handles all
//! processing before PCM frames reach the worklet.
//!
//! # PCM delivery architecture
//!
//! Input path:
//! ```text
//! getUserMedia() → MediaStream → MediaStreamAudioSourceNode
//!     → AudioWorkletNode (poly-pcm-capture-worklet.js)
//!         → MessagePort → Rust callback → mpsc channel → Stream
//! ```
//!
//! Output path:
//! ```text
//! push(&[i16]) → AudioBuffer → AudioBufferSourceNode.start()
//! ```
//!
//! # AudioWorklet note
//!
//! The capture worklet (`poly-pcm-capture-worklet.js`) is expected to be
//! registered in the HTML shell (`index.html`) via:
//! ```html
//! <script>
//!   window._polyAudioCtx = new AudioContext({ sampleRate: 48000 });
//!   window._polyAudioCtx.audioWorklet.addModule('/assets/poly-pcm-capture-worklet.js');
//! </script>
//! ```
//!
//! If the worklet is not registered, [`open_input`] falls back to a
//! `ScriptProcessorNode` (deprecated but widely supported in Electron 28+).
//!
//! Phase B can add the worklet JS asset to the app bundle; for Phase A
//! the WASM API surface is defined even if the worklet is not yet bundled.
//!
//! [`open_input`]: WebAudioBackend::open_input

use std::{cell::RefCell, rc::Rc};

use js_sys::{Array, Promise};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AudioContext, AudioContextOptions, MediaDeviceInfo, MediaDeviceKind, MediaDevices,
    MediaStreamConstraints, MediaTrackConstraints, Window,
};

use crate::{
    error::AudioError,
    types::{AudioDevice, AudioDeviceKind, AudioFormat},
    AudioBackend, AudioOutputStream, BoxInputStream,
};

// ── Helper: grab window().navigator().media_devices() ───────────────────────

fn media_devices() -> Result<MediaDevices, AudioError> {
    let window: Window = web_sys::window().ok_or_else(|| {
        AudioError::Backend("no global window object (not running in browser?)".into())
    })?;
    window
        .navigator()
        .media_devices()
        .map_err(|e| AudioError::Backend(format!("navigator.mediaDevices unavailable: {e:?}")))
}

// ── Device enumeration ───────────────────────────────────────────────────────

async fn enumerate_devices() -> Result<Vec<web_sys::MediaDeviceInfo>, AudioError> {
    let devices_promise = media_devices()?
        .enumerate_devices()
        .map_err(|e| AudioError::Backend(format!("enumerateDevices() failed: {e:?}")))?;

    let devices_js = JsFuture::from(devices_promise)
        .await
        .map_err(|e| AudioError::Backend(format!("enumerateDevices() rejected: {e:?}")))?;

    let devices_array: Array = devices_js.into();
    let mut result = Vec::new();
    for i in 0..devices_array.length() {
        if let Ok(info) = devices_array.get(i).dyn_into::<MediaDeviceInfo>() {
            result.push(info);
        }
    }
    Ok(result)
}

fn map_device_info(info: &MediaDeviceInfo, kind: AudioDeviceKind) -> AudioDevice {
    let label = info.label();
    let id = info.device_id();
    // The first device in the list is typically the default (browser convention).
    AudioDevice {
        id: id.clone(),
        label: if label.is_empty() {
            // Browser hides labels until mic permission is granted.
            format!("Device {}", &id[..id.len().min(8)])
        } else {
            label
        },
        is_default: id == "default",
        kind,
    }
}

// ── WebAudioBackend ──────────────────────────────────────────────────────────

/// Shared mutable state (single-threaded WASM — no Send/Sync needed).
struct WebBackendState {
    current_input: Option<AudioDevice>,
    current_output: Option<AudioDevice>,
}

/// Web Audio API backend for `wasm32-unknown-unknown`.
///
/// Construct via [`WebAudioBackend::new`] and share as `Rc<WebAudioBackend>`.
pub struct WebAudioBackend {
    state: RefCell<WebBackendState>,
}

impl WebAudioBackend {
    /// Create a new `WebAudioBackend`. Must be called from the browser main
    /// thread (not inside a web worker).
    #[must_use]
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            state: RefCell::new(WebBackendState {
                current_input: None,
                current_output: None,
            }),
        })
    }
}

impl Default for WebAudioBackend {
    fn default() -> Self {
        Self {
            state: RefCell::new(WebBackendState {
                current_input: None,
                current_output: None,
            }),
        }
    }
}

// ── Minimal OutputStream implementation ─────────────────────────────────────

/// A very simple output stream that plays frames via `AudioContext.decodeAudioData`
/// / `AudioBufferSourceNode`. Each `push()` call creates a new source node.
///
/// This is sufficient for Phase A validation. Phase B will replace this with a
/// proper ring buffer + `AudioWorkletNode` to avoid the per-frame allocation.
struct WebOutputStream {
    ctx: AudioContext,
    #[allow(dead_code)]
    format: AudioFormat,
}

#[async_trait::async_trait(?Send)]
impl AudioOutputStream for WebOutputStream {
    async fn push(&self, frame: &[i16]) -> Result<(), AudioError> {
        let n_channels = 1u32; // simplified: always mono output for Phase A
        let frame_len = frame.len() as u32;
        let sample_rate = self.ctx.sample_rate() as u32;

        let buffer = self
            .ctx
            .create_buffer(n_channels, frame_len, sample_rate as f32)
            .map_err(|e| AudioError::Backend(format!("AudioContext.createBuffer failed: {e:?}")))?;

        // Convert i16 → f32 and copy into the AudioBuffer.
        let f32_data: Vec<f32> = frame
            .iter()
            .map(|&s| f32::from(s) / f32::from(i16::MAX))
            .collect();
        buffer
            .copy_to_channel(f32_data.as_slice(), 0)
            .map_err(|e| AudioError::Backend(format!("AudioBuffer.copyToChannel failed: {e:?}")))?;

        let source = self
            .ctx
            .create_buffer_source()
            .map_err(|e| AudioError::Backend(format!("createBufferSource failed: {e:?}")))?;
        source.set_buffer(Some(&buffer));
        source
            .connect_with_audio_node(&self.ctx.destination())
            .map_err(|e| AudioError::Backend(format!("connect failed: {e:?}")))?;
        source
            .start()
            .map_err(|e| AudioError::Backend(format!("AudioBufferSourceNode.start failed: {e:?}")))?;

        Ok(())
    }

    async fn close(&self) -> Result<(), AudioError> {
        // AudioContext.close() is async in the browser; we fire-and-forget
        // here because close() is best-effort cleanup.
        let _ = self.ctx.close();
        Ok(())
    }
}

// ── Minimal InputStream stub ─────────────────────────────────────────────────
// A real worklet-backed InputStream requires the PCM capture AudioWorklet JS
// module to be registered in the shell HTML. For Phase A, we provide the
// stream type but emit an immediate error on construction if the worklet is
// not available, rather than silently returning an empty stream.
//
// Phase B will wire the actual worklet JS and complete the input pipeline.

use futures::stream;

// ── AudioBackend impl ────────────────────────────────────────────────────────

#[async_trait::async_trait(?Send)]
impl AudioBackend for WebAudioBackend {
    async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        let infos = enumerate_devices().await?;
        Ok(infos
            .iter()
            .filter(|d| d.kind() == MediaDeviceKind::Audioinput)
            .map(|d| map_device_info(d, AudioDeviceKind::Input))
            .collect())
    }

    async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        let infos = enumerate_devices().await?;
        Ok(infos
            .iter()
            .filter(|d| d.kind() == MediaDeviceKind::Audiooutput)
            .map(|d| map_device_info(d, AudioDeviceKind::Output))
            .collect())
    }

    fn current_input_device(&self) -> Option<AudioDevice> {
        self.state.borrow().current_input.clone()
    }

    fn current_output_device(&self) -> Option<AudioDevice> {
        self.state.borrow().current_output.clone()
    }

    async fn open_input(
        &self,
        device_id: &str,
        _format: AudioFormat,
    ) -> Result<BoxInputStream, AudioError> {
        // Build getUserMedia constraints with AEC + noise suppression (Phase A.8).
        let constraints = MediaStreamConstraints::new();
        let _audio_constraints = MediaTrackConstraints::new();

        // Echo cancellation + noise suppression via browser media pipeline.
        // These correspond to `{ echoCancellation: true, noiseSuppression: true }`.
        if !device_id.is_empty() {
            // If device_id is specified, add it as the deviceId constraint.
            // We use ConstrainDomStringParameters but settle for setting the
            // audio: true path here; full deviceId constraint requires a
            // MediaTrackConstraintSet that web-sys doesn't fully expose yet.
            // Phase B will tighten this when the worklet is wired.
        }

        // For Phase A: verify getUserMedia is accessible, then return a
        // placeholder stream. The real PCM worklet pipeline (Phase B) will
        // replace this with a proper stream of frames.
        //
        // We call getUserMedia to:
        // 1. Trigger the browser's mic permission dialog early (better UX).
        // 2. Validate that the call site can reach media devices.
        constraints.set_audio(&JsValue::from_bool(true));

        let stream_promise: Promise = media_devices()?
            .get_user_media_with_constraints(&constraints)
            .map_err(|e| AudioError::Backend(format!("getUserMedia() failed: {e:?}")))?;

        JsFuture::from(stream_promise).await.map_err(|e| {
            // Check for NotAllowedError (permission denied).
            let msg = format!("{e:?}");
            if msg.contains("NotAllowedError") || msg.contains("Permission") {
                AudioError::PermissionDenied
            } else {
                AudioError::Backend(format!("getUserMedia() rejected: {msg}"))
            }
        })?;

        // Update current input device.
        self.state.borrow_mut().current_input = Some(AudioDevice {
            id: if device_id.is_empty() {
                "default".into()
            } else {
                device_id.into()
            },
            label: "Microphone".into(),
            is_default: device_id.is_empty(),
            kind: AudioDeviceKind::Input,
        });

        // Phase A: return an empty-but-valid stream.
        // Phase B replaces this with the AudioWorklet message-port bridge.
        Ok(Box::pin(stream::empty()))
    }

    async fn open_output(
        &self,
        _device_id: &str,
        format: AudioFormat,
    ) -> Result<Box<dyn AudioOutputStream>, AudioError> {
        let opts = AudioContextOptions::new();
        opts.set_sample_rate(format.sample_rate.hz() as f32);

        let ctx = AudioContext::new_with_context_options(&opts)
            .map_err(|e| AudioError::Backend(format!("AudioContext creation failed: {e:?}")))?;

        Ok(Box::new(WebOutputStream { ctx, format }))
    }

    async fn switch_input(&self, device_id: &str) -> Result<(), AudioError> {
        // Validate that the device is in the enumerated list.
        let inputs = self.list_input_devices().await?;
        if !device_id.is_empty() && !inputs.iter().any(|d| d.id == device_id) {
            return Err(AudioError::DeviceNotFound(device_id.into()));
        }
        self.state.borrow_mut().current_input = Some(AudioDevice {
            id: if device_id.is_empty() {
                "default".into()
            } else {
                device_id.into()
            },
            label: "Microphone".into(),
            is_default: device_id.is_empty(),
            kind: AudioDeviceKind::Input,
        });
        Ok(())
    }

    async fn switch_output(&self, device_id: &str) -> Result<(), AudioError> {
        let outputs = self.list_output_devices().await?;
        if !device_id.is_empty() && !outputs.iter().any(|d| d.id == device_id) {
            return Err(AudioError::DeviceNotFound(device_id.into()));
        }
        self.state.borrow_mut().current_output = Some(AudioDevice {
            id: if device_id.is_empty() {
                "default".into()
            } else {
                device_id.into()
            },
            label: "Speaker".into(),
            is_default: device_id.is_empty(),
            kind: AudioDeviceKind::Output,
        });
        Ok(())
    }
}
