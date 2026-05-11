//! `FakeAudioBackend` — a no-hardware test double that compiles on all targets.
//!
//! Use this in unit tests to exercise the [`AudioBackend`] trait surface
//! without needing real audio hardware or browser APIs.
//!
//! The fake produces a stream of silence (all-zero PCM frames) and silently
//! discards any output pushed to it.
//!
//! # Pre-registered devices
//!
//! By default the fake advertises two devices:
//! - Input: `"fake-mic"` (default)
//! - Output: `"fake-speaker"` (default)
//!
//! Tests can inject custom device lists via [`FakeAudioBackend::with_devices`].

use std::sync::{Arc, Mutex};

use futures::stream;

use crate::{
    error::AudioError,
    types::{AudioDevice, AudioDeviceKind, AudioFormat},
    AudioBackend, AudioOutputStream, BoxInputStream,
};

/// Pre-canned audio device for the fake backend.
#[derive(Clone, Debug)]
struct FakeDeviceSet {
    inputs: Vec<AudioDevice>,
    outputs: Vec<AudioDevice>,
}

impl Default for FakeDeviceSet {
    fn default() -> Self {
        Self {
            inputs: vec![AudioDevice::new_default(
                "fake-mic",
                "Fake Microphone",
                AudioDeviceKind::Input,
            )],
            outputs: vec![AudioDevice::new_default(
                "fake-speaker",
                "Fake Speaker",
                AudioDeviceKind::Output,
            )],
        }
    }
}

/// Mutable state tracked across calls (for assertion in tests).
#[derive(Default)]
pub struct FakeState {
    pub current_input: Option<AudioDevice>,
    pub current_output: Option<AudioDevice>,
    /// How many times `open_input` was called.
    pub open_input_calls: usize,
    /// How many times `open_output` was called.
    pub open_output_calls: usize,
    /// Total number of PCM samples pushed to any output stream.
    pub output_samples_pushed: usize,
}

/// A no-hardware `AudioBackend` suitable for unit testing.
///
/// `Arc`-wrapped so it can be shared between the backend and the test's
/// state inspector.
pub struct FakeAudioBackend {
    devices: FakeDeviceSet,
    pub state: Arc<Mutex<FakeState>>,
}

impl Default for FakeAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeAudioBackend {
    /// Construct with default fake devices (`"fake-mic"` + `"fake-speaker"`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            devices: FakeDeviceSet::default(),
            state: Arc::new(Mutex::new(FakeState::default())),
        }
    }

    /// Construct with a custom set of input and output devices.
    #[must_use]
    pub fn with_devices(inputs: Vec<AudioDevice>, outputs: Vec<AudioDevice>) -> Self {
        Self {
            devices: FakeDeviceSet { inputs, outputs },
            state: Arc::new(Mutex::new(FakeState::default())),
        }
    }

    /// Return a clone of the current state snapshot for assertions.
    ///
    /// ```
    /// # use poly_audio_backend::fake_backend::FakeAudioBackend;
    /// let backend = FakeAudioBackend::new();
    /// // ... use backend ...
    /// let snap = backend.state_snapshot();
    /// assert_eq!(snap.open_input_calls, 0);
    /// ```
    pub fn state_snapshot(&self) -> FakeState {
        let guard = self.state.lock().expect("FakeState lock poisoned");
        FakeState {
            current_input: guard.current_input.clone(),
            current_output: guard.current_output.clone(),
            open_input_calls: guard.open_input_calls,
            open_output_calls: guard.open_output_calls,
            output_samples_pushed: guard.output_samples_pushed,
        }
    }
}

// ── Fake OutputStream ────────────────────────────────────────────────────────

struct FakeOutputStream {
    state: Arc<Mutex<FakeState>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl AudioOutputStream for FakeOutputStream {
    async fn push(&self, frame: &[i16]) -> Result<(), AudioError> {
        self.state
            .lock()
            .expect("FakeState lock poisoned")
            .output_samples_pushed += frame.len();
        Ok(())
    }

    async fn close(&self) -> Result<(), AudioError> {
        Ok(())
    }
}

// ── AudioBackend impl ────────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl AudioBackend for FakeAudioBackend {
    async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        Ok(self.devices.inputs.clone())
    }

    async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        Ok(self.devices.outputs.clone())
    }

    fn current_input_device(&self) -> Option<AudioDevice> {
        self.state
            .lock()
            .expect("FakeState lock poisoned")
            .current_input
            .clone()
    }

    fn current_output_device(&self) -> Option<AudioDevice> {
        self.state
            .lock()
            .expect("FakeState lock poisoned")
            .current_output
            .clone()
    }

    async fn open_input(
        &self,
        device_id: &str,
        _format: AudioFormat,
    ) -> Result<BoxInputStream, AudioError> {
        // Validate device.
        let resolved_id = if device_id.is_empty() {
            self.devices
                .inputs
                .iter()
                .find(|d| d.is_default)
                .or_else(|| self.devices.inputs.first())
                .map(|d| d.id.clone())
                .unwrap_or_else(|| "fake-mic".into())
        } else {
            device_id.to_owned()
        };

        if !self.devices.inputs.iter().any(|d| d.id == resolved_id) {
            return Err(AudioError::DeviceNotFound(resolved_id));
        }

        let device = self
            .devices
            .inputs
            .iter()
            .find(|d| d.id == resolved_id)
            .cloned()
            .expect("just validated above");

        {
            let mut state = self.state.lock().expect("FakeState lock poisoned");
            state.current_input = Some(device);
            state.open_input_calls += 1;
        }

        // Return an empty stream (silence). Downstream Opus encoders will
        // see no frames and produce no packets — correct for a test stub.
        Ok(Box::pin(stream::empty()))
    }

    async fn open_output(
        &self,
        device_id: &str,
        _format: AudioFormat,
    ) -> Result<Box<dyn AudioOutputStream>, AudioError> {
        let resolved_id = if device_id.is_empty() {
            self.devices
                .outputs
                .iter()
                .find(|d| d.is_default)
                .or_else(|| self.devices.outputs.first())
                .map(|d| d.id.clone())
                .unwrap_or_else(|| "fake-speaker".into())
        } else {
            device_id.to_owned()
        };

        if !self.devices.outputs.iter().any(|d| d.id == resolved_id) {
            return Err(AudioError::DeviceNotFound(resolved_id));
        }

        let device = self
            .devices
            .outputs
            .iter()
            .find(|d| d.id == resolved_id)
            .cloned()
            .expect("just validated above");

        {
            let mut state = self.state.lock().expect("FakeState lock poisoned");
            state.current_output = Some(device);
            state.open_output_calls += 1;
        }

        Ok(Box::new(FakeOutputStream {
            state: Arc::clone(&self.state),
        }))
    }

    async fn switch_input(&self, device_id: &str) -> Result<(), AudioError> {
        if !device_id.is_empty() && !self.devices.inputs.iter().any(|d| d.id == device_id) {
            return Err(AudioError::DeviceNotFound(device_id.into()));
        }
        let device = if device_id.is_empty() {
            self.devices
                .inputs
                .iter()
                .find(|d| d.is_default)
                .cloned()
                .unwrap_or_else(|| self.devices.inputs[0].clone())
        } else {
            self.devices
                .inputs
                .iter()
                .find(|d| d.id == device_id)
                .cloned()
                .expect("validated above")
        };
        self.state.lock().expect("FakeState lock poisoned").current_input = Some(device);
        Ok(())
    }

    async fn switch_output(&self, device_id: &str) -> Result<(), AudioError> {
        if !device_id.is_empty() && !self.devices.outputs.iter().any(|d| d.id == device_id) {
            return Err(AudioError::DeviceNotFound(device_id.into()));
        }
        let device = if device_id.is_empty() {
            self.devices
                .outputs
                .iter()
                .find(|d| d.is_default)
                .cloned()
                .unwrap_or_else(|| self.devices.outputs[0].clone())
        } else {
            self.devices
                .outputs
                .iter()
                .find(|d| d.id == device_id)
                .cloned()
                .expect("validated above")
        };
        self.state.lock().expect("FakeState lock poisoned").current_output = Some(device);
        Ok(())
    }
}
