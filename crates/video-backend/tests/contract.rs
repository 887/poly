//! Contract tests for the `VideoBackend` trait using `MockVideoBackend`.
//!
//! These tests verify that any impl of `VideoBackend` upholding the trait
//! contract passes a fixed set of assertions. When real impls land (Phase E.3
//! `native` / `web` features), run the same tests against them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use poly_video_backend::{
    test_support::{generate_gradient_frame, mock_stream, MockVideoBackend},
    VideoBackend, VideoError, VideoPixelFormat,
};
use futures::StreamExt;

// ── enumerate_cameras ─────────────────────────────────────────────────────────

#[tokio::test]
async fn enumerate_cameras_returns_at_least_one() {
    let backend = MockVideoBackend::new();
    let cameras = backend.enumerate_cameras().await.unwrap();
    assert!(!cameras.is_empty(), "enumerate_cameras must return at least one device");
}

#[tokio::test]
async fn enumerate_cameras_default_device_exists() {
    let backend = MockVideoBackend::new();
    let cameras = backend.enumerate_cameras().await.unwrap();
    let has_default = cameras.iter().any(|d| d.is_default);
    assert!(has_default, "at least one camera must be marked is_default");
}

#[tokio::test]
async fn enumerate_cameras_ids_are_non_empty() {
    let backend = MockVideoBackend::new();
    let cameras = backend.enumerate_cameras().await.unwrap();
    for cam in &cameras {
        assert!(!cam.id.is_empty(), "camera id must not be empty; got {:?}", cam);
        assert!(!cam.label.is_empty(), "camera label must not be empty; got {:?}", cam);
    }
}

// ── enumerate_screens ─────────────────────────────────────────────────────────

#[tokio::test]
async fn enumerate_screens_returns_at_least_one() {
    let backend = MockVideoBackend::new();
    let screens = backend.enumerate_screens().await.unwrap();
    assert!(!screens.is_empty(), "enumerate_screens must return at least one source");
}

#[tokio::test]
async fn enumerate_screens_ids_are_non_empty() {
    let backend = MockVideoBackend::new();
    let screens = backend.enumerate_screens().await.unwrap();
    for src in &screens {
        assert!(!src.id.is_empty(), "screen source id must not be empty");
        assert!(!src.label.is_empty(), "screen source label must not be empty");
    }
}

// ── open_camera ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn open_camera_default_returns_stream() {
    let backend = MockVideoBackend::new();
    // Empty string → default camera
    let mut stream = backend.open_camera("").await.unwrap();
    // Mock emits 3 frames by default; at least one must come.
    let first_frame = stream.poll_next_frame();
    assert!(first_frame.is_some(), "open_camera must yield at least one frame");
}

#[tokio::test]
async fn open_camera_by_id_updates_state() {
    let backend = MockVideoBackend::new();
    let _stream = backend.open_camera("mock-camera").await.unwrap();
    let snap = backend.state_snapshot();
    assert_eq!(snap.open_camera_calls, 1);
    assert_eq!(snap.last_camera_device_id.as_deref(), Some("mock-camera"));
}

#[tokio::test]
async fn open_camera_unknown_device_errors() {
    let backend = MockVideoBackend::new();
    let result = backend.open_camera("no-such-camera-42").await;
    assert!(
        matches!(result, Err(VideoError::DeviceNotFound(_))),
        "expected DeviceNotFound"
    );
}

#[tokio::test]
async fn open_camera_frames_have_correct_format() {
    let backend = MockVideoBackend::new();
    let mut stream = backend.open_camera("").await.unwrap();
    let frame = stream.poll_next_frame().unwrap();
    assert_eq!(frame.format, VideoPixelFormat::Bgra, "mock frame must be BGRA");
    assert_eq!(frame.width, 480);
    assert_eq!(frame.height, 360);
    let expected_len = frame.width as usize * frame.height as usize * 4;
    assert_eq!(
        frame.data.len(),
        expected_len,
        "BGRA frame byte length must be width*height*4"
    );
}

// ── open_screen_share ─────────────────────────────────────────────────────────

#[tokio::test]
async fn open_screen_share_default_returns_stream() {
    let backend = MockVideoBackend::new();
    let mut stream = backend.open_screen_share("").await.unwrap();
    let first_frame = stream.poll_next_frame();
    assert!(first_frame.is_some(), "open_screen_share must yield at least one frame");
}

#[tokio::test]
async fn open_screen_share_by_id_updates_state() {
    let backend = MockVideoBackend::new();
    let _stream = backend.open_screen_share("mock-screen-0").await.unwrap();
    let snap = backend.state_snapshot();
    assert_eq!(snap.open_screen_calls, 1);
    assert_eq!(snap.last_screen_source_id.as_deref(), Some("mock-screen-0"));
}

#[tokio::test]
async fn open_screen_share_unknown_source_errors() {
    let backend = MockVideoBackend::new();
    let result = backend.open_screen_share("no-such-screen-42").await;
    assert!(
        matches!(result, Err(VideoError::DeviceNotFound(_))),
        "expected DeviceNotFound"
    );
}

// ── VideoFrame helpers ────────────────────────────────────────────────────────

#[test]
fn video_frame_expected_len_bgra() {
    use poly_video_backend::VideoPixelFormat;
    let len = poly_video_backend::VideoFrame::expected_len(480, 360, VideoPixelFormat::Bgra);
    assert_eq!(len, 480 * 360 * 4);
}

#[test]
fn video_frame_expected_len_yuv420p() {
    use poly_video_backend::VideoPixelFormat;
    // YUV420p: Y = w*h, U = w/2*h/2, V = w/2*h/2 → total = w*h * 3/2
    let len = poly_video_backend::VideoFrame::expected_len(480, 360, VideoPixelFormat::Yuv420p);
    assert_eq!(len, 480 * 360 * 3 / 2);
}

// ── gradient frame generation ─────────────────────────────────────────────────

#[test]
fn generate_gradient_frame_correct_length() {
    let frame = generate_gradient_frame(320, 240, 0, 0);
    assert_eq!(frame.data.len(), 320 * 240 * 4);
    assert_eq!(frame.format, VideoPixelFormat::Bgra);
}

#[test]
fn generate_gradient_frame_consecutive_differ() {
    let frame0 = generate_gradient_frame(480, 360, 0, 0);
    let frame1 = generate_gradient_frame(480, 360, 8, 33);
    // With different offsets the first pixel should differ.
    assert_ne!(
        frame0.data[0..4],
        frame1.data[0..4],
        "consecutive frames with different offsets must differ"
    );
}

// ── mock_stream helper ────────────────────────────────────────────────────────

#[tokio::test]
async fn mock_stream_yields_correct_count() {
    let mut stream = mock_stream(5);
    let mut count = 0usize;
    while let Some(_frame) = stream.next().await {
        count += 1;
    }
    assert_eq!(count, 5, "mock_stream(5) must yield exactly 5 frames");
}

#[tokio::test]
async fn mock_stream_zero_frames_yields_nothing() {
    let mut stream = mock_stream(0);
    assert!(stream.next().await.is_none(), "mock_stream(0) must be empty");
}
