//! `poly-video-backend` — protocol-agnostic video I/O abstraction.
//!
//! # Design
//!
//! One [`VideoBackend`] trait drives all video capture operations across
//! shells. It mirrors the shape of `poly-audio-backend` so the two crates
//! can be wired together cleanly in Phase E.
//!
//! | Shell | Feature | Impl (planned) |
//! |---|---|---|
//! | Wry (apps/desktop) | `native` (future) | nokhwa + scap |
//! | apps/web (browser) | `web` (future) | getUserMedia / getDisplayMedia |
//! | Electron renderer | `web` (future) | Same as browser shell |
//!
//! # Phase E scope note (2026-05-14)
//!
//! This crate currently ships **trait surface + mock impl only**. The real
//! native and web capture backends are **deferred** pending explicit user
//! decision on the binary-size cost:
//!
//! - `webrtc-rs` — H.264 / VP8 / VP9 codec + ICE/DTLS stack → adds ~5 MB to
//!   native binaries and requires non-trivial build dependencies.
//! - `openh264-rs` — software H.264 encoder/decoder (bundled C library).
//! - `nokhwa` — native camera capture (V4L2 / AVFoundation / MSMF).
//! - `scap` — native screen capture (Wayland/X11/macOS/Win).
//!
//! When the user approves those dependencies, add them gated behind
//! `features = ["native"]` and `features = ["web"]` respectively.
//! The trait surface below is designed to require zero changes when the
//! impls land.
//!
//! See `docs/plans/plan-voice-video-calls.md` Phase E.3, E.4, E.5, E.6.
//!
//! # Pixel format
//!
//! All streams default to BGRA (4 bytes/pixel). The mock generates gradient
//! BGRA frames. Real impls may emit Yuv420p for the H.264 encoder path —
//! see [`VideoPixelFormat`].
//!
//! # Stream model
//!
//! [`VideoInputStream`] uses a synchronous `poll_next_frame` rather than a
//! `futures::Stream` to avoid boxing overhead in the hot encode path. A
//! [`BoxVideoStream`] adaptor (futures::Stream wrapper) is also provided for
//! callers that need the async Stream interface.

pub mod error;
pub mod mock_backend;
pub mod test_support;
pub mod types;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use error::VideoError;
pub use types::{ScreenSource, VideoDevice, VideoFrame, VideoPixelFormat};

use futures::Stream;
use std::pin::Pin;

// ── VideoInputStream ──────────────────────────────────────────────────────────

/// A synchronous video frame source.
///
/// The synchronous interface avoids boxing overhead in the hot encode path.
/// Call [`poll_next_frame`] in a loop (typically on a dedicated capture
/// thread) until it returns `None` (stream ended / device lost).
///
/// # WASM note
///
/// On `wasm32-unknown-unknown` there are no dedicated threads, so the mock
/// impl yields a fixed number of pre-generated frames without blocking. Real
/// impls on web will use a callback-based `MediaStreamTrack` + `ReadableStream`
/// bridge (deferred to Phase E.3 `web` feature).
///
/// [`poll_next_frame`]: VideoInputStream::poll_next_frame
pub trait VideoInputStream: Send {
    /// Return the next frame, or `None` if the stream has ended.
    ///
    /// Implementations SHOULD return `None` when the device is removed
    /// (triggering [`VideoError::DeviceLost`] at the call site is the
    /// preferred pattern for async callers).
    fn poll_next_frame(&mut self) -> Option<VideoFrame>;
}

/// A heap-allocated, pinned async stream of [`VideoFrame`]s.
///
/// Use this when you need a `futures::Stream` interface (e.g. integration
/// with async pipelines in the Phase E encode path).
pub type BoxVideoStream = Pin<Box<dyn Stream<Item = VideoFrame> + Send>>;

// ── VideoBackend ──────────────────────────────────────────────────────────────

/// The primary video abstraction.
///
/// Implementations are expected to be cheaply cloneable (e.g. `Arc`-wrapped)
/// so they can be shared between the capture and encode loops in Phase E.5.
///
/// On WASM the trait is `?Send`; on native it is `Send + Sync`.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait VideoBackend {
    // ── Device enumeration ───────────────────────────────────────────────

    /// List all available camera / capture-card devices.
    ///
    /// IDs MUST be stable across enumerations (used as KV keys for
    /// "remember last camera" in Phase J device-picker follow-up).
    async fn enumerate_cameras(&self) -> Result<Vec<VideoDevice>, VideoError>;

    /// List all available screen / window sources for screen sharing.
    ///
    /// On web, this triggers the `getDisplayMedia` picker dialog —
    /// call only in direct response to a user gesture (Phase E.4).
    /// On native (scap), this enumerates non-interactively.
    ///
    /// Source IDs on web are NOT stable — do not persist them as KV keys.
    async fn enumerate_screens(&self) -> Result<Vec<ScreenSource>, VideoError>;

    // ── Stream lifecycle ─────────────────────────────────────────────────

    /// Open a video capture stream from a camera with `device_id`.
    ///
    /// Passing `""` (empty string) selects the system default camera.
    ///
    /// Returns a [`VideoInputStream`] that yields frames until dropped or
    /// the device is removed.
    ///
    /// # Errors
    ///
    /// - [`VideoError::DeviceNotFound`] — unknown or unavailable device.
    /// - [`VideoError::PermissionDenied`] — browser or OS denied access.
    /// - [`VideoError::NotSupported`] — real capture not yet implemented
    ///   (mock always returns Ok).
    async fn open_camera(
        &self,
        device_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError>;

    /// Open a screen / window capture stream.
    ///
    /// Passing `""` selects the first available screen source.
    ///
    /// On web, `getDisplayMedia` shows the OS picker and blocks until the
    /// user makes a selection (or cancels, returning [`VideoError::PermissionDenied`]).
    async fn open_screen_share(
        &self,
        source_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError>;
}

// ── Internal tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::mock_backend::MockVideoBackend;

    #[tokio::test]
    async fn mock_backend_enumerate_cameras() {
        let backend = MockVideoBackend::new();
        let cameras = backend.enumerate_cameras().await.unwrap();
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].id, "mock-camera");
        assert!(cameras[0].is_default);
    }

    #[tokio::test]
    async fn mock_backend_enumerate_screens() {
        let backend = MockVideoBackend::new();
        let screens = backend.enumerate_screens().await.unwrap();
        assert_eq!(screens.len(), 1);
        assert_eq!(screens[0].id, "mock-screen-0");
        assert!(screens[0].is_screen);
    }
}
