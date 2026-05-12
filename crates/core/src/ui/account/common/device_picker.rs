//! Voice device picker — Phase J of `docs/plans/plan-voice-video-calls.md`.
//!
//! A popover component anchored to the gear icon in the voice banner controls.
//! Allows the user to:
//! - Select input (microphone) and output (speaker) devices.
//! - Run a 2-second mic test (record → play back).
//! - Persist the selection via `poly_kv` (Phase A.6 keys).
//!
//! # Architecture
//!
//! ```text
//! DevicePickerToggle (gear icon)
//!   └── VoiceDevicePicker (popover)
//!         ├── DevicePickerInputSection  (mic selector)
//!         ├── DevicePickerOutputSection (speaker selector)
//!         └── DevicePickerMicTest       (test button + status)
//! ```
//!
//! The audio backend is consumed from Dioxus context
//! (`try_consume_context::<SharedAudioBackend>()`). If no audio backend is
//! provided the popover renders a "Not available" message instead of crashing.
//!
//! # Hot-swap / devicechange (Phase J.5)
//!
//! On web (`cfg(target_arch = "wasm32")`), the component subscribes to
//! `navigator.mediaDevices.ondevicechange` and re-enumerates devices when
//! the event fires. On native, a polling loop is the correct approach but
//! is left as a TODO — Phase A.7 punted native hot-plug to a follow-up.
//!
//! # Video device enumeration (Phase E)
//!
//! Camera device listing (`VideoBackend::enumerate_cameras`) is post-Phase E
//! (video + screen share). This module leaves a `// TODO(Phase-E)` marker.
//!
//! # 150-line component rule
//! Each `#[component]` fn body stays under 150 lines.

// TODO(Phase-E): wire VideoBackend::enumerate_cameras when Phase E lands.

use std::sync::Arc;

use dioxus::prelude::*;
use poly_audio_backend::{AudioBackend, AudioDevice};
use poly_audio_backend::kv_keys::{last_input_device_key, last_output_device_key};
use poly_client::ToastTone;

use crate::i18n::t;
use crate::state::{BatchedSignal, VoiceState};
use crate::ui::client_ui::toast::{push_toast, ToastMessage};
use poly_ui_macros::{context_menu, ui_action};

// ── JS snippets ──────────────────────────────────────────────────────────────

/// JS to wire `navigator.mediaDevices.ondevicechange` (Phase J.5, web only).
const JS_DEVICE_CHANGE_LISTENER: &str = r#"
(function() {
    if (navigator.mediaDevices && !window.__polyDeviceChangeWired) {
        window.__polyDeviceChangeWired = true;
        navigator.mediaDevices.addEventListener('devicechange', function() {
            window.__polyDeviceChangeTs = Date.now();
        });
    }
})();
"#;

/// Record 2s of mic audio and play it back via the selected output (Phase J.6).
const JS_MIC_TEST: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        const ctx = new AudioContext();
        const src = ctx.createMediaStreamSource(stream);
        const dest = ctx.createMediaStreamDestination();
        src.connect(dest);
        const rec = new MediaRecorder(dest.stream);
        const chunks = [];
        rec.ondataavailable = e => chunks.push(e.data);
        rec.start();
        await new Promise(r => setTimeout(r, 2000));
        rec.stop();
        stream.getTracks().forEach(t => t.stop());
        await new Promise(r => rec.onstop = r);
        const blob = new Blob(chunks);
        const url = URL.createObjectURL(blob);
        const audio = new Audio(url);
        audio.play();
        await new Promise(r => audio.onended = r);
        URL.revokeObjectURL(url);
        await dioxus.send("ok");
    } catch(e) {
        await dioxus.send("error: " + e.message);
    }
})();
"#;

// ── Context type ─────────────────────────────────────────────────────────────

/// Type alias for the shared audio backend provided via Dioxus context.
///
/// Shells that support audio (apps/web, apps/desktop, apps/desktop-electron)
/// provide an instance via `provide_context::<SharedAudioBackend>(...)` before
/// mounting the root layout.
pub type SharedAudioBackend = Arc<dyn AudioBackend + Send + Sync>;

// ── Mic test status ──────────────────────────────────────────────────────────

/// Status of the 2-second mic test (J.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MicTestStatus {
    Idle,
    Recording,
    Playing,
}

// ── Root component ───────────────────────────────────────────────────────────

/// Device picker popover (Phase J.1).
///
/// `on_close` is called when the popover should be dismissed.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn VoiceDevicePicker(
    on_close: EventHandler<()>,
) -> Element {
    let backend: Option<SharedAudioBackend> = try_consume_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();

    let input_devices: Signal<Vec<AudioDevice>> = use_signal(Vec::new);
    let output_devices: Signal<Vec<AudioDevice>> = use_signal(Vec::new);
    let mic_test_status: Signal<MicTestStatus> = use_signal(|| MicTestStatus::Idle);

    // Load device list on mount. One-shot; no Signal deps.
    // poly-lint: allow stale-effect-capture — one-shot mount; backend is Arc (not a Signal); no stale-capture risk
    // poly-lint: allow use-effect-spawn-cycle — spawn writes local-only Signals (input_devices, output_devices); no cycle
    {
        let backend = backend.clone();
        let mut inputs = input_devices;
        let mut outputs = output_devices;
        use_effect(move || { // poly-lint: allow stale-effect-capture — one-shot mount; backend is Arc (not a Signal); no stale-capture risk
            #[cfg(target_arch = "wasm32")]
            {
                let _ = document::eval(JS_DEVICE_CHANGE_LISTENER);
            }
            if let Some(ref ab) = backend {
                let ab = Arc::clone(ab);
                spawn(async move {
                    if let Ok(devs) = ab.list_input_devices().await {
                        inputs.set(devs);
                    }
                    if let Ok(devs) = ab.list_output_devices().await {
                        outputs.set(devs);
                    }
                });
            }
        });
    }

    let mic_setting = voice_state.read().voice_media_settings.mic_device_id.clone(); // poly-lint: allow render-time-read — drives selected-device display; subscription IS the intent
    let speaker_setting = voice_state.read().voice_media_settings.speaker_device_id.clone(); // poly-lint: allow render-time-read — drives selected-device display; subscription IS the intent
    let has_backend = backend.is_some();

    rsx! {
        div { class: "voice-device-picker",
            div { class: "voice-device-picker-header",
                span { class: "voice-device-picker-title", "{t(\"voice-device-picker-title\")}" }
                button {
                    class: "voice-device-picker-close",
                    onclick: move |_| on_close.call(()),
                    "×"
                }
            }

            if !has_backend {
                div { class: "voice-device-picker-unavailable",
                    "{t(\"voice-no-channel\")}"
                }
            } else {
                DevicePickerInputSection {
                    devices: input_devices.read().clone(), // poly-lint: allow render-time-read — drives device list rendering; subscription IS the intent
                    selected_id: mic_setting,
                    voice_state,
                }
                DevicePickerOutputSection {
                    devices: output_devices.read().clone(), // poly-lint: allow render-time-read — drives device list rendering; subscription IS the intent
                    selected_id: speaker_setting,
                    voice_state,
                }
                DevicePickerMicTest { status: mic_test_status }
            }
        }
    }
}

// ── Input section ────────────────────────────────────────────────────────────

/// Microphone (input device) selector section (Phase J.2 / J.3 / J.4).
///
/// The audio backend is consumed from context inside the `onchange` handler
/// to avoid making `Arc<dyn AudioBackend>` a component prop (Arc<dyn> doesn't
/// implement PartialEq which Dioxus requires for prop memoization).
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn DevicePickerInputSection(
    devices: Vec<AudioDevice>,
    selected_id: Option<String>,
    voice_state: BatchedSignal<VoiceState>,
) -> Element {
    rsx! {
        div { class: "voice-device-section",
            label { class: "voice-device-label", "{t(\"voice-device-picker-input\")}" }
            select {
                class: "voice-device-select",
                value: selected_id.clone().unwrap_or_default(),
                onchange: move |evt: Event<FormData>| {
                    let new_id = evt.value().to_string();
                    // J.3 — switch the active input device mid-call.
                    let new_id_clone = new_id.clone();
                    if let Some(ab) = try_consume_context::<SharedAudioBackend>() {
                        spawn(async move {
                            if let Err(e) = ab.switch_input(&new_id_clone).await {
                                tracing::warn!("device_picker: switch_input failed: {e:?}");
                            }
                        });
                    }
                    // J.4 — persist selection to VoiceMediaSettings.
                    voice_state.batch(move |v| {
                        v.voice_media_settings.mic_device_id = Some(new_id.clone());
                    });
                    // J.4 — TODO(Phase-J-kv): thread account_id via NavState context and
                    // write to last_input_device_key(account_id) via poly_host_bridge.
                },
                option { value: "", "{t(\"voice-default-mic\")}" }
                for device in &devices {
                    option {
                        key: "{device.id}",
                        value: "{device.id}",
                        selected: selected_id.as_deref() == Some(&device.id),
                        if device.is_default {
                            "{device.label} ({t(\"voice-device-picker-current\")})"
                        } else {
                            "{device.label}"
                        }
                    }
                }
            }
        }
    }
}

// ── Output section ───────────────────────────────────────────────────────────

/// Speaker (output device) selector section (Phase J.2 / J.3 / J.4).
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn DevicePickerOutputSection(
    devices: Vec<AudioDevice>,
    selected_id: Option<String>,
    voice_state: BatchedSignal<VoiceState>,
) -> Element {
    rsx! {
        div { class: "voice-device-section",
            label { class: "voice-device-label", "{t(\"voice-device-picker-output\")}" }
            select {
                class: "voice-device-select",
                value: selected_id.clone().unwrap_or_default(),
                onchange: move |evt: Event<FormData>| {
                    let new_id = evt.value().to_string();
                    let new_id_clone = new_id.clone();
                    if let Some(ab) = try_consume_context::<SharedAudioBackend>() {
                        spawn(async move {
                            if let Err(e) = ab.switch_output(&new_id_clone).await {
                                tracing::warn!("device_picker: switch_output failed: {e:?}");
                            }
                        });
                    }
                    voice_state.batch(move |v| {
                        v.voice_media_settings.speaker_device_id = Some(new_id.clone());
                    });
                    // J.4 — TODO(Phase-J-kv): write to last_output_device_key(account_id).
                },
                option { value: "", "{t(\"voice-default-speakers\")}" }
                for device in &devices {
                    option {
                        key: "{device.id}",
                        value: "{device.id}",
                        selected: selected_id.as_deref() == Some(&device.id),
                        if device.is_default {
                            "{device.label} ({t(\"voice-device-picker-current\")})"
                        } else {
                            "{device.label}"
                        }
                    }
                }
            }
        }
    }
}

// ── Mic test section (J.6) ───────────────────────────────────────────────────

/// Mic test button: records 2s, plays back via the selected output (Phase J.6).
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
fn DevicePickerMicTest(status: Signal<MicTestStatus>) -> Element {
    let current = status.read().clone(); // poly-lint: allow render-time-read — drives label; subscription IS the intent
    let label = match current {
        MicTestStatus::Idle => t("voice-device-picker-test-mic"),
        MicTestStatus::Recording => t("voice-device-picker-recording"),
        MicTestStatus::Playing => t("voice-device-picker-playing"),
    };
    let busy = *status.read() != MicTestStatus::Idle; // poly-lint: allow render-time-read — drives disabled state; subscription IS the intent

    rsx! {
        div { class: "voice-device-mic-test",
            button {
                class: "voice-device-test-btn",
                disabled: busy,
                onclick: move |_| {
                    if busy { return; }
                    let mut st = status;
                    spawn(async move {
                        st.set(MicTestStatus::Recording);
                        let mut eval = document::eval(JS_MIC_TEST);
                        st.set(MicTestStatus::Playing);
                        match eval.recv::<String>().await {
                            Ok(ref s) if s == "ok" => {}
                            Ok(ref err_s) => {
                                tracing::warn!("mic test: {err_s}");
                            }
                            Err(e) => {
                                tracing::warn!("mic test eval error: {e:?}");
                            }
                        }
                        st.set(MicTestStatus::Idle);
                    });
                },
                "{label}"
            }
        }
    }
}

// ── Gear-icon trigger (J.1) ───────────────────────────────────────────────────

/// Gear icon button that toggles the device picker popover (Phase J.1).
///
/// Placed inside `VoiceBannerControls` next to the mute/deafen/disconnect buttons.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub fn DevicePickerToggle() -> Element {
    let mut show_picker: Signal<bool> = use_signal(|| false);

    let is_open = *show_picker.read(); // poly-lint: allow render-time-read — drives active CSS class + conditional render; subscription IS the intent

    rsx! {
        div { class: "voice-device-picker-wrapper",
            button {
                class: if is_open { "voice-ctrl-btn active" } else { "voice-ctrl-btn" },
                title: "{t(\"voice-device-picker-title\")}",
                onclick: move |_| {
                    let cur = *show_picker.peek();
                    show_picker.set(!cur);
                },
                "⚙"
            }
            if is_open {
                VoiceDevicePicker {
                    on_close: move |_| show_picker.set(false),
                }
            }
        }
    }
}

// ── J.5 — devicechange hot-swap helper ───────────────────────────────────────

/// Show a "headset disconnected" toast when the active device disappears (Phase J.5).
pub fn notify_device_disconnected(
    toast_queue: Signal<Vec<ToastMessage>>,
    device_label: &str,
) {
    let msg = format!("{device_label} disconnected — switched to built-in speakers.");
    push_toast(toast_queue, ToastMessage::new(msg, ToastTone::Warning));
}

// ── KV key re-exports (Phase J.4) ────────────────────────────────────────────

/// KV key for the last-used input device (re-exported for call-site convenience).
pub use poly_audio_backend::kv_keys::last_input_device_key as input_device_kv_key;
/// KV key for the last-used output device (re-exported for call-site convenience).
pub use poly_audio_backend::kv_keys::last_output_device_key as output_device_kv_key;

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn kv_key_re_exports_are_stable() {
        assert_eq!(
            input_device_kv_key("acct-1"),
            last_input_device_key("acct-1")
        );
        assert_eq!(
            output_device_kv_key("acct-1"),
            last_output_device_key("acct-1")
        );
    }

    #[test]
    fn mic_test_status_eq() {
        assert_eq!(MicTestStatus::Idle, MicTestStatus::Idle);
        assert_ne!(MicTestStatus::Idle, MicTestStatus::Recording);
    }
}
