//! Setup wizard — first-launch key generation and recovery phrase display.

use crate::i18n::t;
use dioxus::prelude::*;

/// Setup wizard component shown on first launch.
///
/// Walks the user through:
/// 1. Welcome screen
/// 2. Key generation + Account ID display
/// 3. Recovery phrase display
/// 4. Setup complete / redirect
///
/// `on_complete` receives the generated `account_id` string so the parent
/// (`App`) can persist it to storage.
#[component]
pub fn SetupWizard(on_complete: EventHandler<String>) -> Element {
    let mut step = use_signal(|| 0u8);
    let mut account_id = use_signal(String::new);
    let mut recovery_phrase = use_signal(String::new);

    rsx! {
        div { class: "setup-wizard",
            match *step.read() {
                0 => rsx! {
                    // Welcome screen
                    div { class: "setup-step setup-welcome",
                        h1 { class: "setup-title", "{t(\"setup-welcome-title\")}" }
                        p { class: "setup-description", "{t(\"setup-welcome-description\")}" }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                // Generate identity
                                let identity = crate::crypto::Identity::generate();
                                let public = identity.public_identity();
                                account_id.set(public.account_id);
                                if let Ok(phrase) = identity.to_mnemonic() {
                                    recovery_phrase.set(phrase);
                                }
                                step.set(1);
                            },
                            "{t(\"setup-continue\")}"
                        }
                    }
                },
                1 => rsx! {
                    // Account ID display
                    div { class: "setup-step setup-account-id",
                        h2 { class: "setup-title", "{t(\"setup-your-account-id\")}" }
                        p { class: "setup-description", "{t(\"setup-account-id-description\")}" }
                        div { class: "account-id-display",
                            code { "{account_id}" }
                        }
                        button { class: "btn btn-primary", onclick: move |_| step.set(2), "{t(\"setup-continue\")}" }
                    }
                },
                2 => rsx! {
                    // Recovery phrase display
                    div { class: "setup-step setup-recovery", // Recovery phrase display
                        h2 { class: "setup-title", "{t(\"setup-recovery-phrase\")}" }
                        p { class: "setup-description", "{t(\"setup-recovery-phrase-description\")}" }
                        div { class: "recovery-phrase-display",
                            {
                                let phrase = recovery_phrase.read().clone();
                                let words: Vec<(usize, String)> = phrase
                                    .split_whitespace()
                                    .enumerate()
                                    .map(|(i, w)| (i, w.to_string()))
                                    .collect();
                                words
                                    .into_iter()
                                    .map(|(i, word)| {
                                        rsx! {
                                            span { class: "recovery-word", key: "{i}",
                                                span { class: "word-number", "{i + 1}." }
                                                span { class: "word-text", "{word}" }
                                            }
                                        }
                                    }) // TODO(phase-2.7.1.3): Copy to clipboard  TODO(phase-2.7.1.3): Copy to clipboard  TODO(phase-2.7.1.3): Copy to clipboard  TODO(phase-2.7.1.3): Copy to clipboard
                            }
                        }
                        p { class: "setup-warning", "{t(\"setup-recovery-warning\")}" } // TODO(phase-2.7.1.3): Copy to clipboard
                        div { class: "setup-actions",
                            // TODO(phase-2.7.1.3): Copy to clipboard
                            button { class: "btn btn-secondary", onclick: move |_| {}, "{t(\"setup-copy-phrase\")}" } // Setup complete  Setup complete  Setup complete  Setup complete
                            button { class: "btn btn-primary", onclick: move |_| step.set(3), "{t(\"setup-continue\")}" } // Setup complete
                        }
                    } // Setup complete // Setup complete // Setup complete  Setup complete
                },
                _ => rsx! {
                    // Setup complete
                    div { class: "setup-step setup-complete",
                        h2 { class: "setup-title", "{t(\"setup-complete\")}" }
                        p { class: "setup-description", "{t(\"setup-complete-description\")}" }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| on_complete.call(account_id.read().clone()),
                            "{t(\"setup-go-to-app\")}"
                        }
                    }
                },
            }
        }
    }
}
