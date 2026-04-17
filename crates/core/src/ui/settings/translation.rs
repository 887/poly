//! Translation settings — browser built-in detection + Bergamot download UI.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// What the JS capability probe returned.
#[derive(Clone, Debug, PartialEq)]
enum BrowserTranslation {
    /// Not yet probed.
    Unknown,
    /// `window.translation.createTranslator` exists.
    Available,
    /// API absent or returned unavailable.
    Unavailable,
}

#[ui_action(None)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub(super) fn TranslationSettings() -> Element {
    let mut browser_status = use_signal(|| BrowserTranslation::Unknown);

    // Probe window.translation once on mount.
    use_future(move || async move {
        let mut eval = document::eval(
            r#"
            (async () => {
                try {
                    if (
                        window.translation &&
                        typeof window.translation.createTranslator === 'function'
                    ) {
                        // Quick availability check — some Chromium builds expose the
                        // API surface but report "not available" when models are absent.
                        const cap = window.translation.canTranslate
                            ? await window.translation.canTranslate({ sourceLanguage: 'de', targetLanguage: 'en' })
                            : 'available';
                        dioxus.send(cap === 'no' ? 'unavailable' : 'available');
                    } else {
                        dioxus.send('unavailable');
                    }
                } catch (_) {
                    dioxus.send('unavailable');
                }
            })();
            "#,
        );
        if let Ok(val) = eval.recv::<String>().await {
            browser_status.set(if val == "available" {
                BrowserTranslation::Available
            } else {
                BrowserTranslation::Unavailable
            });
        }
    });

    let status = browser_status.read().clone();

    rsx! {
        div { class: "settings-section translation-settings",
            h2 { "{t(\"settings-translation\")}" }
            p { class: "settings-description", "{t(\"settings-translation-description\")}" }

            // Browser built-in block
            div { class: "translation-provider-block",
                div { class: "translation-provider-header",
                    span { class: "translation-provider-title", "{t(\"settings-translation-browser-title\")}" }
                    match status {
                        BrowserTranslation::Unknown => rsx! {
                            span { class: "translation-badge translation-badge-checking",
                                "{t(\"settings-translation-checking\")}"
                            }
                        },
                        BrowserTranslation::Available => rsx! {
                            span { class: "translation-badge translation-badge-available",
                                "✓ {t(\"settings-translation-available\")}"
                            }
                        },
                        BrowserTranslation::Unavailable => rsx! {
                            span { class: "translation-badge translation-badge-unavailable",
                                "{t(\"settings-translation-not-available\")}"
                            }
                        },
                    }
                }
                p { class: "settings-description translation-provider-body",
                    "{t(\"settings-translation-browser-body\")}"
                }
            }

            // Bergamot block
            div { class: "translation-provider-block",
                div { class: "translation-provider-header",
                    span { class: "translation-provider-title", "{t(\"settings-translation-bergamot-title\")}" }
                    span { class: "translation-badge translation-badge-unavailable",
                        "{t(\"settings-translation-not-installed\")}"
                    }
                }
                p { class: "settings-description translation-provider-body",
                    if status == BrowserTranslation::Available {
                        "{t(\"settings-translation-bergamot-body-optional\")}"
                    } else {
                        "{t(\"settings-translation-bergamot-body-needed\")}"
                    }
                }
                div { class: "translation-bergamot-actions",
                    button {
                        class: "btn btn-secondary btn-sm",
                        disabled: true,
                        "{t(\"settings-translation-download-engine\")}"
                    }
                    span { class: "translation-size-hint", "~5 MB" }
                }
                p { class: "translation-coming-soon", "{t(\"settings-translation-coming-soon\")}" }
            }
        }
    }
}
