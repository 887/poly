//! Identity settings — create, view, and manage multiple Poly identity keys.
//!
//! The user can have one or more Ed25519 keypairs. Each identity serves as the
//! user's identity on backup servers and Poly server accounts. The public key
//! is the Account ID.
//!
//! # Features
//! - Create new identities (generates keypair, shows 24-word mnemonic)
//! - View all identities with their Account IDs
//! - View recovery phrase for each identity (in a modal)
//! - See which backup servers and Poly accounts use each identity
//! - Copy Account ID to clipboard
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the identity settings section.
pub enum IdentitySettingsAction {
    /// Generate a new Ed25519 identity keypair.
    CreateIdentity,
    /// Delete an identity by its account ID.
    DeleteIdentity(String),
    /// Copy an account ID to the clipboard.
    CopyAccountId(String),
    /// Show the mnemonic recovery phrase for an identity.
    ShowRecoveryPhrase(String),
}

impl UiAction for IdentitySettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::CreateIdentity => todo!("phase-E: generate new identity keypair"),
            Self::DeleteIdentity(_account_id) => todo!("phase-E: delete identity"),
            Self::CopyAccountId(_account_id) => todo!("phase-E: copy account ID to clipboard"),
            Self::ShowRecoveryPhrase(_account_id) => todo!("phase-E: show recovery phrase modal"),
        }
    }
}

#[derive(Clone, PartialEq)]
struct LinkedPolyAccount {
    account_id: String,
    display_name: String,
    server_url: Option<String>,
}

/// Generate a brand-new identity and store it. Returns (account_id, words).
async fn create_identity() -> Result<(String, Vec<String>), String> {
    let s = crate::STORAGE
        .get()
        .ok_or_else(|| "Storage not ready".to_string())?;
    let identity = crate::crypto::Identity::generate();
    let account_id = identity.public_identity().account_id;
    let phrase = identity.to_mnemonic().map_err(|e| e.to_string())?;
    let words: Vec<String> = phrase.split_whitespace().map(str::to_string).collect();
    s.set_identity_key(&identity.private_key_bytes())
        .await
        .map_err(|e| e.to_string())?;
    // Persist account_id to app settings
    if let Ok(mut settings) = s.get_app_settings().await {
        settings.account_id = account_id.clone();
        let _ = s.set_app_settings(&settings).await;
    }
    Ok((account_id, words))
}

/// Modal overlay that displays and allows copying the 24-word recovery phrase.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn MnemonicModal(
    account_id: String,
    mnemonic_words: Signal<Vec<String>>,
    show: Signal<bool>,
) -> Element {
    let mut visible = show;
    rsx! {
        div { class: "modal-overlay", onclick: move |_| visible.set(false),
            div {
                class: "modal-content identity-mnemonic-modal",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "modal-title", "{t(\"settings-identity-phrase-modal-title\")}" }
                p { class: "modal-warning", "{t(\"settings-identity-phrase-warning\")}" }
                p { class: "modal-account-subtitle", "{account_id}" }
                div { class: "mnemonic-grid",
                    {
                        let words = mnemonic_words.read().clone();
                        words.into_iter().enumerate().map(|(i, word)| {
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
/// Shows all identities the user has created, with options to:
/// - View / copy Account ID
/// - Show recovery phrase
/// - See which backup servers use each identity
/// - Create additional identities
#[rustfmt::skip]
#[ui_action(IdentitySettingsAction)]
#[context_menu(inherit)]
#[component]
pub(super) fn IdentitySettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let mut identities: Signal<Vec<String>> = use_signal(Vec::new);
    let mut show_phrase_modal = use_signal(|| false);
    let mut mnemonic_words: Signal<Vec<String>> = use_signal(Vec::new);
    let mut modal_account_id: Signal<String> = use_signal(String::new);
    let mut creating = use_signal(|| false);

    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get()
            && let Ok(settings) = s.get_app_settings().await
            && !settings.account_id.is_empty()
        {
            identities.set(vec![settings.account_id]);
        }
    });

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-identity\")}" }
            p { class: "settings-description", "{t(\"settings-identity-description\")}" }

            if identities.read().is_empty() {
                // No identities yet — offer creation
                p { class: "settings-info", "{t(\"settings-identity-no-identity\")}" }
                button {
                    class: "btn btn-primary",
                    disabled: *creating.read(),
                    onclick: move |_| {
                        creating.set(true);
                        spawn(async move {
                            match create_identity().await {
                                Ok((id, words)) => {
                                    let id_copy = id.clone();
                                    identities.write().push(id);
                                    modal_account_id.set(id_copy);
                                    mnemonic_words.set(words);
                                    show_phrase_modal.set(true);
                                }
                                Err(e) => {
                                    tracing::error!("Create identity: {e}");
                                }
                            }
                            creating.set(false);
                        });
                    },
                    if *creating.read() {
                        "{t(\"settings-identity-creating\")}"
                    } else {
                        "{t(\"settings-identity-create-btn\")}"
                    }
                }
            } else {
                // List all identities
                div { class: "identity-list",
                    {
                        let id_list = identities.read().clone();
                        id_list.into_iter().map(|id| {
                            let id_for_card = id.clone();
                            rsx! {
                                IdentityCard {
                                    key: "{id}",
                                    account_id: id_for_card,
                                    on_show_phrase: move |_| {
                                        let id = id.clone();
                                        spawn(async move {
                                            match load_mnemonic_words_for(&id).await {
                                                Ok(words) => {
                                                    modal_account_id.set(id);
                                                    mnemonic_words.set(words);
                                                    show_phrase_modal.set(true);
                                                }
                                                Err(e) => tracing::error!("Mnemonic: {e}"),
                                            }
                                        });
                                    },
                                    on_delete: move |id_to_delete: String| {
                                        identities.write().retain(|x| x != &id_to_delete);
                                    },
                                }
                            }
                        })
                    }
                }

                // Button to create additional identities
                button {
                    class: "btn btn-secondary",
                    disabled: *creating.read(),
                    onclick: move |_| {
                        creating.set(true);
                        spawn(async move {
                            match create_identity().await {
                                Ok((id, words)) => {
                                    let id_copy = id.clone();
                                    identities.write().push(id);
                                    modal_account_id.set(id_copy);
                                    mnemonic_words.set(words);
                                    show_phrase_modal.set(true);
                                }
                                Err(e) => {
                                    tracing::error!("Create identity: {e}");
                                }
                            }
                            creating.set(false);
                        });
                    },
                    "{t(\"settings-identity-create-btn\")}"
                }
            }

            if *show_phrase_modal.read() {
                MnemonicModal { 
                    account_id: modal_account_id.read().clone(),
                    mnemonic_words, 
                    show: show_phrase_modal 
                }
            }
        }
    }
}

/// Single identity card showing account ID, backup servers, Poly accounts, and delete button.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn IdentityCard(
    account_id: String,
    on_show_phrase: EventHandler<String>,
    on_delete: EventHandler<String>,
) -> Element {
    let mut servers: Signal<Vec<crate::storage::BackupServerRecord>> = use_signal(Vec::new);
    let mut poly_accounts: Signal<Vec<LinkedPolyAccount>> = use_signal(Vec::new);
    let mut show_delete_confirm = use_signal(|| false);

    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get()
            && let Ok(list) = s.get_backup_servers().await
        {
            servers.set(list);
        }
    });

    use_future(move || async move {
        if let Some(s) = crate::STORAGE.get()
            && let Ok(tokens) = s.get_account_tokens().await
        {
            poly_accounts.set(
                tokens
                    .into_iter()
                    .filter(|token| token.backend == "poly")
                    .map(|token| LinkedPolyAccount {
                        account_id: token.account_id,
                        display_name: token.display_name,
                        server_url: token.instance_id,
                    })
                    .collect(),
            );
        }
    });

    rsx! {
        div { class: "identity-card",
            div { class: "identity-card-header",
                div { class: "identity-account-id",
                    code { "{account_id}" }
                    {
                        let account_id_copy = account_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-sm btn-ghost",
                                onclick: move |_| {
                                    let js = format!("navigator.clipboard.writeText({:?}).catch(() => {{}})", account_id_copy);
                                    let _ = document::eval(&js);
                                },
                                "{t(\"settings-identity-copy-id\")}"
                            }
                        }
                    }
                }
                {
                    let account_id_show = account_id.clone();
                    rsx! {
                        button {
                            class: "btn btn-sm btn-secondary",
                            onclick: move |_| on_show_phrase.call(account_id_show.clone()),
                            "{t(\"settings-identity-show-phrase\")}"
                        }
                    }
                }
                {
                    rsx! {
                        button {
                            class: "btn btn-sm btn-danger",
                            onclick: move |_| show_delete_confirm.set(true),
                            "{t(\"settings-identity-delete\")}"
                        }
                    }
                }
            }

            div { class: "settings-description",
                p { "{t(\"settings-identity-purpose\")}" }
                ul { class: "settings-list",
                    li { "{t(\"settings-identity-purpose-poly\")}" }
                    li { "{t(\"settings-identity-purpose-backup\")}" }
                }
            }
            
            if !servers.read().is_empty() {
                div { class: "identity-card-usage",
                    h4 { class: "settings-subsection-title", "{t(\"settings-identity-backup-servers\")}" }
                    p { class: "settings-hint", "{t(\"settings-identity-backup-servers-description\")}" }
                    div { class: "identity-server-list",
                        for server in servers.read().iter() {
                            div { class: "identity-server-row",
                                div { class: "identity-server-info",
                                    span { class: "identity-server-label", "{server.label}" }
                                    span { class: "identity-server-url", "{server.url}" }
                                }
                                span {
                                    class: if server.enabled { "identity-server-status status-ok" } else { "identity-server-status status-off" },
                                    if server.enabled {
                                        "{t(\"settings-identity-status-active\")}" 
                                    } else {
                                        "{t(\"settings-identity-status-disabled\")}" 
                                    }
                                }
                            }
                        }
                    }
                }
            }
            else {
                div { class: "identity-card-usage",
                    h4 { class: "settings-subsection-title", "{t(\"settings-identity-backup-servers\")}" }
                    p { class: "settings-hint", "{t(\"settings-identity-backup-servers-description\")}" }
                    p { class: "settings-info", "{t(\"settings-identity-no-servers\")}" }
                }
            }
            
            if !poly_accounts.read().is_empty() {
                div { class: "identity-card-usage",
                    h4 { class: "settings-subsection-title", "{t(\"settings-identity-poly-accounts\")}" }
                    p { class: "settings-hint", "{t(\"settings-identity-poly-accounts-description\")}" }
                    div { class: "identity-server-list",
                        for acct in poly_accounts.read().iter() {
                            div { class: "identity-server-row",
                                div { class: "identity-server-info",
                                    span { class: "identity-server-label", "{acct.display_name}" }
                                    span { class: "identity-server-url",
                                        "{acct.server_url.clone().unwrap_or_else(|| acct.account_id.clone())}"
                                    }
                                }
                                span { class: "identity-server-status status-ok", "{t(\"settings-identity-status-active\")}" }
                            }
                        }
                    }
                }
            }
            else {
                div { class: "identity-card-usage",
                    h4 { class: "settings-subsection-title", "{t(\"settings-identity-poly-accounts\")}" }
                    p { class: "settings-hint", "{t(\"settings-identity-poly-accounts-description\")}" }
                    p { class: "settings-info", "{t(\"settings-identity-no-poly-accounts\")}" }
                }
            }
            
            if *show_delete_confirm.read() {
                div { class: "modal-overlay", onclick: move |_| show_delete_confirm.set(false),
                    div {
                        class: "modal-content",
                        onclick: move |e| e.stop_propagation(),
                        h3 { class: "modal-title", "{t(\"settings-identity-delete-confirm-title\")}" }
                        p { class: "modal-warning", "{t(\"settings-identity-delete-confirm-message\")}" }
                        div { class: "modal-actions",
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| show_delete_confirm.set(false),
                                "{t(\"settings-identity-cancel\")}"
                            }
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| {
                                    let account_id_del = account_id.clone();
                                    spawn(async move {
                                        if let Some(s) = crate::STORAGE.get()
                                            && let Err(e) = s.delete_identity_key().await
                                        {
                                            tracing::error!("Delete identity: {e}");
                                        }
                                        on_delete.call(account_id_del);
                                    });
                                    show_delete_confirm.set(false);
                                },
                                "{t(\"settings-identity-delete-confirm\")}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Load mnemonic for a specific identity by account ID.
async fn load_mnemonic_words_for(_account_id: &str) -> Result<Vec<String>, String> {
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
