//! Shared plain-data types: [`AudioDevice`], [`AudioFormat`], [`SampleRate`].

/// Sample rate in Hz.
///
/// The strongly-typed newtype prevents confusing Hz with channel count or
/// frame size. The most important value for this codebase is [`SampleRate::KHZ_48`]
/// which is what Discord voice (Opus) requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleRate(pub u32);

impl SampleRate {
    /// 48,000 Hz — required by Discord voice (Opus) and the safe default
    /// for Stoat (Revolt) voice.
    pub const KHZ_48: Self = Self(48_000);

    /// 44,100 Hz — CD quality; supported by most consumer hardware but NOT
    /// the preferred rate for voice codecs. Use only when hardware resampling
    /// from 48 kHz is unavailable.
    pub const KHZ_44_1: Self = Self(44_100);

    /// Raw Hz value.
    #[must_use]
    pub fn hz(self) -> u32 {
        self.0
    }
}

impl Default for SampleRate {
    fn default() -> Self {
        Self::KHZ_48
    }
}

/// Number of audio channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channels {
    /// 1-channel (microphone, voice, mono Opus).
    Mono,
    /// 2-channel interleaved (left, right). Discord voice uses stereo Opus.
    Stereo,
}

impl Channels {
    /// Return the raw channel count.
    #[must_use]
    pub fn count(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// PCM audio format descriptor.
///
/// All streams in this crate use signed 16-bit PCM (`i16`).  The frame
/// length is determined by the backend implementation; for Discord voice
/// it is 20 ms (1920 samples at 48 kHz stereo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    /// Target sample rate. Default: [`SampleRate::KHZ_48`].
    pub sample_rate: SampleRate,
    /// Number of channels. Default: [`Channels::Stereo`] (Discord voice).
    pub channels: Channels,
}

impl AudioFormat {
    /// Discord voice: 48 kHz stereo Opus.
    pub const DISCORD_VOICE: Self = Self {
        sample_rate: SampleRate::KHZ_48,
        channels: Channels::Stereo,
    };

    /// Stoat voice: 48 kHz mono (safe default; actual Stoat protocol TBD
    /// in Phase F). If Stoat's Vortex SFU requires stereo, update this
    /// constant and bump the call sites.
    pub const STOAT_VOICE: Self = Self {
        sample_rate: SampleRate::KHZ_48,
        channels: Channels::Mono,
    };

    /// Compute the number of `i16` samples in one frame of `duration_ms` ms.
    ///
    /// For Discord 20 ms stereo at 48 kHz:
    /// `frame_samples(20)` → 1920.
    #[must_use]
    pub fn frame_samples(self, duration_ms: u32) -> usize {
        let samples_per_ms = self.sample_rate.0 / 1000;
        (samples_per_ms * duration_ms * u32::from(self.channels.count())) as usize
    }
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::DISCORD_VOICE
    }
}

/// Whether a device captures (input) or renders (output) audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioDeviceKind {
    /// Microphone, line-in, headset mic.
    Input,
    /// Speaker, headphone, line-out.
    Output,
}

/// A single audio device visible to the OS / browser.
///
/// `id` is stable across enumerations — it is used as a `poly_kv` key
/// for the "remember last device" feature (Phase A.6 / Phase J.4).
///
/// # ID stability contract
///
/// - **cpal backend**: uses `cpal::Device::name()` as the ID. This is
///   stable on most platforms (ALSA/WASAPI/CoreAudio persist the name
///   across reconnections for the same physical device). PulseAudio/PipeWire
///   may use numeric sink indices that shift on server restart — document
///   this limitation when Phase J ships the device picker.
///
/// - **Web Audio backend**: uses `MediaDeviceInfo.deviceId` which is a
///   stable opaque string scoped to the origin + user-permission state.
///   It persists until the user clears site data.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AudioDevice {
    /// Stable, platform-assigned device identifier.
    pub id: String,
    /// Human-readable device label (e.g. "Built-in Microphone", "USB Headset").
    pub label: String,
    /// Whether this is the OS-default device for its kind.
    pub is_default: bool,
    /// Whether this is an input or output device.
    pub kind: AudioDeviceKind,
}

impl AudioDevice {
    /// Construct a new `AudioDevice` with the default flag set to `false`.
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: AudioDeviceKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_default: false,
            kind,
        }
    }

    /// Construct a new `AudioDevice` marked as the OS default.
    #[must_use]
    pub fn new_default(
        id: impl Into<String>,
        label: impl Into<String>,
        kind: AudioDeviceKind,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            is_default: true,
            kind,
        }
    }
}
