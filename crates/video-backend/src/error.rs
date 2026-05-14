//! Error types for `poly-video-backend`.

/// Errors returned by [`crate::VideoBackend`] and its stream types.
#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    /// The requested device ID does not exist or is not available.
    ///
    /// This typically means the device was unplugged between enumeration
    /// and open, or the caller passed a stale ID from a previous session.
    #[error("video device not found: {0}")]
    DeviceNotFound(String),

    /// A previously-opened stream's device was physically removed.
    #[error("video device lost")]
    DeviceLost,

    /// The requested pixel format or resolution is not supported by the device.
    #[error("unsupported video format: {0}")]
    UnsupportedFormat(String),

    /// Platform / OS video subsystem error (v4l2, AVFoundation, MSMF,
    /// WebRTC, getUserMedia, getDisplayMedia, …).
    #[error("video backend error: {0}")]
    Backend(String),

    /// The caller requested an operation that is not implemented on this
    /// platform or feature combination.
    ///
    /// Used when real capture impls are gated behind `native`/`web` features
    /// that are not yet enabled. See Phase E deferral note.
    #[error("not supported: {0}")]
    NotSupported(String),

    /// Camera / screen capture permission was denied by the user or OS policy.
    #[error("video capture permission denied")]
    PermissionDenied,
}
