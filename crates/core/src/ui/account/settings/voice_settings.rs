//! Voice & Audio settings for an account.
//!
//! Provides mic/speaker device pickers, noise cancellation toggle,
//! and a microphone test button. Moved here from the old `VoiceSettingsPopup`
//! in `voice_bar.rs` so audio settings are a proper account settings page.
//!
//! # Architecture
//! - `VoiceSettings` — thin wrapper that composes all settings sections
//! - `MicDevicePicker` — mic device selector (< 150 lines)
//! - `SpeakerDevicePicker` — speaker device selector (< 150 lines)
//! - `NoiseCancelToggle` — noise cancellation toggle (< 150 lines)
//! - `TestMicButton` — mic test button (< 150 lines)
//!
//! # 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**.

use crate::state::BatchedSignal;
use crate::i18n::t;
use crate::state::VoiceState;
use dioxus::prelude::*;
use poly_ui_macros::ui_action;

/// Typed actions for the Voice & Audio settings panel.
pub enum VoiceSettingsAction {
    SetMicDevice(Option<String>),
    SetSpeakerDevice(Option<String>),
    ToggleNoiseCancel(bool),
    TestMic,
}

impl crate::ui::actions::UiAction for VoiceSettingsAction {
    fn apply(self, _cx: crate::ui::actions::ActionCx<'_>) {
        match self {
            Self::SetMicDevice(_) => todo!("phase-E: update mic device"),
            Self::SetSpeakerDevice(_) => todo!("phase-E: update speaker device"),
            Self::ToggleNoiseCancel(_) => todo!("phase-E: toggle noise cancellation"),
            Self::TestMic => todo!("phase-E: run mic test"),
        }
    }
}

// ── JS for device enumeration and mic test ────────────────────────────────────

const JS_ENUMERATE_DEVICES: &str = r#"
(async () => {
    try { await navigator.mediaDevices.getUserMedia({audio: true}); } catch(_) {}
    const devices = await navigator.mediaDevices.enumerateDevices();
    const inputs = devices
        .filter(d => d.kind === 'audioinput')
        .map(d => ({ id: d.deviceId, label: d.label || 'Microphone' }));
    const outputs = devices
        .filter(d => d.kind === 'audiooutput')
        .map(d => ({ id: d.deviceId, label: d.label || 'Speaker' }));
    await dioxus.send(JSON.stringify({ inputs, outputs }));
})();
"#;

const JS_TEST_MIC: &str = r#"
(async () => {
    try {
        const stream = await navigator.mediaDevices.getUserMedia({audio: true});
        const ctx = new AudioContext();
        const src = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        src.connect(analyser);
        setTimeout(() => { stream.getTracks().forEach(t => t.stop()); ctx.close(); }, 3000);
        await dioxus.send("ok");
    } catch(e) {
        await dioxus.send("error");
    }
})();
"#;

/// Voice & Audio settings section within the account settings page.
///
/// Composed of: mic picker, speaker picker, noise cancel toggle, test mic button.
// DECISION(V-4): Audio settings live in AccountSettingsPage, not a popup.
#[rustfmt::skip]
#[ui_action(VoiceSettingsAction)]
#[context_menu(none)]
#[component]
pub fn VoiceSettings() -> Element {
    let mut mic_devices = use_signal::<Vec<(String, String)>>(Vec::new);
    let mut spk_devices = use_signal::<Vec<(String, String)>>(Vec::new);

    // Enumerate devices on mount
    use_effect(move || {
        spawn(async move {
            let mut eval = document::eval(JS_ENUMERATE_DEVICES);
            if let Ok(json) = eval.recv::<serde_json::Value>().await {
                if let Some(arr) = json.get("inputs").and_then(|v| v.as_array()) {
                    mic_devices.set(
                        arr.iter()
                            .filter_map(|d| {
                                let id = d.get("id")?.as_str()?.to_string();
                                let label = d.get("label")?.as_str()?.to_string();
                                Some((id, label))
                            })
                            .collect(),
                    );
                }
                if let Some(arr) = json.get("outputs").and_then(|v| v.as_array()) {
                    spk_devices.set(
                        arr.iter()
                            .filter_map(|d| {
                                let id = d.get("id")?.as_str()?.to_string();
                                let label = d.get("label")?.as_str()?.to_string();
                                Some((id, label))
                            })
                            .collect(),
                    );
                }
            }
        });
    });

    rsx! {
        div { class: "voice-settings-page",
            MicDevicePicker { devices: mic_devices.read().clone() }
            SpeakerDevicePicker { devices: spk_devices.read().clone() }
            NoiseCancelToggle {}
            TestMicButton {}
        }
    }
}

/// Microphone device selector.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn MicDevicePicker(devices: Vec<(String, String)>) -> Element {
    let voice_state: BatchedSignal<VoiceState> = use_context();

    let mic_id = voice_state
        .read()
        .voice_media_settings
        .mic_device_id
        .clone()
        .unwrap_or_default();

    rsx! {
        div { class: "voice-settings-section",
            label { class: "voice-settings-label", "{t(\"voice-mic-device\")}" }
            select {
                class: "voice-settings-select",
                value: "{mic_id}",
                onchange: move |e: Event<FormData>| {
                    let val = e.value();
                    let id = if val.is_empty() { None } else { Some(val) };
                    voice_state.batch(move |v| v.voice_media_settings.mic_device_id = id);
                },
                option { value: "", "{t(\"voice-default-device\")}" }
                for (id , label) in devices.iter() {
                    option { value: "{id}", "{label}" }
                }
            }
        }
    }
}

/// Speaker device selector.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn SpeakerDevicePicker(devices: Vec<(String, String)>) -> Element {
    let voice_state: BatchedSignal<VoiceState> = use_context();

    let spk_id = voice_state
        .read()
        .voice_media_settings
        .speaker_device_id
        .clone()
        .unwrap_or_default();

    rsx! {
        div { class: "voice-settings-section",
            label { class: "voice-settings-label", "{t(\"voice-speaker-device\")}" }
            select {
                class: "voice-settings-select",
                value: "{spk_id}",
                onchange: move |e: Event<FormData>| {
                    let val = e.value();
                    let id = if val.is_empty() { None } else { Some(val) };
                    voice_state.batch(move |v| v.voice_media_settings.speaker_device_id = id);
                },
                option { value: "", "{t(\"voice-default-device\")}" }
                for (id , label) in devices.iter() {
                    option { value: "{id}", "{label}" }
                }
            }
        }
    }
}

/// Noise cancellation toggle.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn NoiseCancelToggle() -> Element {
    let voice_state: BatchedSignal<VoiceState> = use_context();

    let noise_cancel = voice_state.read().voice_media_settings.noise_cancel_enabled;

    rsx! {
        div { class: "voice-settings-section",
            div { class: "voice-settings-row",
                div {
                    label { class: "voice-settings-label", "{t(\"voice-noise-cancel\")}" }
                    p { class: "voice-settings-desc", "{t(\"voice-noise-cancel-desc\")}" }
                }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: noise_cancel,
                        onchange: move |_| {
                            voice_state.batch(|v| {
                                v.voice_media_settings.noise_cancel_enabled =
                                    !v.voice_media_settings.noise_cancel_enabled;
                            });
                        },
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}

/// Test microphone button.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn TestMicButton() -> Element {
    let mut test_running = use_signal(|| false);

    rsx! {
        div { class: "voice-settings-section",
            button {
                class: format_args!(
                    "{} {}",
                    "btn btn-secondary voice-settings-test-btn",
                    if *test_running.read() { "active" } else { "" },
                ),
                disabled: *test_running.read(),
                onclick: move |_| {
                    test_running.set(true);
                    spawn(async move {
                        let mut eval = document::eval(JS_TEST_MIC);
                        let _ = eval.recv::<String>().await;
                        test_running.set(false);
                    });
                },
                if *test_running.read() {
                    "{t(\"voice-testing-mic\")}"
                } else {
                    "{t(\"voice-test-mic\")}"
                }
            }
        }
    }
}
