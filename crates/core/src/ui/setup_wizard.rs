//! Setup wizard — first-launch key generation and recovery phrase display.

use crate::i18n::t;
use dioxus::prelude::*;

// ── Step sub-components ───────────────────────────────────────────────────────

/// Step 1: Welcome + generate keypair.
#[component]
fn WelcomeStep(
    step: Signal<u8>,
    account_id: Signal<String>,
    recovery_phrase: Signal<String>,
    private_key_bytes: Signal<Option<[u8; 32]>>,
) -> Element {
    rsx! {
        div { class: "setup-step setup-welcome",
            h1 { class: "setup-title", "{t(\"setup-welcome-title\")}" }
            p { class: "setup-description", "{t(\"setup-welcome-description\")}" }
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    let identity = crate::crypto::Identity::generate();
                    let public = identity.public_identity();
                    let mut pkb = private_key_bytes;
                    pkb.set(Some(identity.private_key_bytes()));
                    let mut aid = account_id;
                    aid.set(public.account_id);
                    if let Ok(phrase) = identity.to_mnemonic() {
                        let mut rp = recovery_phrase;
                        rp.set(phrase);
                    }
                    let mut s = step;
                    s.set(1);
                },
                "{t(\"setup-continue\")}"
            }
        }
    }
}

/// Step 2: Display the generated Account ID with a copy button.
#[component]
fn AccountIdStep(step: Signal<u8>, account_id: Signal<String>) -> Element {
    rsx! {
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
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        let mut s = step;
                        s.set(2);
                    },
                    "{t(\"setup-continue\")}"
                }
            }
        }
    }
}

/// Step 3: Display the 24-word recovery phrase with a copy button.
#[component]
fn RecoveryPhraseStep(step: Signal<u8>, recovery_phrase: Signal<String>) -> Element {
    rsx! {
        div { class: "setup-step setup-recovery",
            h2 { class: "setup-title", "{t(\"setup-recovery-phrase\")}" }
            p { class: "setup-description", "{t(\"setup-recovery-phrase-description\")}" }
            div { class: "recovery-phrase-display",
                {
                    let words: Vec<(usize, String)> = recovery_phrase
                        .read()
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
                        let phrase = recovery_phrase.read().clone();
                        let js = format!(
                            "navigator.clipboard.writeText({:?}).catch(() => {{}})",
                            phrase,
                        );
                        let _ = document::eval(&js);
                    },
                    "{t(\"setup-copy-phrase\")}"
                }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| {
                        let mut s = step;
                        s.set(3);
                    },
                    "{t(\"setup-continue\")}"
                }
            }
        }
    }
}

/// Step 4: Completion — persists the identity key and calls `on_complete`.
#[component]
fn CompleteStep(
    account_id: Signal<String>,
    private_key_bytes: Signal<Option<[u8; 32]>>,
    on_complete: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "setup-step setup-complete",
            h2 { class: "setup-title", "{t(\"setup-complete\")}" }
            p { class: "setup-description", "{t(\"setup-complete-description\")}" }
            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    let id = account_id.read().clone();
                    let key_opt = *private_key_bytes.read();
                    spawn(async move {
                        if let (Some(storage), Some(key)) = (crate::STORAGE.get(), key_opt) {
                            if let Err(e) = storage.set_identity_key(&key).await {
                                tracing::error!("Failed to persist identity key: {e}");
                            } else {
                                tracing::info!("Identity key persisted ✓");
                            }
                        }
                    });
                    on_complete.call(id);
                },
                "{t(\"setup-go-to-app\")}"
            }
        }
    }
}

// ── SetupWizard ───────────────────────────────────────────────────────────────

/// Setup wizard component shown on first launch.
///
/// Walks the user through:
/// 1. Welcome + keypair generation ([`WelcomeStep`])
/// 2. Account ID display ([`AccountIdStep`])
/// 3. Recovery phrase display ([`RecoveryPhraseStep`])
/// 4. Completion — persists key, calls `on_complete` ([`CompleteStep`])
///
/// `on_complete` receives the hex account ID so the parent ([`crate::ui::App`])
/// can persist it to `AppSettings`.
#[component]
pub fn SetupWizard(on_complete: EventHandler<String>) -> Element {
    let step = use_signal(|| 0u8);
    let account_id = use_signal(String::new);
    let recovery_phrase = use_signal(String::new);
    let private_key_bytes: Signal<Option<[u8; 32]>> = use_signal(|| None);
    rsx! {
        div { class: "setup-wizard",
            match *step.read() {
                0 => rsx! {
                    WelcomeStep {
                        step,
                        account_id,
                        recovery_phrase,
                        private_key_bytes,
                    }
                },
                1 => rsx! {
                    AccountIdStep { step, account_id }
                },
                2 => rsx! {
                    RecoveryPhraseStep { step, recovery_phrase }
                },
                _ => rsx! {
                    CompleteStep { account_id, private_key_bytes, on_complete }
                },
            }
        }
    }
}
