//! Voice & Video settings — audio/video device pickers, VAD, noise suppression.
//!
//! Settings are persisted to storage via `VoiceSettings`.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use crate::storage::VoiceSettings;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Persist current voice settings to storage.
fn persist_voice_settings(settings: VoiceSettings) {
    spawn(async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Err(e) = storage.set_voice_settings(&settings).await
        {
            tracing::warn!("Failed to save voice settings: {e}");
        }
    });
}

fn save_voice(
    input_vol: Signal<u32>,
    output_vol: Signal<u32>,
    vad_mode: Signal<String>,
    noise_suppress: Signal<String>,
    echo_cancel: Signal<bool>,
) {
    persist_voice_settings(VoiceSettings {
        input_volume: *input_vol.read(),
        output_volume: *output_vol.read(),
        input_mode: vad_mode.read().clone(),
        noise_suppression: noise_suppress.read().clone(),
        echo_cancellation: *echo_cancel.read(),
    });
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn DeviceSelectRow(label_key: &'static str, option_key: &'static str) -> Element {
    rsx! {
        div { class: "voice-settings-row",
            label { class: "voice-settings-label", "{t(label_key)}" }
            select { class: "poly-select-native",
                option { value: "default", "{t(option_key)}" }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn MicTestRow(mic_testing: Signal<bool>) -> Element {
    rsx! {
        div { class: "voice-settings-row",
            button {
                class: if *mic_testing.read() { "mic-test-btn active" } else { "mic-test-btn" },
                onclick: move |_| mic_testing.toggle(),
                if *mic_testing.read() {
                    "{t(\"voice-mic-test-stop\")}"
                } else {
                    "{t(\"voice-mic-test\")}"
                }
            }
            if *mic_testing.read() {
                div { class: "mic-level-bar",
                    div { class: "mic-level-fill", style: "width: 40%;" }
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn VoiceModeRow(selected: String, on_change: EventHandler<String>) -> Element {
    rsx! {
        div { class: "voice-settings-row voice-mode-row",
            label { class: "voice-settings-label", "{t(\"voice-input-mode\")}" }
            div { class: "voice-mode-options",
                for (value , label_key) in [("vad", "voice-input-vad"), ("ptt", "voice-input-ptt")] {
                    label { class: "voice-mode-option",
                        input {
                            r#type: "radio",
                            name: "voice-mode",
                            value,
                            checked: selected == value,
                            onchange: {
                                let next = value.to_string();
                                move |_| on_change.call(next.clone())
                            },
                        }
                        "{t(label_key)}"
                    }
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn EchoCancellationRow(enabled: bool, on_change: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "voice-settings-row toggle-row",
            label { class: "voice-settings-label", "{t(\"voice-echo-cancel\")}" }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |e| on_change.call(e.checked()),
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

/// Voice & Video settings section.
///
/// Lets the user configure audio/video input/output devices, volume levels,
/// voice activity detection mode, noise suppression and echo cancellation.
/// Settings are loaded from and persisted to storage.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn VoiceVideoSettings() -> Element {
    let mut input_vol = use_signal(|| 80_u32);
    let mut output_vol = use_signal(|| 80_u32);
    let mut vad_mode = use_signal(|| String::from("vad"));
    let mut noise_suppress = use_signal(|| String::from("standard"));
    let mut echo_cancel = use_signal(|| true);
    let mic_testing = use_signal(|| false);

    // Load persisted settings on mount
    let _load = use_future(move || async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Ok(s) = storage.get_voice_settings().await
        {
            input_vol.set(s.input_volume);
            output_vol.set(s.output_volume);
            vad_mode.set(s.input_mode);
            noise_suppress.set(s.noise_suppression);
            echo_cancel.set(s.echo_cancellation);
        }
    });

    rsx! {
        div { class: "settings-section voice-settings",
            h2 { "{t(\"settings-voice-video\")}" }

            DeviceSelectRow {
                label_key: "voice-input-device",
                option_key: "voice-default-mic",
            }
            VolumeSlider {
                label: t("voice-input-volume"),
                value: *input_vol.read(),
                on_change: move |v| {
                    input_vol.set(v);
                    save_voice(input_vol, output_vol, vad_mode, noise_suppress, echo_cancel);
                },
            }
            MicTestRow { mic_testing }

            DeviceSelectRow {
                label_key: "voice-output-device",
                option_key: "voice-default-speakers",
            }
            VolumeSlider {
                label: t("voice-output-volume"),
                value: *output_vol.read(),
                on_change: move |v| {
                    output_vol.set(v);
                    save_voice(input_vol, output_vol, vad_mode, noise_suppress, echo_cancel);
                },
            }

            VoiceModeRow {
                selected: vad_mode.read().clone(),
                on_change: move |value: String| {
                    vad_mode.set(value);
                    save_voice(input_vol, output_vol, vad_mode, noise_suppress, echo_cancel);
                },
            }

            NoiseSuppressionRow {
                selected: noise_suppress.read().clone(),
                on_change: move |val: String| {
                    noise_suppress.set(val);
                    save_voice(input_vol, output_vol, vad_mode, noise_suppress, echo_cancel);
                },
            }

            EchoCancellationRow {
                enabled: *echo_cancel.read(),
                on_change: move |enabled| {
                    echo_cancel.set(enabled);
                    save_voice(input_vol, output_vol, vad_mode, noise_suppress, echo_cancel);
                },
            }
        }
    }
}

/// Volume slider with label showing percentage.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn VolumeSlider(label: String, value: u32, on_change: EventHandler<u32>) -> Element {
    rsx! {
        div { class: "voice-settings-row",
            label { class: "voice-settings-label", "{label} — {value}%" }
            input {
                r#type: "range",
                class: "voice-settings-slider",
                min: "0",
                max: "100",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(v) = e.value().parse::<u32>() {
                        on_change.call(v);
                    }
                },
            }
        }
    }
}

/// Noise suppression radio group.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn NoiseSuppressionRow(selected: String, on_change: EventHandler<String>) -> Element {
    rsx! {
        div { class: "voice-settings-row",
            label { class: "voice-settings-label", "{t(\"voice-noise-suppression\")}" }
            div { class: "voice-mode-options",
                for (val , lbl) in [
                    ("off", t("voice-noise-off")),
                    ("standard", t("voice-noise-standard")),
                    ("high", t("voice-noise-high")),
                ]
                {
                    {
                        let is_checked = selected == val;
                        let val_s = val.to_string();
                        rsx! {
                            label { class: "voice-mode-option",
                                input {
                                    r#type: "radio",
                                    name: "noise-suppress",
                                    value: "{val}",
                                    checked: is_checked,
                                    onchange: {
                                        let v = val_s.clone();
                                        move |_| on_change.call(v.clone())
                                    },
                                }
                                "{lbl}"
                            }
                        }
                    }
                }
            }
        }
    }
}
