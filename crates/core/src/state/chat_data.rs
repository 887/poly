//! Display helpers and voice media settings for the chat UI.
//!
//! This module retains shared utility functions after the `ChatData` god-struct
//! was deleted in Phase G.6g of plan-solid-refactor-survey.md. The three
//! sub-signal types that replaced it are:
//! - `ChatLists` — servers, channels, dm_channels, groups, friends, notifications + by-id shadows
//! - `ChatViewState` — messages, members, current_server/channel, typing_users, loading, etc.
//! - `AccountSessions` — account_sessions, favorited_server_ids, account_order, content_policies, blocked_users
//!
// DECISION(V-4): VoiceMediaSettings is defined here and re-exported via VoiceState
// (phase-G.2 of plan-solid-refactor-survey.md). Persistence TBD.

use poly_client::*;

/// Runtime voice & audio settings (device selection, noise cancellation).
///
/// Held in ChatData so all voice UI components can read/write without
/// plumbing extra props. Reset on app restart (no persistence yet).
// DECISION(V-noise): noise_cancel_enabled defaults to true — AI noise reduction is on by default.
#[derive(Debug, Clone)]
pub struct VoiceMediaSettings {
    /// Whether RNNoise-based noise cancellation is enabled.
    ///
    /// When true, the mic audio pipeline routes through `nnnoiseless`
    /// (pure-Rust RNNoise port) before the Opus encode step.
    ///
    /// # Integration (B.8)
    ///
    /// Wired in `clients/stoat/src/voice_wasm_audio_capture.rs` via an
    /// `Arc<AtomicBool>` stored in `StoatClient::voice_noise_cancel`.
    /// The filter is applied inline in the `spawn_local` audio-capture task:
    /// mono f32 → scale to i16 range → `nnnoiseless::DenoiseState` → scale back
    /// → `float32_to_i16` → 960-sample i16 Opus frame.
    ///
    /// The UI toggle writes to this field; `StoatClient::set_noise_cancel(bool)`
    /// propagates changes to the `Arc<AtomicBool>` so the running capture loop
    /// picks them up on the very next 480-sample chunk.
    ///
    /// UI→backend wiring (via `use_reactive_effect`) is tracked at
    /// `crates/core/src/ui/account/settings/voice_settings.rs`.
    ///
    /// Defaults to `true` — noise cancellation is on by default.
    pub noise_cancel_enabled: bool,
    /// Selected microphone input device ID (`None` = system default).
    pub mic_device_id: Option<String>,
    /// Selected speaker / output device ID (`None` = system default).
    pub speaker_device_id: Option<String>,
}

impl Default for VoiceMediaSettings {
    fn default() -> Self {
        Self {
            noise_cancel_enabled: true,
            mic_device_id: None,
            speaker_device_id: None,
        }
    }
}

/// Format a file size in human-readable form.
#[must_use]
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    // lint-allow-unused: u64→f64 lossy is acceptable for human-readable size display.
    #[allow(clippy::cast_precision_loss, clippy::as_conversions)]
    let kb = bytes as f64 / 1_024.0_f64;
    if kb < 1_024.0_f64 {
        return format!("{kb:.1} KB");
    }
    let mb = kb / 1_024.0_f64;
    if mb < 1_024.0_f64 {
        return format!("{mb:.1} MB");
    }
    let gb = mb / 1_024.0_f64;
    format!("{gb:.2} GB")
}

/// Generic fallback badge shown as a backend source indicator.
///
/// Pre-WP-7 this function was a `match backend.as_str()` slug ladder. Per
/// D27 (plan `plan-client-ui-surface.md`), backend icons are plugin-declared
/// — the host no longer hard-codes them. Until every caller migrates to the
/// plugin's declared icon field, this returns a single generic placeholder
/// for all backends.
///
/// DECISION(D27): do not re-introduce slug→emoji mapping in this file —
/// it belongs in the plugin's declaration.
#[must_use]
pub fn backend_badge(_backend: &BackendType) -> &'static str {
    "⬜"
}

/// Get a deterministic color for a user ID (for avatar and username coloring).
///
/// Returns a CSS color string.
#[must_use]
pub fn user_color(user_id: &str) -> &'static str {
    let hash: u32 = user_id.bytes().fold(0u32, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(u32::from(b))
    });
    let colors = [
        "#60a5fa", // blue
        "#f87171", // red
        "#4ade80", // green
        "#fbbf24", // amber
        "#a78bfa", // purple
        "#fb923c", // orange
        "#2dd4bf", // teal
        "#f472b6", // pink
    ];
    // lint-allow-unused: hash is u32, usize is at least 32 bits; modulo by a
    // non-zero const len is safe (colors has 8 entries, always nonzero).
    #[allow(clippy::as_conversions, clippy::arithmetic_side_effects)]
    let idx = (hash as usize) % colors.len();
    colors.get(idx).copied().unwrap_or("#60a5fa")
}
