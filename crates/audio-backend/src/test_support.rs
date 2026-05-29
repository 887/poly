//! `MockAudioBackend` — an instrumented test double for `AudioBackend` trait contracts.
//!
//! Unlike [`crate::fake_backend::FakeAudioBackend`], `MockAudioBackend` tracks
//! *hot-swap* events (switch without close), provides configurable multi-device
//! lists, and exposes a channel-based event log for asserting on call sequences.
//!
//! # Usage
//!
//! ```rust,no_run
//! # use poly_audio_backend::test_support::{MockAudioBackend, MockEvent};
//! let mock = MockAudioBackend::new();
//! // ...exercise AudioBackend methods...
//! let events = mock.drain_events();
//! assert!(events.contains(&MockEvent::SwitchInput("fake-mic".into())));
//! ```

// Instrumented test double: `.expect()`/panics on misuse are acceptable, and
// in-test arithmetic is on small fixed sizes. See feedback_test_lints.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::integer_division
)]

use std::sync::{Arc, Mutex};

use futures::stream;

use crate::{
    error::AudioError,
    types::{AudioDevice, AudioDeviceKind, AudioFormat},
    AudioBackend, AudioOutputStream, BoxInputStream,
};

// ── Event log ────────────────────────────────────────────────────────────────

/// An event emitted by `MockAudioBackend` each time a method is called.
///
/// Inspect the event log via [`MockAudioBackend::drain_events`] in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockEvent {
    ListInputDevices,
    ListOutputDevices,
    CurrentInputDevice,
    CurrentOutputDevice,
    OpenInput(String),
    OpenOutput(String),
    /// `switch_input` called with this device_id — fires even when no stream is open.
    SwitchInput(String),
    /// `switch_output` called with this device_id — fires even when no stream is open.
    SwitchOutput(String),
    PushSamples(usize),
    CloseOutput,
}

// ── Shared mutable state ─────────────────────────────────────────────────────

#[derive(Default)]
struct MockState {
    current_input: Option<AudioDevice>,
    current_output: Option<AudioDevice>,
    events: Vec<MockEvent>,
    /// PCM samples accumulated across all push() calls.
    total_pushed: usize,
}

// ── MockAudioBackend ─────────────────────────────────────────────────────────

/// An instrumented `AudioBackend` for contract tests.
///
/// - Advertises up to four devices: `mock-mic-a`, `mock-mic-b`, `mock-speaker-a`,
///   `mock-speaker-b`. Tests may inject a custom device list via
///   [`MockAudioBackend::with_devices`].
/// - Records every method call as a [`MockEvent`] in the event log.
/// - Returns [`AudioError::DeviceNotFound`] for unknown device IDs.
pub struct MockAudioBackend {
    inputs: Vec<AudioDevice>,
    outputs: Vec<AudioDevice>,
    state: Arc<Mutex<MockState>>,
}

impl Default for MockAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAudioBackend {
    /// Construct with two input and two output devices.
    #[must_use]
    pub fn new() -> Self {
        let inputs = vec![
            AudioDevice::new_default("mock-mic-a", "Mock Mic A (default)", AudioDeviceKind::Input),
            AudioDevice::new("mock-mic-b", "Mock Mic B", AudioDeviceKind::Input),
        ];
        let outputs = vec![
            AudioDevice::new_default(
                "mock-speaker-a",
                "Mock Speaker A (default)",
                AudioDeviceKind::Output,
            ),
            AudioDevice::new("mock-speaker-b", "Mock Speaker B", AudioDeviceKind::Output),
        ];
        Self {
            inputs,
            outputs,
            state: Arc::new(Mutex::new(MockState::default())),
        }
    }

    /// Construct with a custom device set.
    #[must_use]
    pub fn with_devices(inputs: Vec<AudioDevice>, outputs: Vec<AudioDevice>) -> Self {
        Self {
            inputs,
            outputs,
            state: Arc::new(Mutex::new(MockState::default())),
        }
    }

    /// Drain all recorded events since the last call.
    #[must_use] 
    pub fn drain_events(&self) -> Vec<MockEvent> {
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .events
            .drain(..)
            .collect()
    }

    /// Peek at the event log without clearing it.
    #[must_use] 
    pub fn peek_events(&self) -> Vec<MockEvent> {
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .events
            .clone()
    }

    /// Total number of PCM samples pushed to any output stream.
    #[must_use] 
    pub fn total_pushed(&self) -> usize {
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .total_pushed
    }

    fn log(&self, event: MockEvent) {
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .events
            .push(event);
    }

    fn resolve_input_device(&self, device_id: &str) -> Result<AudioDevice, AudioError> {
        if device_id.is_empty() {
            return self
                .inputs
                .iter()
                .find(|d| d.is_default)
                .or_else(|| self.inputs.first())
                .cloned()
                .ok_or_else(|| AudioError::DeviceNotFound("(no input devices)".into()));
        }
        self.inputs
            .iter()
            .find(|d| d.id == device_id)
            .cloned()
            .ok_or_else(|| AudioError::DeviceNotFound(device_id.into()))
    }

    fn resolve_output_device(&self, device_id: &str) -> Result<AudioDevice, AudioError> {
        if device_id.is_empty() {
            return self
                .outputs
                .iter()
                .find(|d| d.is_default)
                .or_else(|| self.outputs.first())
                .cloned()
                .ok_or_else(|| AudioError::DeviceNotFound("(no output devices)".into()));
        }
        self.outputs
            .iter()
            .find(|d| d.id == device_id)
            .cloned()
            .ok_or_else(|| AudioError::DeviceNotFound(device_id.into()))
    }
}

// ── Mock OutputStream ────────────────────────────────────────────────────────

struct MockOutputStream {
    state: Arc<Mutex<MockState>>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl AudioOutputStream for MockOutputStream {
    async fn push(&self, frame: &[i16]) -> Result<(), AudioError> {
        let mut st = self.state.lock().expect("MockState lock poisoned");
        st.total_pushed += frame.len();
        st.events.push(MockEvent::PushSamples(frame.len()));
        Ok(())
    }

    async fn close(&self) -> Result<(), AudioError> {
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .events
            .push(MockEvent::CloseOutput);
        Ok(())
    }
}

// ── AudioBackend impl ────────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl AudioBackend for MockAudioBackend {
    async fn list_input_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.log(MockEvent::ListInputDevices);
        Ok(self.inputs.clone())
    }

    async fn list_output_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.log(MockEvent::ListOutputDevices);
        Ok(self.outputs.clone())
    }

    fn current_input_device(&self) -> Option<AudioDevice> {
        self.log(MockEvent::CurrentInputDevice);
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .current_input
            .clone()
    }

    fn current_output_device(&self) -> Option<AudioDevice> {
        self.log(MockEvent::CurrentOutputDevice);
        self.state
            .lock()
            .expect("MockState lock poisoned")
            .current_output
            .clone()
    }

    async fn open_input(
        &self,
        device_id: &str,
        _format: AudioFormat,
    ) -> Result<BoxInputStream, AudioError> {
        let device = self.resolve_input_device(device_id)?;
        {
            let mut st = self.state.lock().expect("MockState lock poisoned");
            st.current_input = Some(device.clone());
            st.events.push(MockEvent::OpenInput(device.id.clone()));
        }
        Ok(Box::pin(stream::empty()))
    }

    async fn open_output(
        &self,
        device_id: &str,
        _format: AudioFormat,
    ) -> Result<Box<dyn AudioOutputStream>, AudioError> {
        let device = self.resolve_output_device(device_id)?;
        {
            let mut st = self.state.lock().expect("MockState lock poisoned");
            st.current_output = Some(device.clone());
            st.events.push(MockEvent::OpenOutput(device.id.clone()));
        }
        Ok(Box::new(MockOutputStream {
            state: Arc::clone(&self.state),
        }))
    }

    async fn switch_input(&self, device_id: &str) -> Result<(), AudioError> {
        let device = self.resolve_input_device(device_id)?;
        {
            let mut st = self.state.lock().expect("MockState lock poisoned");
            st.current_input = Some(device.clone());
            st.events.push(MockEvent::SwitchInput(device.id.clone()));
        }
        Ok(())
    }

    async fn switch_output(&self, device_id: &str) -> Result<(), AudioError> {
        let device = self.resolve_output_device(device_id)?;
        {
            let mut st = self.state.lock().expect("MockState lock poisoned");
            st.current_output = Some(device.clone());
            st.events.push(MockEvent::SwitchOutput(device.id.clone()));
        }
        Ok(())
    }
}
