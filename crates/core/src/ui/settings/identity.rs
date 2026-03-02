//! Identity settings — Account ID, recovery phrase display, and copy actions.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use dioxus::prelude::*;

/// Load the user's 24-word mnemonic from storage.
async fn load_mnemonic_words() -> Result<Vec<String>, String> {
    let s = crate::STORAGE
        .get()
        .ok_or_else(|| "Storage not ready".to_string())?;
    let key_bytes = s
        .get_identity_key()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No identity key in storage".to_string())?;
    let identity = crate::crypto::Identity::from_private_key_bytes(&key_bytes);
    let phrase = identity.to_mnemonic().map_err(|e| e.to_string())?;
    Ok(phrase.split_whitespace().map(str::to_string).collect())
}

/// Modal overlay that displays and allows copying the 24-word recovery phrase.
#[component]
pub(super) fn MnemonicModal(mnemonic_words: Signal<Vec<String>>, show: Signal<bool>) -> Element {
    let mut visible = show;
    rsx! {
        div { class: "modal-overlay", onclick: move |_| visible.set(false),
            div {
                class: "modal-content",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "modal-title", "{t(\"settings-identity-phrase-modal-title\")}" }
                p { class: "modal-warning", "{t(\"settings-identity-phrase-warning\")}" }
                div { class: "mnemonic-grid",
                    {
                        let words = mnemonic_words.read().clone();
                        words
                            .into_iter()
                            .enumerate()
                            .map(|(i, word)| {
                                rsx! {
                                    div { class: "mnemonic-word", key: "{i}",
                                        span { class: "word-number", "{i + 1}." }
                                        span { class: "word-text", "{word}" }
                                    }
                                }
                            })
                    }
                }
                div { class: "modal-actions",
                    button {
                        class: "btn btn-secondary",
                        onclick: move |_| {
                            let phrase = mnemonic_words.read().join(" ");
                            let js = format!(
                                "navigator.clipboard.writeText({:?}).catch(() => {{}})",
                                phrase,
                            );
                            let _ = document::eval(&js);
                        },
                        "{t(\"settings-identity-copy-all\")}"
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| visible.set(false),
                        "{t(\"settings-identity-close\")}"
                    }
                }
            }
        }
    }
}

/// Identity settings section.
///
/// Displays the user's Ed25519 public key (Account ID) and provides a
/// "Show Recovery Phrase" button that opens a [`MnemonicModal`].
#[component]
pub(super) fn IdentitySettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let mut account_id = use_signal(String::new);
    let mut show_phrase_modal = use_signal(|| false);
    let mut mnemonic_words: Signal<Vec<String>> = use_signal(Vec::new);
    let mut status_msg = use_signal(String::new);

    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get() {
            match s.get_app_settings().await {
                Ok(settings) if !settings.account_id.is_empty() => {
                    account_id.set(settings.account_id);
                }
                Ok(_) => status_msg.set(t("settings-identity-no-identity")),
                Err(e) => {
                    tracing::warn!("Failed to load identity: {e}");
                    status_msg.set(t("settings-identity-no-identity"));
                }
            }
        }
    });

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-identity\")}" }
            p { class: "settings-description", "{t(\"settings-identity-description\")}" }

            if !account_id.read().is_empty() {
                div { class: "identity-info",
                    label { class: "settings-label", "{t(\"settings-identity-your-id-label\")}" }
                    div { class: "account-id-row",
                        code { class: "account-id", "{account_id}" }
                        button {
                            class: "btn btn-sm btn-ghost",
                            onclick: move |_| {
                                let id = account_id.read().clone();
                                let js = format!("navigator.clipboard.writeText({:?}).catch(() => {{}})", id);
                                let _ = document::eval(&js);
                            },
                            "{t(\"settings-identity-copy-id\")}"
                        }
                    }
                }
                button {
                    class: "btn btn-secondary",
                    onclick: move |_| {
                        spawn(async move {
                            match load_mnemonic_words().await {
                                Ok(words) => {
                                    mnemonic_words.set(words);
                                    show_phrase_modal.set(true);
                                }
                                Err(e) => tracing::error!("Mnemonic: {e}"),
                            }
                        });
                    },
                    "{t(\"settings-identity-show-phrase\")}"
                }
            } else {
                p { class: "settings-info", "{status_msg}" }
            }

            if *show_phrase_modal.read() {
                MnemonicModal { mnemonic_words, show: show_phrase_modal }
            }
        }
    }
}
