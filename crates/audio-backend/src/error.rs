//! Error types for `poly-audio-backend`.

/// Errors returned by [`crate::AudioBackend`] and its stream types.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// The requested device ID does not exist or is not available.
    ///
    /// This typically means the device was unplugged between enumeration
    /// and open, or the caller passed a stale ID from a previous session.
    #[error("audio device not found: {0}")]
    DeviceNotFound(String),

    /// A previously-opened stream's device was physically removed.
    ///
    /// Callers should stop pushing/reading frames, notify the UI ("Headset
    /// disconnected"), and switch to the default device.
    #[error("audio device lost")]
    DeviceLost,

    /// The [`crate::AudioFormat`] requested is not supported by the device.
    ///
    /// For example, a device might not support 48 kHz stereo natively.
    /// The `CpalBackend` attempts automatic resampling; if it cannot, this
    /// error is returned.
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),

    /// Platform / OS audio subsystem error (ALSA, PulseAudio, CoreAudio,
    /// WASAPI, Web Audio API, getUserMedia, …).
    #[error("audio backend error: {0}")]
    Backend(String),

    /// The caller requested an operation that is not implemented on this
    /// platform or feature combination.
    ///
    /// For example, calling cpal stream switch on a WASM build.
    #[error("not supported: {0}")]
    NotSupported(String),

    /// getUserMedia / mic permission was denied by the user or browser policy.
    ///
    /// Only returned by the Web Audio backend.
    #[error("microphone permission denied")]
    PermissionDenied,
}
