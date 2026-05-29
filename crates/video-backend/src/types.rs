//! Shared plain-data types: [`VideoDevice`], [`ScreenSource`], [`VideoFrame`],
//! and [`VideoPixelFormat`].

/// Pixel format of a [`VideoFrame`].
///
/// BGRA is the preferred format for this codebase: it is the native output
/// of most OS capture APIs on Linux (V4L2 / PipeWire) and macOS
/// (AVFoundation BGRA mode), and can be uploaded to a `<canvas>` element
/// on web via `ImageData` (which requires RGBA — a single channel-swap).
///
/// When real H.264 encoding lands (Phase E.5), the encoder will convert to
/// YUV420 internally; callers should not pre-convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoPixelFormat {
    /// Blue–Green–Red–Alpha, 4 bytes per pixel.
    Bgra,
    /// Red–Green–Blue–Alpha, 4 bytes per pixel.
    Rgba,
    /// YUV 4:2:0 planar (Y then U then V). Used by H.264 / VP8 encoders.
    Yuv420p,
}

/// A single decoded video frame.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Pixel format of `data`.
    pub format: VideoPixelFormat,
    /// Raw pixel bytes. Length = `width * height * bytes_per_pixel(format)`.
    pub data: Vec<u8>,
    /// Capture timestamp in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
}

impl VideoFrame {
    /// Expected byte length for a frame with this format and dimensions.
    ///
    /// `u32 → usize` casts are safe: `usize` is at least 32 bits on every
    /// target this codebase supports, so no truncation can occur.
    #[must_use]
    #[allow(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "u32 → usize widening; usize ≥ 32 bits on all supported targets"
    )]
    pub const fn expected_len(width: u32, height: u32, format: VideoPixelFormat) -> usize {
        let bpp: usize = match format {
            VideoPixelFormat::Bgra | VideoPixelFormat::Rgba => 4,
            VideoPixelFormat::Yuv420p => 3, // 1.5 bytes/pixel but we use 3/2 → usize math
        };
        match format {
            VideoPixelFormat::Bgra | VideoPixelFormat::Rgba => {
                (width as usize).saturating_mul(height as usize).saturating_mul(bpp)
            }
            VideoPixelFormat::Yuv420p => {
                // Y plane: w*h, U plane: w/2*h/2, V plane: w/2*h/2
                (width as usize)
                    .saturating_mul(height as usize)
                    .saturating_mul(3)
                    .div_ceil(2)
            }
        }
    }
}

/// A camera or capture card visible to the OS.
///
/// `id` is stable across enumerations and used as a `poly_kv` key for
/// "remember last camera" (Phase J / E follow-up).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VideoDevice {
    /// Stable, platform-assigned device identifier.
    pub id: String,
    /// Human-readable device label (e.g. "Built-in Camera", "Logitech C920").
    pub label: String,
    /// Whether this is the OS-default camera.
    pub is_default: bool,
}

impl VideoDevice {
    /// Construct a new `VideoDevice` with `is_default = false`.
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_default: false,
        }
    }

    /// Construct a `VideoDevice` marked as the OS default.
    #[must_use]
    pub fn new_default(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_default: true,
        }
    }
}

/// A screen or window available for capture via `getDisplayMedia` / scap.
///
/// `id` is platform-specific: on Linux it may be a Wayland surface ID or
/// X11 window ID; on macOS a CGWindowID; on Windows an HWND. On Web it is
/// the opaque string returned by `getDisplayMedia`'s `id` field (not stable
/// across sessions — do NOT persist this as a KV key).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScreenSource {
    /// Platform-assigned source identifier.
    pub id: String,
    /// Human-readable source label (e.g. "Entire Screen", "Firefox — GitHub").
    pub label: String,
    /// Whether this source represents the entire screen vs a single window.
    pub is_screen: bool,
}

impl ScreenSource {
    /// Construct a full-screen source.
    #[must_use]
    pub fn screen(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_screen: true,
        }
    }

    /// Construct a single-window source.
    #[must_use]
    pub fn window(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_screen: false,
        }
    }
}
