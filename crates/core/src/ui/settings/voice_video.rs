//! Voice & Video settings — audio/video device pickers, VAD, noise suppression.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use dioxus::prelude::*;

/// Voice & Video settings section.
///
/// Lets the user configure audio/video input/output devices, volume levels,
/// voice activity detection mode, noise suppression and echo cancellation.
/// (Actual device enumeration uses browser/OS APIs wired up in later phases.)
#[component]
pub(super) fn VoiceVideoSettings() -> Element {
    let mut input_vol = use_signal(|| 80_u32);
    let mut output_vol = use_signal(|| 80_u32);
    let mut vad_mode = use_signal(|| "vad"); // "vad" | "ptt"
    let mut noise_suppress = use_signal(|| "standard"); // "off" | "standard" | "high"
    let mut echo_cancel = use_signal(|| true);
    let mut mic_testing = use_signal(|| false);

    rsx! {
        div { class: "settings-section voice-settings",
            h2 { "{t(\"settings-voice-video\")}" }

            // Input device
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-input-device\")}" }
                select { class: "poly-select-native",
                    option { value: "default", "Default Microphone" }
                }
            }
            // Input volume
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-input-volume\")} — {input_vol}%" }
                input {
                    r#type: "range",
                    class: "voice-settings-slider",
                    min: "0",
                    max: "100",
                    value: "{input_vol}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<u32>() {
                            input_vol.set(v);
                        }
                    },
                }
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
                    option { value: "default", "Default Speakers" }
                }
            }
            // Output volume
            div { class: "voice-settings-row",
                label { class: "voice-settings-label", "{t(\"voice-output-volume\")} — {output_vol}%" }
                input {
                    r#type: "range",
                    class: "voice-settings-slider",
                    min: "0",
                    max: "100",
                    value: "{output_vol}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<u32>() {
                            output_vol.set(v);
                        }
                    },
                }
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
                            onchange: move |_| vad_mode.set("vad"),
                        }
                        "{t(\"voice-input-vad\")}"
                    }
                    label { class: "voice-mode-option",
                        input {
                            r#type: "radio",
                            name: "voice-mode",
                            value: "ptt",
                            checked: *vad_mode.read() == "ptt",
                            onchange: move |_| vad_mode.set("ptt"),
                        }
                        "{t(\"voice-input-ptt\")}"
                    }
                }
            }

            // Noise suppression
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
                            let val_owned = val;
                            let is_checked = *noise_suppress.read() == val_owned;
                            rsx! {
                                label { class: "voice-mode-option",
                                    input {
                                        r#type: "radio",
                                        name: "noise-suppress",
                                        value: "{val_owned}",
                                        checked: is_checked,
                                        onchange: move |_| noise_suppress.set(val_owned),
                                    }
                                    "{lbl}"
                                }
                            }
                        }
                    }
                }
            }

            // Echo cancellation toggle
            div { class: "voice-settings-row toggle-row",
                label { class: "voice-settings-label", "{t(\"voice-echo-cancel\")}" }
                label { class: "toggle-switch",
                    input {
                        r#type: "checkbox",
                        checked: *echo_cancel.read(),
                        onchange: move |e| echo_cancel.set(e.checked()),
                    }
                    span { class: "toggle-slider" }
                }
            }
        }
    }
}
