//! Native (non-wasm) video backend: nokhwa camera capture + host-bridge H.264.
//!
//! # Availability
//!
//! Compiled only when **both**:
//! - `target_arch != "wasm32"` (native desktop / server)
//! - feature `native` is enabled
//!
//! # Camera capture
//!
//! Uses `nokhwa` (V4L2 on Linux, AVFoundation on macOS, MSMF on Windows).
//! Frames are emitted as BGRA (4 bytes/pixel) to match the rest of the
//! `VideoInputStream` contract.
//!
//! # Screen capture
//!
//! **Currently stubbed.** `scap 0.1.0-beta.1` depends on `libspa-sys 0.8.0`
//! which has a field-name mismatch with PipeWire ≥ 1.0 on Linux.
//! Screen capture returns `VideoError::NotImplemented` until `libspa-sys 0.9+` lands.
//!
//! # H.264 encode/decode
//!
//! Routed via `poly_host_bridge::video_client::VideoBridgeClient`.
//! The codec (openh264-rs) lives in the host-bridge server — this crate never
//! links `openh264` directly.

#![cfg(all(not(target_arch = "wasm32"), feature = "native"))]

use std::sync::{mpsc, Arc};

use nokhwa::{
    pixel_format::RgbAFormat,
    query,
    utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};

use crate::{
    error::VideoError,
    types::{ScreenSource, VideoDevice, VideoFrame, VideoPixelFormat},
    VideoBackend, VideoInputStream,
};

// ── NativeVideoBackend ─────────────────────────────────────────────────────────

/// Native (non-wasm) video backend.
///
/// Cheaply cloneable via `Arc`. Holds no OS handles at construction time.
#[derive(Clone, Debug, Default)]
pub struct NativeVideoBackend {
    _inner: Arc<()>,
}

impl NativeVideoBackend {
    /// Construct a new `NativeVideoBackend`. No I/O at construction time.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl VideoBackend for NativeVideoBackend {
    async fn enumerate_cameras(&self) -> Result<Vec<VideoDevice>, VideoError> {
        let devices = tokio::task::spawn_blocking(|| {
            query(ApiBackend::Auto)
                .map_err(|e| VideoError::Backend(format!("nokhwa enumerate: {e}")))
        })
        .await
        .map_err(|e| VideoError::Backend(format!("spawn_blocking panic: {e}")))?
        ?;

        let cameras: Vec<VideoDevice> = devices
            .into_iter()
            .enumerate()
            .map(|(i, info)| VideoDevice {
                id: info.index().to_string(),
                label: info.human_name().to_string(),
                is_default: i == 0,
            })
            .collect();

        Ok(cameras)
    }

    async fn enumerate_screens(&self) -> Result<Vec<ScreenSource>, VideoError> {
        tracing::warn!(
            "enumerate_screens: screen capture stubbed (libspa-sys/PipeWire 1.x compat). \
             Returning empty list."
        );
        Ok(vec![])
    }

    async fn open_camera(
        &self,
        device_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError> {
        let index: CameraIndex = if device_id.is_empty() {
            CameraIndex::Index(0)
        } else {
            device_id
                .parse::<u32>()
                .map(CameraIndex::Index)
                .unwrap_or_else(|_| CameraIndex::String(device_id.to_string()))
        };

        let (tx, rx) = mpsc::sync_channel::<Result<VideoFrame, VideoError>>(8);

        let index_clone = index.clone();
        tokio::task::spawn_blocking(move || {
            let format = RequestedFormat::new::<RgbAFormat>(
                RequestedFormatType::AbsoluteHighestFrameRate,
            );
            let mut cam = match Camera::new(index_clone.clone(), format) {
                Ok(c) => c,
                Err(e) => {
                    let err = match e {
                        nokhwa::NokhwaError::OpenDeviceError(_, _) => {
                            VideoError::DeviceNotFound(index_clone.to_string())
                        }
                        other => VideoError::Backend(format!("nokhwa Camera::new: {other}")),
                    };
                    let _ = tx.send(Err(err));
                    return;
                }
            };
            if let Err(e) = cam.open_stream() {
                let _ = tx.send(Err(VideoError::Backend(format!(
                    "nokhwa open_stream: {e}"
                ))));
                return;
            }
            loop {
                let frame = match cam.frame() {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = tx.send(Err(VideoError::Backend(format!(
                            "nokhwa frame: {e}"
                        ))));
                        break;
                    }
                };
                // Convert RGBA (nokhwa RgbAFormat) to BGRA.
                let mut data = frame.buffer().to_vec();
                for chunk in data.chunks_exact_mut(4) {
                    chunk.swap(0, 2); // R ↔ B
                }
                let res = frame.resolution();
                let timestamp_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let poly_frame = VideoFrame {
                    width: res.width(),
                    height: res.height(),
                    format: VideoPixelFormat::Bgra,
                    data,
                    timestamp_ms,
                };
                if tx.send(Ok(poly_frame)).is_err() {
                    break;
                }
            }
        });

        Ok(Box::new(NativeCameraInputStream { rx }))
    }

    async fn open_screen_share(
        &self,
        _source_id: &str,
    ) -> Result<Box<dyn VideoInputStream>, VideoError> {
        Err(VideoError::NotImplemented(
            "screen capture requires scap (disabled: libspa-sys/PipeWire 1.x compat)".into(),
        ))
    }
}

// ── NativeCameraInputStream ────────────────────────────────────────────────────

/// A `VideoInputStream` backed by a nokhwa camera on a dedicated blocking thread.
pub struct NativeCameraInputStream {
    rx: std::sync::mpsc::Receiver<Result<VideoFrame, VideoError>>,
}

impl VideoInputStream for NativeCameraInputStream {
    fn poll_next_frame(&mut self) -> Option<VideoFrame> {
        match self.rx.recv() {
            Ok(Ok(frame)) => Some(frame),
            Ok(Err(e)) => {
                tracing::warn!("NativeCameraInputStream: capture error: {e}");
                None
            }
            Err(_) => None,
        }
    }
}

// ── NativeVideoEncoder ─────────────────────────────────────────────────────────

/// H.264 encoder routing frames to `/host/video/encode_h264` via host-bridge.
pub struct NativeVideoEncoder {
    client: poly_host_bridge::video_client::VideoBridgeClient,
    session_id: String,
}

impl NativeVideoEncoder {
    /// Construct with explicit `base_url` (e.g. `"http://127.0.0.1:9333"`).
    #[must_use]
    pub fn new(session_id: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: poly_host_bridge::video_client::VideoBridgeClient::new(base_url),
            session_id: session_id.into(),
        }
    }

    /// Construct using the default loopback bridge port.
    #[must_use]
    pub fn default_local(session_id: impl Into<String>) -> Self {
        Self {
            client: poly_host_bridge::video_client::VideoBridgeClient::default_local(),
            session_id: session_id.into(),
        }
    }

    /// Encode one BGRA frame. Returns the encode response with NAL units.
    pub async fn encode_frame(
        &self,
        frame: &VideoFrame,
        force_keyframe: bool,
    ) -> Result<poly_host_bridge::video_client::EncodeH264Response, VideoError> {
        if frame.format != VideoPixelFormat::Bgra {
            return Err(VideoError::UnsupportedFormat(format!(
                "NativeVideoEncoder requires BGRA input, got {:?}",
                frame.format
            )));
        }

        use base64::Engine as _;
        let data_b64 = base64::engine::general_purpose::STANDARD.encode(&frame.data);

        let req = poly_host_bridge::video_client::EncodeH264Request {
            width: frame.width,
            height: frame.height,
            format: "bgra".into(),
            data_b64,
            force_keyframe,
            session_id: self.session_id.clone(),
            target_bps: None,
        };

        self.client
            .encode(req)
            .await
            .map_err(|e| VideoError::Backend(format!("encode_h264: {e}")))
    }

    /// Release the encoder session on the host-bridge server.
    pub async fn close(&self) -> Result<(), VideoError> {
        self.client
            .close_session(&self.session_id)
            .await
            .map_err(|e| VideoError::Backend(format!("close_session: {e}")))
    }
}

// ── NativeVideoDecoder ─────────────────────────────────────────────────────────

/// H.264 decoder routing NAL units to `/host/video/decode_h264` via host-bridge.
pub struct NativeVideoDecoder {
    client: poly_host_bridge::video_client::VideoBridgeClient,
    session_id: String,
}

impl NativeVideoDecoder {
    /// Construct using the default loopback bridge port.
    #[must_use]
    pub fn default_local(session_id: impl Into<String>) -> Self {
        Self {
            client: poly_host_bridge::video_client::VideoBridgeClient::default_local(),
            session_id: session_id.into(),
        }
    }

    /// Construct with explicit `base_url`.
    #[must_use]
    pub fn new(session_id: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: poly_host_bridge::video_client::VideoBridgeClient::new(base_url),
            session_id: session_id.into(),
        }
    }

    /// Decode H.264 NAL units (base64-encoded, without Annex-B start codes).
    pub async fn decode(
        &self,
        nal_units_b64: Vec<String>,
    ) -> Result<poly_host_bridge::video_client::DecodeH264Response, VideoError> {
        let req = poly_host_bridge::video_client::DecodeH264Request {
            nal_units_b64,
            session_id: self.session_id.clone(),
        };

        self.client
            .decode(req)
            .await
            .map_err(|e| VideoError::Backend(format!("decode_h264: {e}")))
    }

    /// Release the decoder session on the host-bridge server.
    pub async fn close(&self) -> Result<(), VideoError> {
        self.client
            .close_session(&self.session_id)
            .await
            .map_err(|e| VideoError::Backend(format!("close_session: {e}")))
    }
}
