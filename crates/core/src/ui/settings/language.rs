//! Language / locale settings — flag+checkmark pickers to switch the app language.
//!
//! Shows the 4 supported languages plus a "System (auto-detect)" option.
//! Clicking a flag row sets that locale immediately and persists it.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// One language option shown in the picker.
struct LangOption {
    /// BCP-47 code stored in settings. Empty string = auto-detect from system.
    code: &'static str,
    /// Unicode flag emoji followed by the language's own name.
    label: &'static str,
}

const LANGUAGES: &[LangOption] = &[
    LangOption {
        code: "",
        label: "🌐  System (auto-detect)",
    },
    LangOption {
        code: "en",
        label: "🇬🇧  English",
    },
    LangOption {
        code: "de",
        label: "🇩🇪  Deutsch",
    },
    LangOption {
        code: "fr",
        label: "🇫🇷  Français",
    },
    LangOption {
        code: "es",
        label: "🇪🇸  Español",
    },
];

/// Resolve a stored locale code (possibly empty = "system") to the actual
/// BCP-47 tag that the i18n system understands.
fn resolve_locale(stored: &str) -> &str {
    if stored.is_empty() || stored == "system" {
        // Detect from browser / OS. Fall back to "en" if undetectable.
        #[cfg(target_arch = "wasm32")]
        {
            // JS: navigator.language gives e.g. "en-US", "de"
            // We can only do a static fallback here; real detection happens in i18n::init.
            "en"
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            "en"
        }
    } else {
        stored
    }
}

/// Single language option row with flag, name, and active checkmark.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn LangRow(code: String, label: String, selected: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            class: if selected { "lang-option lang-option-selected" } else { "lang-option" },
            onclick: move |evt| onclick.call(evt),
            span { class: "lang-flag-label", "{label}" }
            if selected {
                span { class: "lang-checkmark", "✓" }
            }
        }
    }
}

/// Language settings section — flag+checkmark picker.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(super) fn LanguageSettings() -> Element {
    let mut locale_sig = crate::i18n::use_locale();
    let current_locale = locale_sig.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-language\")}" }
            p { class: "settings-description", "{t(\"settings-language-description\")}" }
            div { class: "lang-picker",
                for opt in LANGUAGES {
                    {
                        let code = opt.code.to_string();
                        let label = opt.label.to_string();
                        // "system" option is selected when current_locale is empty or "system"
                        let selected = if opt.code.is_empty() {
                            current_locale.is_empty() || current_locale == "system"
                        } else {
                            current_locale == opt.code
                        };
                        let code_for_handler = code.clone();
                        rsx! {
                            LangRow {
                                key: "{code}",
                                code: code.clone(),
                                label,
                                selected,
                                onclick: move |_| {
                                    let new_locale = if code_for_handler.is_empty() {
                                        // "System" = resolve actual preferred language then apply
                                        resolve_locale("").to_string()
                                    } else {
                                        code_for_handler.clone()
                                    };
                                    // Store the raw code (empty = system) in settings
                                    let stored = code_for_handler.clone();
                                    crate::i18n::set_locale(&new_locale);
                                    *locale_sig.write() = if stored.is_empty() {
                                        new_locale.clone()
                                    } else {
                                        stored.clone()
                                    };
                                    spawn(async move {
                                        if let Some(s) = crate::STORAGE.get() {
                                            match s.get_app_settings().await {
                                                Ok(mut settings) => {
                                                    settings.locale = new_locale;
                                                    if let Err(e) = s.set_app_settings(&settings).await {
                                                        tracing::error!("Failed to persist locale: {e}");
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!(
                                                        "Failed to read settings for locale persist: {e}"
                                                    );
                                                }
                                            }
                                        }
                                    });
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
