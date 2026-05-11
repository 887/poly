//! `poly_kv` key constants for audio device persistence (Phase A.6).
//!
//! These keys are written and read by the voice channel connect helpers in
//! `clients/discord/src/voice/` and `clients/stoat/src/voice/` (Phases B/F).
//! They are defined here so that both voice backends and the device-picker UI
//! (Phase J) use the same stable key format.
//!
//! # Key format
//!
//! ```text
//! voice.last_input_device.<account_id>   → device ID string
//! voice.last_output_device.<account_id>  → device ID string
//! ```
//!
//! Keys are scoped by `account_id` so switching between accounts restores
//! the preferred device for each one independently.

/// Build the `poly_kv` key for the last-used input device of `account_id`.
///
/// Usage (call site, phases B/F/D):
/// ```rust
/// use poly_audio_backend::kv_keys::last_input_device_key;
/// let key = last_input_device_key("acct-discord-123");
/// // store: kv.set(&key, device_id).await?;
/// // restore: let device_id = kv.get(&key).await?;
/// ```
#[must_use]
pub fn last_input_device_key(account_id: &str) -> String {
    format!("voice.last_input_device.{account_id}")
}

/// Build the `poly_kv` key for the last-used output device of `account_id`.
#[must_use]
pub fn last_output_device_key(account_id: &str) -> String {
    format!("voice.last_output_device.{account_id}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn key_format_is_stable() {
        assert_eq!(
            last_input_device_key("discord-uid-42"),
            "voice.last_input_device.discord-uid-42"
        );
        assert_eq!(
            last_output_device_key("stoat-uid-7"),
            "voice.last_output_device.stoat-uid-7"
        );
    }
}
