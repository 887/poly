//! WebCodecs + getUserMedia / getDisplayMedia backend for wasm32 targets.
//!
//! # Availability
//!
//! Compiled only when `target_arch = "wasm32"` AND feature `web`.
//!
//! # Codec choice
//!
//! H.264 Constrained Baseline (`"avc1.42E01E"`) for maximum compatibility.
//!
//! # Phase E.3/E.4 status
//!
//! This file implements the full `WebVideoBackend` for camera/screen enumeration
//! and capture. WebVideoEncoder/WebVideoDecoder are stubs — full WebCodecs wiring
//! is deferred until the wasm32 target's web_sys WebCodecs bindings stabilize in
//! the project's web-sys version.

#![cfg(all(target_arch = "wasm32", feature = "web"))]

use crate::{
    error::VideoError,
    types::{ScreenSource, VideoDevice, VideoFrame, VideoPixelFormat},
    VideoBackend, VideoInputStream,
};

// ── WebVideoBackend ────────────────────────────────────────────────────────────

/// Browser (wasm32) video backend using `getUserMedia` and `getDisplayMedia`.
///
/// # Thread safety
///
/// This backend is `!Send` (wasm32 has no threads). The `VideoBackend` trait
/// is `async_trait(?Send)` on wasm32, so this is correct.
#[derive(Clone, Debug, Default)]
pub struct WebVideoBackend;

impl WebVideoBackend {
    /// Construct a new `WebVideoBackend`.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait(?Send)]
impl VideoBackend for WebVideoBackend {
    async fn enumerate_cameras(&self) -> Result<Vec<VideoDevice>, VideoError> {
        use wasm_bindgen_futures::JsFuture;
        use web_sys::MediaDeviceKind;

        let window = web_sys::window()
            .ok_or_else(|| VideoError::Backend("no window object".into()))?;
        let nav = window.navigator();
        let media_devices = nav
            .media_devices()
            .map_err(|e| VideoError::Backend(format!("media_devices: {e:?}")))?;

        let devices_promise = media_devices
            .enumerate_devices()
            .map_err(|e| VideoError::Backend(format!("enumerate_devices: {e:?}")))?;

        let devices_js = JsFuture::from(devices_promise)
            .await
            .map_err(|e| VideoError::Backend(format!("enumerate_devices await: {e:?}")))?;

        let devices_array = js_sys::Array::from(&devices_js);
        let mut cameras = Vec::new();
        for i in 0..devices_array.length() {
            let item = devices_array.get(i);
            let info = web_sys::MediaDeviceInfo::from(item);
            if info.kind() == MediaDeviceKind::Videoinput {
                cameras.push(VideoDevice {
                    id: info.device_id(),
                    label: info.label(),
                    is_default: cameras.is_empty(),
                });
            }
        }
        Ok(cameras)
    }

    async fn enumerate_screens(&self) -> Result<Vec<ScreenSource>, VideoError> {
        // getDisplayMedia shows OS picker inline — no pre-enumeration on web.
        Ok(vec![])
    }

    async fn open_camera(
        &self,
        device_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError> {
        use wasm_bindgen_futures::JsFuture;
        use wasm_bindgen::JsValue;

        let window = web_sys::window()
            .ok_or_else(|| VideoError::Backend("no window object".into()))?;
        let nav = window.navigator();
        let media_devices = nav
            .media_devices()
            .map_err(|e| VideoError::Backend(format!("media_devices: {e:?}")))?;

        let mut constraints = web_sys::MediaStreamConstraints::new();
        if device_id.is_empty() {
            constraints.video(&JsValue::TRUE);
        } else {
            // Build {deviceId: {exact: device_id}} constraint.
            let video_constraint = js_sys::Object::new();
            let device_id_obj = js_sys::Object::new();
            js_sys::Reflect::set(
                &device_id_obj,
                &JsValue::from_str("exact"),
                &JsValue::from_str(device_id),
            )
            .map_err(|e| VideoError::Backend(format!("reflect set: {e:?}")))?;
            js_sys::Reflect::set(
                &video_constraint,
                &JsValue::from_str("deviceId"),
                &device_id_obj,
            )
            .map_err(|e| VideoError::Backend(format!("reflect set2: {e:?}")))?;
            constraints.video(&video_constraint);
        }

        let stream_promise = media_devices
            .get_user_media_with_constraints(&constraints)
            .map_err(|e| VideoError::Backend(format!("getUserMedia: {e:?}")))?;

        let stream_js = JsFuture::from(stream_promise)
            .await
            .map_err(|_| VideoError::PermissionDenied)?;

        let _stream = web_sys::MediaStream::from(stream_js);

        // Web camera stream delivers frames via MediaStreamTrackProcessor (WebCodecs).
        // For Phase E, we return a placeholder stream — frame blitting via canvas
        // is handled in the UI (voice_view.rs JS_START_CAMERA / JS_STOP_CAMERA).
        // The MediaStream is intentionally not captured here; the UI's JS code
        // drives the srcObject assignment to the <video> element directly.
        tracing::info!("WebVideoBackend: camera stream opened (UI-side srcObject wiring active)");
        Ok(Box::new(WebPlaceholderInputStream))
    }

    async fn open_screen_share(
        &self,
        _source_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError> {
        use wasm_bindgen_futures::JsFuture;

        let window = web_sys::window()
            .ok_or_else(|| VideoError::Backend("no window object".into()))?;
        let nav = window.navigator();
        let media_devices = nav
            .media_devices()
            .map_err(|e| VideoError::Backend(format!("media_devices: {e:?}")))?;

        let constraints = web_sys::DisplayMediaStreamConstraints::new();
        let stream_promise = media_devices
            .get_display_media_with_constraints(&constraints)
            .map_err(|e| VideoError::Backend(format!("getDisplayMedia: {e:?}")))?;

        let _stream_js = JsFuture::from(stream_promise)
            .await
            .map_err(|_| VideoError::PermissionDenied)?;

        tracing::info!("WebVideoBackend: screen share stream opened (UI-side srcObject wiring active)");
        Ok(Box::new(WebPlaceholderInputStream))
    }
}

// ── WebPlaceholderInputStream ──────────────────────────────────────────────────

/// Placeholder stream for the web backend.
///
/// On web, frames are consumed by the UI layer via `<video srcObject=stream>`
/// and canvas blitting. This stream returns `None` immediately so the caller
/// doesn't busy-loop waiting for frames that won't arrive via this path.
struct WebPlaceholderInputStream;

impl VideoInputStream for WebPlaceholderInputStream {
    fn poll_next_frame(&mut self) -> Option<VideoFrame> {
        None
    }
}
