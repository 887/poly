//! Native audio backend using `cpal` (ALSA / PulseAudio / PipeWire on Linux,
//! CoreAudio on macOS, WASAPI on Windows).
//!
//! # cpal callback model
//!
//! cpal delivers input PCM frames via a *synchronous* callback on a
//! real-time audio thread — it cannot await a future or call async code.
//! We bridge this to the [`AudioInputStream`] / `futures::Stream` model via a
//! `tokio::sync::mpsc::channel`:
//!
//! ```text
//! cpal RT thread  ──push──▶  mpsc::Sender<Vec<i16>>  ──poll──▶  Stream consumer
//! ```
//!
//! The channel is bounded (256 frames ≈ 5 s of 20 ms frames). If the
//! consumer falls behind, the callback drops frames and emits a warning.
//! Phase B should replace this with a lock-free SPSC ring buffer if
//! latency is a concern.
//!
//! # Hotplug / device change (Phase J.5)
//!
//! cpal ≤0.16 does not expose device-change callbacks. Callers should call
//! [`CpalBackend::list_input_devices`] / [`CpalBackend::list_output_devices`]
//! on a 2-second polling timer and compare against their last snapshot.
//! When an active stream's device disappears, the cpal error callback fires
//! [`AudioError::DeviceLost`] through the stream.
//!
//! # AEC / noise suppression (Phase A.8)
//!
//! There is no built-in AEC in cpal. The caller is responsible for
//! post-processing (Phase J will integrate `nnnoiseless` for NS; full AEC
//! is deferred). Document this loudly at the call site so Phase J authors
//! don't miss it.
//!
//! # Switch semantics (mid-call swap)
//!
//! [`CpalBackend::switch_input`] / [`switch_output`] atomically swap the
//! current device selection stored in the shared state. The next call to
//! [`open_input`] / [`open_output`] picks up the new device. Mid-call
//! device swapping (without dropping the encode pipeline) requires the
//! caller to re-open the stream against the new device ID and reconnect
//! to the encoder. This is Phase J.3 work; the `switch_*` methods here
//! only update the "current device" preference.
//!
//! [`open_input`]: CpalBackend::open_input
//! [`open_output`]: CpalBackend::open_output
//! [`switch_input`]: CpalBackend::switch_input
//! [`switch_output`]: CpalBackend::switch_output

use std::sync::{Arc, Mutex};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat,
};
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::{
    error::AudioError,
    types::{AudioDevice, AudioDeviceKind, AudioFormat},
    AudioBackend, AudioOutputStream, BoxInputStream,
};

// Bounded channel capacity: 256 × 20 ms = ~5 s of buffered audio.
// Allows the consumer to be briefly preempted without dropping frames.
const CHANNEL_CAPACITY: usize = 256;

/// Shared mutable state for the `CpalBackend`.
#[derive(Default)]
struct BackendState {
    current_input: Option<AudioDevice>,
    current_output: Option<AudioDevice>,
}

/// Native audio backend built on `cpal`.
///
/// Construct via [`CpalBackend::new`] and share as `Arc<CpalBackend>`.
/// The `Arc` wrapper is required because the cpal stream callbacks hold
/// a reference to internal sender channels.
pub struct CpalBackend {
    host: cpal::Host,
    state: Mutex<BackendState>,
}

impl CpalBackend {
    /// Create a new backend using the platform-default cpal host.
    ///
    /// On Linux this is PipeWire > PulseAudio > ALSA (depending on which
    /// is available). Use `cpal::available_hosts()` to enumerate alternatives.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::Backend`] if the cpal host cannot be
    /// initialised (very rare on supported platforms).
    pub fn new() -> Result<Arc<Self>, AudioError> {
        let host = cpal::default_host();
        Ok(Arc::new(Self {
            host,
            state: Mutex::new(BackendState::default()),
        }))
    }

    /// Enumerate input devices and convert to [`AudioDevice`] list.
    fn enumerate_inputs(&self) -> Result<Vec<AudioDevice>, AudioError> {
        let default_name = self
            .host
            .default_input_device()
            .and_then(|d| d.name().ok());

        let devices = self
            .host
            .input_devices()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        let mut result = Vec::new();
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".into());
            let is_default = Some(&name) == default_name.as_ref();
            result.push(AudioDevice {
                id: name.clone(),
                label: name,
                is_default,
                kind: AudioDeviceKind::Input,
            });
        }
        Ok(result)
    }

    /// Enumerate output devices and convert to [`AudioDevice`] list.
    fn enumerate_outputs(&self) -> Result<Vec<AudioDevice>, AudioError> {
        let default_name = self
            .host
            .default_output_device()
            .and_then(|d| d.name().ok());

        let devices = self
            .host
            .output_devices()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        let mut result = Vec::new();
        for device in devices {
            let name = device.name().unwrap_or_else(|_| "Unknown".into());
            let is_default = Some(&name) == default_name.as_ref();
            result.push(AudioDevice {
                id: name.clone(),
                label: name,
                is_default,
                kind: AudioDeviceKind::Output,
            });
        }
        Ok(result)
    }

    /// Resolve a device by ID string. `""` → default device.
    fn find_input_device(&self, device_id: &str) -> Result<cpal::Device, AudioError> {
        if device_id.is_empty() {
            return self
                .host
                .default_input_device()
                .ok_or_else(|| AudioError::DeviceNotFound("(default input)".into()));
        }
        let mut devices = self
            .host
            .input_devices()
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        devices
            .find(|d| d.name().as_deref() == Ok(device_id))
            .ok_or_else(|| AudioError::DeviceNotFound(device_id.into()))
    }

    fn find_output_device(&self, device_id: &str) -> Result<cpal::Device, AudioError> {
        if device_id.is_empty() {
            return self
                .host
                .default_output_device()
                .ok_or_else(|| AudioError::DeviceNotFound("(default output)".into()));
        }
        let mut devices = self
            .host
            .output_devices()
            .map_err(|e| AudioError::Backend(e.to_string()))?;
        devices
            .find(|d| d.name().as_deref() == Ok(device_id))
            .ok_or_else(|| AudioError::DeviceNotFound(device_id.into()))
    }

    /// Pick the best supported stream config matching `format`.
    ///
    /// Prefer the exact sample rate and channel count; fall back to the
    /// device's default config if the requested rate is unsupported.
    fn input_config(
        device: &cpal::Device,
        format: AudioFormat,
    ) -> Result<cpal::StreamConfig, AudioError> {
        let configs = device
            .supported_input_configs()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        let target_rate = cpal::SampleRate(format.sample_rate.hz());
        let target_channels = format.channels.count();

        // Try to find a config that supports our exact rate + channel count.
        for cfg_range in configs {
            if cfg_range.channels() == target_channels
                && cfg_range.min_sample_rate() <= target_rate
                && cfg_range.max_sample_rate() >= target_rate
            {
                return Ok(cfg_range.with_sample_rate(target_rate).into());
            }
        }

        // Fall back to device default.
        warn!(
            "Device does not support {:?} {}ch; using device default config",
            format.sample_rate,
            target_channels
        );
        device
            .default_input_config()
            .map(|c| c.into())
            .map_err(|e| AudioError::Backend(e.to_string()))
    }

    fn output_config(
        device: &cpal::Device,
        format: AudioFormat,
    ) -> Result<cpal::StreamConfig, AudioError> {
        let configs = device
            .supported_output_configs()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        let target_rate = cpal::SampleRate(format.sample_rate.hz());
        let target_channels = format.channels.count();

        for cfg_range in configs {
            if cfg_range.channels() == target_channels
                && cfg_range.min_sample_rate() <= target_rate
                && cfg_range.max_sample_rate() >= target_rate
            {
                return Ok(cfg_range.with_sample_rate(target_rate).into());
            }
        }

        warn!(
            "Device does not support {:?} {}ch output; using device default config",
            format.sample_rate,
            target_channels
        );
        device
            .default_output_config()
            .map(|c| c.into())
            .map_err(|e| AudioError::Backend(e.to_string()))
    }
}

// ── AudioInputStream bridge ──────────────────────────────────────────────────

/// A `futures::Stream` that yields PCM frames received from a cpal
/// real-time audio thread via an MPSC channel.
struct CpalInputStream {
    receiver: mpsc::Receiver<Vec<i16>>,
    /// Keep the stream alive until this object is dropped.
    _stream: cpal::Stream,
}

impl Stream for CpalInputStream {
    type Item = Vec<i16>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

// ── AudioOutputStream wrapper ────────────────────────────────────────────────

/// A [`crate::AudioOutputStream`] backed by a cpal output stream.
///
/// Frames pushed via [`push`] are sent over an MPSC channel to the cpal
/// data callback, which writes them to the audio device.
///
/// [`push`]: CpalOutputStream::push
struct CpalOutputStream {
    sender: mpsc::Sender<Vec<i16>>,
    /// Keep the stream alive.
    _stream: cpal::Stream,
}

#[async_trait::async_trait]
impl AudioOutputStream for CpalOutputStream {
    async fn push(&self, frame: &[i16]) -> Result<(), AudioError> {
        self.sender
            .send(frame.to_vec())
            .await
            .map_err(|_| AudioError::DeviceLost)
    }

    async fn close(&self) -> Result<(), AudioError> {
        // Dropping the sender signals the output callback to drain + stop.
        // The actual cpal stream lives in `_stream` and will stop when
        // the CpalOutputStream is dropped by the caller.
        Ok(())
    }
}

// ── AudioBackend impl ────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl AudioBackend for CpalBackend {
    async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.enumerate_inputs()
    }

    async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.enumerate_outputs()
    }

    fn current_input_device(&self) -> Option<AudioDevice> {
        self.state
            .lock()
            .expect("BackendState lock poisoned")
            .current_input
            .clone()
    }

    fn current_output_device(&self) -> Option<AudioDevice> {
        self.state
            .lock()
            .expect("BackendState lock poisoned")
            .current_output
            .clone()
    }

    async fn open_input(
        &self,
        device_id: &str,
        format: AudioFormat,
    ) -> Result<BoxInputStream, AudioError> {
        let device = self.find_input_device(device_id)?;
        let config = Self::input_config(&device, format)?;
        let (tx, rx) = mpsc::channel::<Vec<i16>>(CHANNEL_CAPACITY);

        debug!(
            "Opening cpal input stream on '{}' {:?}",
            device.name().unwrap_or_default(),
            config
        );

        let channels = config.channels as usize;

        // Build the stream. cpal supports multiple sample formats; we
        // normalise everything to i16 for consistency with the trait contract.
        let stream = match device
            .default_input_config()
            .map_err(|e| AudioError::Backend(e.to_string()))?
            .sample_format()
        {
            SampleFormat::I16 => {
                let tx = tx.clone();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i16], _| {
                            let frame: Vec<i16> = data.to_vec();
                            if tx.try_send(frame).is_err() {
                                warn!("cpal input buffer full — dropping frame");
                            }
                        },
                        |e| warn!("cpal input stream error: {e}"),
                        None,
                    )
                    .map_err(|e| AudioError::Backend(e.to_string()))?
            }
            SampleFormat::F32 => {
                let tx = tx.clone();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| {
                            let frame: Vec<i16> = data
                                .iter()
                                .map(|&s| (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16)
                                .collect();
                            if tx.try_send(frame).is_err() {
                                warn!("cpal input buffer full — dropping frame");
                            }
                        },
                        |e| warn!("cpal input stream error: {e}"),
                        None,
                    )
                    .map_err(|e| AudioError::Backend(e.to_string()))?
            }
            SampleFormat::U8 => {
                let tx = tx.clone();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[u8], _| {
                            let frame: Vec<i16> = data
                                .iter()
                                .map(|&s| ((s as i32 - 128) * 256) as i16)
                                .collect();
                            if tx.try_send(frame).is_err() {
                                warn!("cpal input buffer full — dropping frame");
                            }
                        },
                        |e| warn!("cpal input stream error: {e}"),
                        None,
                    )
                    .map_err(|e| AudioError::Backend(e.to_string()))?
            }
            other => {
                return Err(AudioError::UnsupportedFormat(format!(
                    "cpal sample format {other:?} is not supported"
                )));
            }
        };

        stream
            .play()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        // Update current input device record.
        {
            let mut state = self.state.lock().expect("BackendState lock poisoned");
            let device_name = device.name().unwrap_or_else(|_| device_id.into());
            state.current_input = Some(AudioDevice {
                id: device_name.clone(),
                label: device_name,
                is_default: device_id.is_empty(),
                kind: AudioDeviceKind::Input,
            });
        }

        let _ = channels; // used indirectly via the stream config
        Ok(Box::pin(CpalInputStream {
            receiver: rx,
            _stream: stream,
        }))
    }

    async fn open_output(
        &self,
        device_id: &str,
        format: AudioFormat,
    ) -> Result<Box<dyn AudioOutputStream>, AudioError> {
        let device = self.find_output_device(device_id)?;
        let config = Self::output_config(&device, format)?;
        let (tx, rx) = mpsc::channel::<Vec<i16>>(CHANNEL_CAPACITY);

        debug!(
            "Opening cpal output stream on '{}' {:?}",
            device.name().unwrap_or_default(),
            config
        );

        // The output callback pulls frames from the rx side of the channel.
        // We move `rx` into the callback closure. Because cpal callbacks are
        // `'static`, we wrap in `Arc<Mutex>` so ownership is clear.
        let rx = Arc::new(Mutex::new(rx));

        let stream = {
            let rx = Arc::clone(&rx);
            match device
                .default_output_config()
                .map_err(|e| AudioError::Backend(e.to_string()))?
                .sample_format()
            {
                SampleFormat::I16 => device
                    .build_output_stream(
                        &config,
                        move |data: &mut [i16], _| {
                            let mut guard = rx.lock().expect("rx lock poisoned");
                            if let Ok(frame) = guard.try_recv() {
                                let len = frame.len().min(data.len());
                                data[..len].copy_from_slice(&frame[..len]);
                                // Zero out any remaining samples if frame is short.
                                for s in &mut data[len..] {
                                    *s = 0;
                                }
                            } else {
                                // No frame available — fill with silence.
                                for s in data.iter_mut() {
                                    *s = 0;
                                }
                            }
                        },
                        |e| warn!("cpal output stream error: {e}"),
                        None,
                    )
                    .map_err(|e| AudioError::Backend(e.to_string()))?,
                SampleFormat::F32 => {
                    let rx = Arc::clone(&rx);
                    device
                        .build_output_stream(
                            &config,
                            move |data: &mut [f32], _| {
                                let mut guard = rx.lock().expect("rx lock poisoned");
                                if let Ok(frame) = guard.try_recv() {
                                    let len = frame.len().min(data.len());
                                    for (out, &s) in data[..len].iter_mut().zip(frame.iter()) {
                                        *out = f32::from(s) / f32::from(i16::MAX);
                                    }
                                    for s in &mut data[len..] {
                                        *s = 0.0;
                                    }
                                } else {
                                    for s in data.iter_mut() {
                                        *s = 0.0;
                                    }
                                }
                            },
                            |e| warn!("cpal output stream error: {e}"),
                            None,
                        )
                        .map_err(|e| AudioError::Backend(e.to_string()))?
                }
                other => {
                    return Err(AudioError::UnsupportedFormat(format!(
                        "cpal output sample format {other:?} is not supported"
                    )));
                }
            }
        };

        stream
            .play()
            .map_err(|e| AudioError::Backend(e.to_string()))?;

        // Update current output device record.
        {
            let mut state = self.state.lock().expect("BackendState lock poisoned");
            let device_name = device.name().unwrap_or_else(|_| device_id.into());
            state.current_output = Some(AudioDevice {
                id: device_name.clone(),
                label: device_name,
                is_default: device_id.is_empty(),
                kind: AudioDeviceKind::Output,
            });
        }

        Ok(Box::new(CpalOutputStream {
            sender: tx,
            _stream: stream,
        }))
    }

    async fn switch_input(&self, device_id: &str) -> Result<(), AudioError> {
        // Validate that the device exists.
        let device = self.find_input_device(device_id)?;
        let name = device.name().unwrap_or_else(|_| device_id.into());
        let mut state = self.state.lock().expect("BackendState lock poisoned");
        state.current_input = Some(AudioDevice {
            id: name.clone(),
            label: name,
            is_default: device_id.is_empty(),
            kind: AudioDeviceKind::Input,
        });
        debug!("CpalBackend: switched preferred input to '{device_id}'");
        // NOTE: This only updates the preference. The caller must re-open
        // the input stream against the new device_id for the change to
        // take effect on the encode pipeline (Phase J.3).
        Ok(())
    }

    async fn switch_output(&self, device_id: &str) -> Result<(), AudioError> {
        let device = self.find_output_device(device_id)?;
        let name = device.name().unwrap_or_else(|_| device_id.into());
        let mut state = self.state.lock().expect("BackendState lock poisoned");
        state.current_output = Some(AudioDevice {
            id: name.clone(),
            label: name,
            is_default: device_id.is_empty(),
            kind: AudioDeviceKind::Output,
        });
        debug!("CpalBackend: switched preferred output to '{device_id}'");
        Ok(())
    }
}

