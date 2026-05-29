//! `MockVideoBackend` — a no-hardware test double that compiles on all targets.
//!
//! Use this in unit tests to exercise the [`VideoBackend`] trait surface
//! without needing real camera hardware or browser APIs.
//!
//! # Generated test pattern
//!
//! The mock camera stream produces a procedurally-generated color-gradient
//! frame at 480×360 (BGRA) every ~33 ms (≈30 fps). The gradient shifts by
//! one hue step per frame so consecutive frames are distinguishably different,
//! which lets tests assert temporal progression.
//!
//! The mock screen-share stream produces the same frames but with `is_screen`
//! set on the enumerated [`ScreenSource`].
//!
//! # Pre-registered devices
//!
//! By default the mock advertises:
//! - Camera: `"mock-camera"` (default)
//! - Screen: `"mock-screen-0"` (full screen)

// Synthetic test-pattern generator: all arithmetic below is gradient /
// color-bar pixel math bounded by the frame dimensions, and the `.expect()`s
// are on infallible in-test conversions. The overflow / division / expect
// panic-class lints would fire on every pixel expression, so they are allowed
// module-wide rather than line-by-line. See feedback_test_lints.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::integer_division,
    clippy::indexing_slicing,
    clippy::expect_used
)]

use std::sync::{Arc, Mutex};

use futures::stream;

use crate::{
    error::VideoError,
    types::{ScreenSource, VideoDevice, VideoFrame, VideoPixelFormat},
    BoxVideoStream, VideoBackend, VideoInputStream,
};

// ── Frame generation ──────────────────────────────────────────────────────────

/// Generate a single BGRA gradient frame.
///
/// The gradient sweeps from hue `offset` (0–255) across the frame width, with
/// brightness increasing top-to-bottom. `offset` is advanced per frame so
/// successive frames differ.
#[must_use]
pub fn generate_gradient_frame(width: u32, height: u32, offset: u8, timestamp_ms: u64) -> VideoFrame {
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            // Hue sweeps across width, brightness across height.
            let hue = ((x * 255 / width.max(1)) as u8).wrapping_add(offset);
            let val = (y * 255 / height.max(1)) as u8;
            // Simple hue → RGB (6 sectors).
            let (r, g, b) = hue_to_rgb(hue, val);
            // BGRA order.
            data.push(b);
            data.push(g);
            data.push(r);
            data.push(255u8); // alpha = fully opaque
        }
    }
    VideoFrame {
        width,
        height,
        format: VideoPixelFormat::Bgra,
        data,
        timestamp_ms,
    }
}

fn hue_to_rgb(hue: u8, brightness: u8) -> (u8, u8, u8) {
    let h = u32::from(hue);
    let sector = h / 43;
    let rem = (h - sector * 43) * 6;
    let p = 0u32;
    let q = (255u32 - rem).min(255) as u8;
    let t = rem.min(255) as u8;
    let v = brightness;
    match sector % 6 {
        0 => (v, t, p as u8),
        1 => (q, v, p as u8),
        2 => (p as u8, v, t),
        3 => (p as u8, q, v),
        4 => (t, p as u8, v),
        _ => (v, p as u8, q),
    }
}

// ── State tracking ────────────────────────────────────────────────────────────

/// Mutable state tracked across calls (for assertion in tests).
#[derive(Default)]
pub struct MockVideoState {
    /// How many times `open_camera` was called.
    pub open_camera_calls: usize,
    /// How many times `open_screen_share` was called.
    pub open_screen_calls: usize,
    /// The most recently opened camera device_id.
    pub last_camera_device_id: Option<String>,
    /// The most recently opened screen source_id.
    pub last_screen_source_id: Option<String>,
}

/// A no-hardware `VideoBackend` suitable for unit testing.
pub struct MockVideoBackend {
    cameras: Vec<VideoDevice>,
    screens: Vec<ScreenSource>,
    /// Number of gradient frames to emit per open stream.
    /// `0` means the stream immediately ends (useful for testing EOF handling).
    pub frames_per_stream: usize,
    pub state: Arc<Mutex<MockVideoState>>,
}

impl Default for MockVideoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockVideoBackend {
    /// Construct with default mock devices and 3 frames per stream.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cameras: vec![VideoDevice::new_default("mock-camera", "Mock Camera (480×360)")],
            screens: vec![ScreenSource::screen("mock-screen-0", "Mock Entire Screen")],
            frames_per_stream: 3,
            state: Arc::new(Mutex::new(MockVideoState::default())),
        }
    }

    /// Construct with custom camera and screen lists.
    #[must_use]
    pub fn with_devices(cameras: Vec<VideoDevice>, screens: Vec<ScreenSource>) -> Self {
        Self {
            cameras,
            screens,
            frames_per_stream: 3,
            state: Arc::new(Mutex::new(MockVideoState::default())),
        }
    }

    /// Return a snapshot of current call counts for test assertions.
    #[must_use] 
    pub fn state_snapshot(&self) -> MockVideoState {
        let g = self.state.lock().expect("MockVideoState lock poisoned");
        MockVideoState {
            open_camera_calls: g.open_camera_calls,
            open_screen_calls: g.open_screen_calls,
            last_camera_device_id: g.last_camera_device_id.clone(),
            last_screen_source_id: g.last_screen_source_id.clone(),
        }
    }
}

// ── MockVideoInputStream ──────────────────────────────────────────────────────

/// A mock [`VideoInputStream`] that emits a fixed number of gradient frames
/// then signals EOF.
pub struct MockVideoInputStream {
    width: u32,
    height: u32,
    remaining: usize,
    frame_index: u8,
    next_timestamp_ms: u64,
}

impl MockVideoInputStream {
    fn new(width: u32, height: u32, frames: usize) -> Self {
        Self {
            width,
            height,
            remaining: frames,
            frame_index: 0,
            next_timestamp_ms: 0,
        }
    }
}

impl VideoInputStream for MockVideoInputStream {
    fn poll_next_frame(&mut self) -> Option<VideoFrame> {
        if self.remaining == 0 {
            return None;
        }
        let frame = generate_gradient_frame(
            self.width,
            self.height,
            self.frame_index,
            self.next_timestamp_ms,
        );
        self.remaining -= 1;
        self.frame_index = self.frame_index.wrapping_add(8); // advance hue offset
        self.next_timestamp_ms += 33; // ~30 fps
        Some(frame)
    }
}

// ── VideoBackend impl ─────────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl VideoBackend for MockVideoBackend {
    async fn enumerate_cameras(&self) -> Result<Vec<VideoDevice>, VideoError> {
        Ok(self.cameras.clone())
    }

    async fn enumerate_screens(&self) -> Result<Vec<ScreenSource>, VideoError> {
        Ok(self.screens.clone())
    }

    async fn open_camera(
        &self,
        device_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError> {
        let resolved = if device_id.is_empty() {
            self.cameras
                .iter()
                .find(|d| d.is_default)
                .or_else(|| self.cameras.first()).map_or_else(|| "mock-camera".into(), |d| d.id.clone())
        } else {
            device_id.to_owned()
        };

        if !self.cameras.iter().any(|d| d.id == resolved) {
            return Err(VideoError::DeviceNotFound(resolved));
        }

        {
            let mut s = self.state.lock().expect("MockVideoState lock poisoned");
            s.open_camera_calls += 1;
            s.last_camera_device_id = Some(resolved);
        }

        Ok(Box::new(MockVideoInputStream::new(
            480,
            360,
            self.frames_per_stream,
        )))
    }

    async fn open_screen_share(
        &self,
        source_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError> {
        let resolved = if source_id.is_empty() {
            self.screens
                .first().map_or_else(|| "mock-screen-0".into(), |s| s.id.clone())
        } else {
            source_id.to_owned()
        };

        if !self.screens.iter().any(|s| s.id == resolved) {
            return Err(VideoError::DeviceNotFound(resolved));
        }

        {
            let mut s = self.state.lock().expect("MockVideoState lock poisoned");
            s.open_screen_calls += 1;
            s.last_screen_source_id = Some(resolved);
        }

        Ok(Box::new(MockVideoInputStream::new(
            480,
            360,
            self.frames_per_stream,
        )))
    }
}

// ── Convenience: build a BoxVideoStream from MockVideoInputStream ─────────────

/// Build a [`BoxVideoStream`] from the mock — converts the sync
/// `poll_next_frame` loop into a futures::Stream.
#[must_use] 
pub fn mock_stream(frames: usize) -> BoxVideoStream {
    let frames: Vec<_> = (0..frames as u8)
        .map(|i| generate_gradient_frame(480, 360, i.wrapping_mul(8), u64::from(i) * 33))
        .collect();
    Box::pin(stream::iter(frames))
}
