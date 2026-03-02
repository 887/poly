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

/// Persist current voice settings to storage.
fn save_voice(input_vol: u32, output_vol: u32, mode: &str, noise: &str, echo: bool) {
    let settings = VoiceSettings {
        input_volume: input_vol,
        output_volume: output_vol,
        input_mode: mode.to_string(),
        noise_suppression: noise.to_string(),
        echo_cancellation: echo,
    };
    spawn(async move {
        if let Some(storage) = crate::STORAGE.get()
            && let Err(e) = storage.set_voice_settings(&settings).await
        {
            tracing::warn!("Failed to save voice settings: {e}");
        }
    });
}

/// Voice & Video settings section.
///
/// Lets the user configure audio/video input/output devices, volume levels,
/// voice activity detection mode, noise suppression and echo cancellation.
/// Settings are loaded from and persisted to storage.
#[component]
pub(super) fn VoiceVideoSettings() -> Element {
    let mut input_vol = use_signal(|| 80_u32);
    let mut output_vol = use_signal(|| 80_u32);
    let mut vad_mode = use_signal(|| String::from("vad"));
    let mut noise_suppress = use_signal(|| String::from("standard"));
    let mut echo_cancel = use_signal(|| true);
    let mut mic_testing = use_signal(|| false);

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

            // Input device
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-input-device\")}" }
                select { class: "poly-select-native",
                    option { value: "default", "{t(\"voice-default-mic\")}" }
                }
            }
            // Input volume
            VolumeSlider {
                label: t("voice-input-volume"),
                value: *input_vol.read(),
                on_change: move |v| {
                    input_vol.set(v);
                    save_voice(
                        v,
                        *output_vol.read(),
                        &vad_mode.read(),
                        &noise_suppress.read(),
                        *echo_cancel.read(),
                    );
                },
            }
            // Mic test
            div { class: "voice-settings-row",
                button {
                    class: if *mic_testing.read() { "mic-test-btn active" } else { "mic-test-btn" },
                    onclick: move |_| {
                        let current = *mic_testing.read();
                        mic_testing.set(!current);
                    },
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

            // Output device
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-output-device\")}" }
                select { class: "poly-select-native",
                    option { value: "default", "{t(\"voice-default-speakers\")}" }
                }
            }
            // Output volume
            VolumeSlider {
                label: t("voice-output-volume"),
                value: *output_vol.read(),
                on_change: move |v| {
                    output_vol.set(v);
                    save_voice(
                        *input_vol.read(),
                        v,
                        &vad_mode.read(),
                        &noise_suppress.read(),
                        *echo_cancel.read(),
                    );
                },
            }

            // Voice Activity Detection vs Push-to-Talk
            div { class: "voice-settings-row voice-mode-row",
                label { class: "voice-settings-label", "{t(\"voice-input-mode\")}" }
                div { class: "voice-mode-options",
                    label { class: "voice-mode-option",
                        input {
                            r#type: "radio",
                            name: "voice-mode",
                            value: "vad",
                            checked: *vad_mode.read() == "vad",
                            onchange: move |_| {
                                vad_mode.set(String::from("vad"));
                                save_voice(
                                    *input_vol.read(),
                                    *output_vol.read(),
                                    "vad",
                                    &noise_suppress.read(),
                                    *echo_cancel.read(),
                                );
                            },
                        }
                        "{t(\"voice-input-vad\")}"
                    }
                    label { class: "voice-mode-option",
                        input {
                            r#type: "radio",
                            name: "voice-mode",
                            value: "ptt",
                            checked: *vad_mode.read() == "ptt",
                            onchange: move |_| {
                                vad_mode.set(String::from("ptt"));
                                save_voice(
                                    *input_vol.read(),
                                    *output_vol.read(),
                                    "ptt",
                                    &noise_suppress.read(),
                                    *echo_cancel.read(),
                                );
                            },
                        }
                        "{t(\"voice-input-ptt\")}"
                    }
                }
            }

            // Noise suppression
            NoiseSuppressionRow {
                selected: noise_suppress.read().clone(),
                on_change: move |val: String| {
                    noise_suppress.set(val.clone());
                    save_voice(
                        *input_vol.read(),
                        *output_vol.read(),
                        &vad_mode.read(),
                        &val,
                        *echo_cancel.read(),
                    );
                },
            }

            // Echo cancellation toggle
            div { class: "voice-settings-row toggle-row",
                label { class: "voice-settings-label", "{t(\"voice-echo-cancel\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *echo_cancel.read(),
                        onchange: move |e| {
                            echo_cancel.set(e.checked());
                            save_voice(
                                *input_vol.read(),
                                *output_vol.read(),
                                &vad_mode.read(),
                                &noise_suppress.read(),
                                e.checked(),
                            );
                        },
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}

/// Volume slider with label showing percentage.
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
