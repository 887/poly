//! Language / locale settings — dropdown to switch the app language.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::common::{PolySelect, SelectOption};
use crate::i18n::t;
use dioxus::prelude::*;

/// Language settings section.
///
/// The dropdown pre-selects the OS/browser-detected language (set during
/// [`crate::i18n::init`]) and switches the entire app's strings reactively
/// on change. Works identically on desktop (Wry) and web (WASM).
#[component]
pub(super) fn LanguageSettings() -> Element {
    // Reads the locale Signal from context — subscribes to changes so the
    // selected option updates immediately when another part of the app
    // changes the locale.
    let mut locale_sig = crate::i18n::use_locale();
    let current_locale = locale_sig.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-language\")}" }
            p { class: "settings-description", "{t(\"settings-language-description\")}" }
            PolySelect {
                options: vec![
                    SelectOption {
                        value: "en",
                        label: "English",
                    },
                    SelectOption {
                        value: "de",
                        label: "Deutsch",
                    },
                    SelectOption {
                        value: "fr",
                        label: "Français",
                    },
                    SelectOption {
                        value: "es",
                        label: "Español",
                    },
                ],
                value: current_locale.clone(),
                onchange: move |new_locale: String| {
                    // Update global i18n state and re-render all subscribed
                    // components via the shared Signal.
                    crate::i18n::set_locale(&new_locale);
                    *locale_sig.write() = new_locale.clone();
                    // Persist (fire-and-forget).
                    spawn(async move {
                        if let Some(s) = crate::STORAGE.get() {
                            match s.get_app_settings().await {
                                Ok(mut settings) => {
                                    settings.locale = new_locale;
                                    if let Err(e) = s.set_app_settings(&settings).await {
                                        tracing::error!("Failed to persist locale: {e}");
                                    } else {
                                        tracing::info!("Locale persisted to storage ✓");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to read settings for locale persist: {e}"
                                    )
                                }
                            }
                        }
                    });
                },
            }
        }
    }
}
