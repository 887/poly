//! Setup wizard — first-launch key generation and recovery phrase display.

use crate::i18n::t;
use dioxus::prelude::*;

/// Setup wizard component shown on first launch.
///
/// Walks the user through:
/// 1. Welcome screen (generates Ed25519 keypair on click)
/// 2. Account ID display (public key = Poly ID)
/// 3. Recovery phrase display (BIP39 mnemonic, copy button)
/// 4. Setup complete / redirect
///
/// `on_complete` receives the generated `account_id` string so the parent
/// (`App`) can persist it to `AppSettings`. The wizard itself persists the
/// raw private key bytes via `Storage::set_identity_key()`.
#[component]
pub fn SetupWizard(on_complete: EventHandler<String>) -> Element {
    let mut step = use_signal(|| 0u8);
    let mut account_id = use_signal(String::new);
    let mut recovery_phrase = use_signal(String::new);
    // Raw Ed25519 private key bytes — persisted to storage when wizard completes.
    let mut private_key_bytes: Signal<Option<[u8; 32]>> = use_signal(|| None);

    rsx! {
        div { class: "setup-wizard",
            match *step.read() {
                0 => rsx! {
                    // Step 1: Welcome — generate identity on click
                    div { class: "setup-step setup-welcome",
                        h1 { class: "setup-title", "{t(\"setup-welcome-title\")}" }
                        p { class: "setup-description", "{t(\"setup-welcome-description\")}" }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                let identity = crate::crypto::Identity::generate();
                                let public = identity.public_identity();
                                private_key_bytes.set(Some(identity.private_key_bytes()));
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
                    // Step 2: Account ID display
                    div { class: "setup-step setup-account-id",
                        h2 { class: "setup-title", "{t(\"setup-your-account-id\")}" }
                        p { class: "setup-description", "{t(\"setup-account-id-description\")}" }
                        div { class: "account-id-display",
                            code { class: "account-id", "{account_id}" }
                        }
                        div { class: "setup-actions",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| {
                                    let id = account_id.read().clone();
                                    let js = format!("navigator.clipboard.writeText({:?}).catch(() => {{}})", id);
                                    let _ = document::eval(&js);
                                },
                                "{t(\"action-copy\")}"
                            }
                            button { class: "btn btn-primary", onclick: move |_| step.set(2), "{t(\"setup-continue\")}" }
                        }
                    }
                },
                2 => rsx! {
                    // Step 3: Recovery phrase display
                    div { class: "setup-step setup-recovery", // Step 3: Recovery phrase display
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
                                    })
                            }
                        }
                        p { class: "setup-warning", "{t(\"setup-recovery-warning\")}" }
                        div { class: "setup-actions",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| {
                                    let phrase = recovery_phrase.read().clone(); // Step 4: Complete
                                    let js = format!(
                                        "navigator.clipboard.writeText({:?}).catch(() => {{}})",
                                        phrase,
                                    );
                                    let _ = document::eval(&js);
                                },
                                "{t(\"setup-copy-phrase\")}"
                            }
                            button { class: "btn btn-primary", onclick: move |_| step.set(3), "{t(\"setup-continue\")}" }
                        }
                    }
                },
                _ => rsx! {
                    // Step 4: Complete
                    div { class: "setup-step setup-complete",
                        h2 { class: "setup-title", "{t(\"setup-complete\")}" }
                        p { class: "setup-description", "{t(\"setup-complete-description\")}" }
                        button {
                            class: "btn btn-primary",
                            onclick: move |_| {
                                let id = account_id.read().clone();
                                let key_opt = *private_key_bytes.read();

                                // Persist private key bytes to storage before completing.
                                // Fire-and-forget — the App's on_complete handler persists AppSettings.
                                spawn(async move {
                                    if let (Some(storage), Some(key)) = (crate::STORAGE.get(), key_opt) {
                                        if let Err(e) = storage.set_identity_key(&key).await {
                                            tracing::error!("Failed to persist identity key: {e}");
                                        } else {
                                            tracing::info!("Identity key persisted to storage ✓");
                                        }
                                    }
                                });

                                on_complete.call(id);
                            },
                            "{t(\"setup-go-to-app\")}"
                        }
                    }
                },
            }
        }
    }
}
