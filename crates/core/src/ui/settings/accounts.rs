//! Accounts settings section.
//!
//! Lists all active messenger accounts with their display name, backend badge,
//! and a gear icon linking to /:backend/:instance_id/:account_id/settings.
//!
//! Includes `AddPolyServerWizard` — a modal form for adding poly-server accounts.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::client_manager::{BackendHandle, ClientManager};
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use std::collections::HashMap;

/// Derive a stable hsl color from an account ID string (same as search.rs).
fn account_color(account_id: &str) -> String {
    let hash: u32 = account_id.bytes().fold(5381_u32, |h, b| {
        h.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

/// Emoji icon for a backend slug.
fn backend_emoji(slug: &str) -> &'static str {
    match slug {
        "demo" => "🧪",
        "stoat" => "🦦",
        "matrix" => "🟩",
        "discord" => "🟣",
        "teams" => "🟦",
        "poly" => "🔷",
        _ => "💬",
    }
}

/// A single row in the accounts list showing account icon, name, backend, and settings gear.
#[rustfmt::skip]
#[component]
fn AccountRow(
    account_id: String,
    display_name: String,
    backend_slug: String,
    backend_label: String,
    instance_id: String,
    icon_color: String,
) -> Element {
    let icon_char: String = display_name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".to_string());
    let emoji = backend_emoji(&backend_slug);
    rsx! {
        div { class: "accounts-settings-row",
            // Colored icon bubble
            div {
                class: "accounts-settings-icon",
                style: "background: {icon_color}",
                "{icon_char}"
            }
            // Name + backend label
            div { class: "accounts-settings-info",
                span { class: "accounts-settings-name", "{display_name}" }
                span { class: "accounts-settings-backend", "{emoji} {backend_label}" }
            }
            // Gear icon → account settings
            Link {
                to: Route::AccountSettingsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                },
                class: "accounts-settings-gear",
                title: "{t(\"settings-account-settings-link\")}",
                "⚙"
            }
        }
    }
}

/// Accounts settings section.
///
/// Lists active messenger accounts grouped by backend and provides
/// an "Add Account" entry point that opens the Poly Server wizard.
#[rustfmt::skip]
#[component]
pub(super) fn AccountsSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager: Signal<ClientManager> = use_context();
    let _chat_data: Signal<ChatData> = use_context();
    let mut show_wizard = use_signal(|| false);

    let account_ids = client_manager.read().active_account_ids();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-accounts\")}" }
            p { class: "settings-description", "{t(\"settings-accounts-description\")}" }

            if account_ids.is_empty() {
                p { class: "settings-empty-hint", "{t(\"settings-no-accounts\")}" }
            } else {
                div { class: "accounts-settings-list",
                    for account_id in &account_ids {
                        {
                            let aid = account_id.clone();
                            let cm = client_manager.read();
                            let session = cm.sessions.get(&aid);
                            let display_name = session
                                .map(|s| s.user.display_name.clone())
                                .unwrap_or_else(|| aid.clone());
                            let backend_slug = session
                                .map(|s| s.backend.slug().to_string())
                                .unwrap_or_else(|| "demo".to_string());
                            let backend_label = session
                                .map(|s| s.backend.display_name().to_string())
                                .unwrap_or_else(|| "Demo".to_string());
                            let instance_id = session
                                .map(|s| s.instance_id.clone())
                                .unwrap_or_else(|| "demo".to_string());
                            let icon_color = account_color(&aid);
                            rsx! {
                                AccountRow {
                                    key: "{aid}",
                                    account_id: aid,
                                    display_name,
                                    backend_slug,
                                    backend_label,
                                    instance_id,
                                    icon_color,
                                }
                            }
                        }
                    }
                }
            }

            button {
                class: "btn btn-primary",
                onclick: move |_| show_wizard.set(true),
                "{t(\"settings-add-account\")}"
            }

            if *show_wizard.read() {
                AddPolyServerWizard { show: show_wizard }
            }
        }
    }
}

/// Modal wizard for adding a poly-server account.
///
/// ## Signal / RefCell discipline — two-phase approach
///
/// Phase 1 (async): Create backend, authenticate — NO Dioxus Signal lock held.
/// Phase 2 (sync): Commit session + backend + server_map via brief `.write()`.
/// Phase 3 (async): Load data (servers, DMs, etc.) into ChatData.
///
/// This follows the exact same pattern as [`crate::ui::demo::toggle_demo`].
#[rustfmt::skip]
#[component]
fn AddPolyServerWizard(show: Signal<bool>) -> Element {
    let mut visible = show;
    let server_url = use_signal(|| "http://127.0.0.1:7080".to_string());
    let username = use_signal(String::new);
    let display_name_input = use_signal(String::new);
    let is_signup = use_signal(|| true);
    let connecting = use_signal(|| false);
    let error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        div { class: "modal-overlay", onclick: move |_| { if !*connecting.read() { visible.set(false); } },
            div {
                class: "modal-content add-server-wizard",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "modal-title", "{t(\"add-server-wizard-title\")}" }
                p { class: "settings-description", "{t(\"add-server-wizard-description\")}" }

                WizardForm {
                    server_url,
                    username,
                    display_name_input,
                    is_signup,
                    connecting,
                    error_msg,
                    show: visible,
                }
            }
        }
    }
}

/// The inner form body of the wizard — extracted to stay under 150 lines.
#[rustfmt::skip]
#[component]
fn WizardForm(
    server_url: Signal<String>,
    username: Signal<String>,
    display_name_input: Signal<String>,
    is_signup: Signal<bool>,
    connecting: Signal<bool>,
    error_msg: Signal<Option<String>>,
    show: Signal<bool>,
) -> Element {
    let mut server_url = server_url;
    let mut username = username;
    let mut display_name_input = display_name_input;
    let mut is_signup = is_signup;
    let mut connecting = connecting;
    let mut error_msg = error_msg;
    let mut show = show;

    rsx! {
        div { class: "wizard-step-body",
            // Server URL
            label { class: "settings-label", "{t(\"add-server-wizard-url-label\")}" }
            input {
                class: "settings-input",
                value: "{server_url}",
                placeholder: "{t(\"add-server-wizard-url-placeholder\")}",
                oninput: move |e: Event<FormData>| server_url.set(e.value()),
            }

            // Username (signup only)
            if *is_signup.read() {
                label { class: "settings-label", "{t(\"add-server-wizard-username-label\")}" }
                input {
                    class: "settings-input",
                    value: "{username}",
                    placeholder: "{t(\"add-server-wizard-username-placeholder\")}",
                    oninput: move |e: Event<FormData>| username.set(e.value()),
                }

                label { class: "settings-label", "{t(\"add-server-wizard-displayname-label\")}" }
                input {
                    class: "settings-input",
                    value: "{display_name_input}",
                    placeholder: "{t(\"add-server-wizard-displayname-placeholder\")}",
                    oninput: move |e: Event<FormData>| display_name_input.set(e.value()),
                }
            }

            // Signup / Signin toggle
            div { class: "wizard-mode-toggle",
                label { class: "settings-label",
                    input {
                        r#type: "checkbox",
                        checked: *is_signup.read(),
                        onchange: move |_| {
                        let current = *is_signup.read();
                        is_signup.set(!current);
                    },
                    }
                    " {t(\"add-server-wizard-signup\")}"
                }
                p { class: "wizard-step-hint",
                    if *is_signup.read() {
                        "{t(\"add-server-wizard-signup-hint\")}"
                    } else {
                        "{t(\"add-server-wizard-signin-hint\")}"
                    }
                }
            }

            // Error display
            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }
        }

        // Actions
        div { class: "modal-actions",
            button {
                class: "btn btn-secondary",
                disabled: *connecting.read(),
                onclick: move |_| show.set(false),
                "{t(\"add-server-wizard-cancel\")}"
            }
            button {
                class: "btn btn-primary",
                disabled: *connecting.read(),
                onclick: move |_| {
                    let url = server_url.read().clone();
                    let user = username.read().clone();
                    let dname = display_name_input.read().clone();
                    let signup = *is_signup.read();
                    connecting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match do_add_poly_server(&url, &user, &dname, signup).await {
                            Ok(()) => {
                                show.set(false);
                            }
                            Err(e) => {
                                error_msg.set(Some(e));
                            }
                        }
                        connecting.set(false);
                    });
                },
                if *connecting.read() {
                    "{t(\"add-server-wizard-connecting\")}"
                } else if *is_signup.read() {
                    "{t(\"add-server-wizard-signup\")}"
                } else {
                    "{t(\"add-server-wizard-signin\")}"
                }
            }
        }
    }
}

/// Perform the actual poly-server add using the two-phase pattern.
///
/// Phase 1: async auth without Signal locks.
/// Phase 2: sync commit to ClientManager + ChatData.
/// Phase 3: async data loading from the new backend.
async fn do_add_poly_server(
    server_url: &str,
    username: &str,
    display_name: &str,
    is_signup: bool,
) -> Result<(), String> {
    use poly_client::ClientBackend as _;
    use poly_server_client::PolyServerBackend;

    // ── Step 0: Get the identity key from storage ───────────────────────────
    let storage = crate::STORAGE
        .get()
        .ok_or_else(|| "Storage not ready".to_string())?;
    let private_key_bytes = storage
        .get_identity_key()
        .await
        .map_err(|e| format!("Storage error: {e}"))?
        .ok_or_else(|| t("add-server-wizard-no-identity").to_string())?;

    // ── Phase 1: async auth — NO Dioxus Signal lock held ────────────────────
    let mut backend = PolyServerBackend::new(server_url, private_key_bytes);
    let credentials = poly_client::AuthCredentials::PolyServer {
        server_url: server_url.to_string(),
        private_key_bytes: private_key_bytes.to_vec(),
        username: if is_signup { Some(username.to_string()) } else { None },
        display_name: if is_signup { Some(display_name.to_string()) } else { None },
        is_signup,
    };

    let session = backend
        .authenticate(credentials)
        .await
        .map_err(|e| format!("{e}"))?;

    let account_id = session.id.clone();
    let backend_handle: BackendHandle =
        std::sync::Arc::new(tokio::sync::RwLock::new(Box::new(backend)));

    // Build server→account map from the authenticated backend.
    let mut server_map = HashMap::new();
    {
        let guard = backend_handle.read().await;
        if let Ok(servers) = guard.get_servers().await {
            for srv in &servers {
                server_map.insert(srv.id.clone(), account_id.clone());
            }
        }
    }

    // ── Phase 2: sync commit — brief Signal writes, NO await ────────────────
    let mut client_manager: Signal<ClientManager> = consume_context();
    let mut chat_data: Signal<ChatData> = consume_context();

    client_manager.write().commit_poly_server(
        account_id.clone(),
        session.clone(),
        backend_handle.clone(),
        server_map,
    );

    // Copy session into ChatData for sidebar rendering.
    chat_data
        .write()
        .account_sessions
        .insert(account_id.clone(), session);

    // ── Phase 3: async data loading — no Signal lock held ───────────────────
    {
        let guard = backend_handle.read().await;
        if let Ok(servers) = guard.get_servers().await {
            let mut cd = chat_data.write();
            for srv in &servers {
                if !cd.favorited_server_ids.contains(&srv.id) {
                    cd.favorited_server_ids.push(srv.id.clone());
                }
            }
            cd.servers.extend(servers);
        }
    }
    {
        let guard = backend_handle.read().await;
        if let Ok(dms) = guard.get_dm_channels().await {
            chat_data.write().dm_channels.extend(dms);
        }
        if let Ok(groups) = guard.get_groups().await {
            chat_data.write().groups.extend(groups);
        }
        if let Ok(notifs) = guard.get_notifications().await {
            chat_data.write().notifications.extend(notifs);
        }
        if let Ok(friends) = guard.get_friends().await {
            for friend in friends {
                if !chat_data.read().friends.iter().any(|f| f.id == friend.id) {
                    chat_data.write().friends.push(friend);
                }
            }
        }
    }

    tracing::info!("Poly server account added successfully: {account_id}");
    Ok(())
}
