//! Poly Server signup page component — `/signup/poly`.
//!
//! This module owns the **entire** signup UX for the Poly Server backend.
//! `poly-core` has zero knowledge of what the form contains or how auth works.
//!
//! ## Interface contract
//!
//! [`signup_render_fn`] is registered as `SignupEntry::render` at startup.
//! The host calls it with:
//! - `on_complete: Callback<SignupCompleted>` — called on success; the host
//!   commits the session to `ClientManager` / `ChatData` and navigates.
//! - `ctx: SignupContext` — host-provided private key + i18n lookup fn.
//!
//! ## UX flow (key-first)
//!
//! 1. **URL step**: User enters a server URL and clicks "Connect".
//!    - Poly looks up all existing accounts using the device Ed25519 key.
//!    - If one or more accounts are found → user is shown an account picker.
//!    - If none are found → user is moved to the sign-up name step.
//! 2. **Existing account step**: User picks a previous account or chooses
//!    to create another account on that same server.
//! 3. **Sign-up step**: User chooses username + display name, clicks "Create Account".
//!    - Server registers the key and returns a session.
//!
//! ## i18n
//!
//! All strings live in this crate's `locales/<lang>/plugin.ftl`.
//! At startup, poly-core calls `register_plugin_ftl("poly", locale, src)`
//! which merges these strings into the host bundle.  The `ctx.t` function
//! pointer routes through the host's full Fluent lookup, so locale-switching
//! works at runtime without any extra effort here.
//!
//! ## No circular dependencies
//!
//! `poly-server-client` → `poly-client` (traits + context types)
//!                       → `dioxus`      (UI)
//!
//! It does NOT depend on `poly-core`.  All the state commitment after
//! successful auth is handled by the host via the `on_complete` callback.
//!
//! ## 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::{PolyServerBackend, models::IdentityAccount};
use poly_ui_macros::context_menu;

// ── Public render entry-point ────────────────────────────────────────────────

/// Render entry-point stored in [`poly_client::SignupEntry::render`].
///
/// Called by the host's `ClientSignupPage` with a completion callback and
/// host-provided context.  Returns a Dioxus [`Element`] for the full
/// signup page.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        PolySignupPage { on_complete, ctx }
    }
}

// ── State machine ────────────────────────────────────────────────────────────

/// Which step of the connect flow we are on.
#[derive(Clone, PartialEq)]
enum ConnectStep {
    /// Initial state — user enters a server URL.
    Url,
    /// Existing server accounts were found for this identity key.
    ExistingAccounts {
        server_url: String,
        accounts: Vec<IdentityAccount>,
    },
    /// Server was reachable but the device key is not registered.
    /// The URL that was probed is carried here so Step 2 can show it.
    Signup {
        server_url: String,
        existing_accounts: Vec<IdentityAccount>,
    },
}

// ── Top-level page ───────────────────────────────────────────────────────────

/// Full Poly Server signup page — key-first connect flow.
///
/// The host's `ClientSignupPage` wraps this output in `div.signup-content`
/// which provides scroll, padding, and layout context — so this component
/// renders just a heading, description, and form with no outer card wrapper.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn PolySignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let step: Signal<ConnectStep> = use_signal(|| ConnectStep::Url);
    let t = ctx.t;

    rsx! {
        h2 { class: "signup-form-title",
            "{t(\"plugin-poly-signup-title\")}"
        }
        p { class: "signup-form-desc",
            "{t(\"plugin-poly-signup-description\")}"
        }
        match step.read().clone() {
            ConnectStep::Url => rsx! {
                UrlConnectForm {
                    step,
                    ctx,
                }
            },
            ConnectStep::ExistingAccounts { server_url, accounts } => rsx! {
                ExistingAccountsForm {
                    step,
                    server_url,
                    accounts,
                    ctx,
                    on_complete,
                }
            },
            ConnectStep::Signup {
                server_url,
                existing_accounts,
            } => rsx! {
                SignupDetailsForm {
                    step,
                    server_url,
                    existing_accounts,
                    ctx,
                    on_complete,
                }
            },
        }
    }
}

// ── Step 1: URL entry + Connect ──────────────────────────────────────────────

/// URL entry form — step 1 of the connect flow.
///
/// Looks up all existing accounts bound to the device identity key.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn UrlConnectForm(
    mut step: Signal<ConnectStep>,
    ctx: SignupContext,
) -> Element {
    let mut server_url = use_signal(|| "http://127.0.0.1:7080".to_string());
    let mut connecting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let t = ctx.t;

    rsx! {
        div { class: "signup-form",
            label { class: "settings-label",
                "{t(\"plugin-poly-signup-url-label\")}"
            }
            input {
                class: "settings-input",
                value: "{server_url}",
                placeholder: "{t(\"plugin-poly-signup-url-placeholder\")}",
                disabled: *connecting.read(),
                oninput: move |e: Event<FormData>| server_url.set(e.value()),
            }
            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }
            button {
                class: "btn btn-primary",
                disabled: *connecting.read() || server_url.read().trim().is_empty(),
                onclick: move |_| {
                    let url    = server_url.read().trim().to_string();
                    let key    = ctx.private_key.clone();
                    let no_key = t("plugin-poly-signup-no-identity").to_string();
                    connecting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match discover_accounts(&url, key, &no_key).await {
                            AccountDiscoveryResult::ExistingAccounts(accounts) => {
                                step.set(ConnectStep::ExistingAccounts {
                                    server_url: url,
                                    accounts,
                                });
                                connecting.set(false);
                            }
                            AccountDiscoveryResult::NeedsSignup => {
                                step.set(ConnectStep::Signup {
                                    server_url: url,
                                    existing_accounts: Vec::new(),
                                });
                                connecting.set(false);
                            }
                            AccountDiscoveryResult::Error(e) => {
                                error_msg.set(Some(e));
                                connecting.set(false);
                            }
                        }
                    });
                },
                if *connecting.read() {
                    "{t(\"plugin-poly-signup-connecting\")}"
                } else {
                    "{t(\"plugin-poly-connect-btn\")}"
                }
            }
        }
    }
}

/// Existing-account picker shown when this identity key is already registered.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn ExistingAccountsForm(
    mut step: Signal<ConnectStep>,
    server_url: String,
    accounts: Vec<IdentityAccount>,
    ctx: SignupContext,
    on_complete: Callback<SignupCompleted>,
) -> Element {
    let mut connecting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let t = ctx.t;

    rsx! {
        div { class: "signup-form",
            p { class: "settings-hint",
                "{t(\"plugin-poly-existing-accounts-desc\")}"
            }
            p { class: "signup-server-badge", code { "{server_url}" } }

            div { class: "settings-list",
                for account in accounts.iter().cloned() {
                    {
                        let account_id = account.user_id.clone();
                        let label = if account.display_name == account.username {
                            account.display_name.clone()
                        } else {
                            format!("{} (@{})", account.display_name, account.username)
                        };
                        let url = server_url.clone();
                        let key = ctx.private_key.clone();
                        let no_key = t("plugin-poly-signup-no-identity").to_string();
                        rsx! {
                            button {
                                class: "profile-status-option",
                                disabled: *connecting.read(),
                                onclick: move |_| {
                                    let selected_user_id = account_id.clone();
                                    let url = url.clone();
                                    let key = key.clone();
                                    let no_key = no_key.clone();
                                    connecting.set(true);
                                    error_msg.set(None);
                                    spawn(async move {
                                        match do_signin(&url, Some(selected_user_id), key, &no_key).await {
                                            Ok((session, backend)) => {
                                                on_complete.call(SignupCompleted::new(session, backend));
                                            }
                                            Err(e) => {
                                                error_msg.set(Some(e));
                                                connecting.set(false);
                                            }
                                        }
                                    });
                                },
                                span { class: "status-dot online" }
                                span { "{label}" }
                            }
                        }
                    }
                }
            }

            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }

            div { class: "signup-form-actions",
                button {
                    class: "btn btn-secondary",
                    disabled: *connecting.read(),
                    onclick: move |_| step.set(ConnectStep::Url),
                    "{t(\"plugin-poly-signup-back-btn\")}"
                }
                button {
                    class: "btn btn-primary",
                    disabled: *connecting.read(),
                    onclick: move |_| {
                        step.set(ConnectStep::Signup {
                            server_url: server_url.clone(),
                            existing_accounts: accounts.clone(),
                        });
                    },
                    "{t(\"plugin-poly-create-another-account-btn\")}"
                }
            }
        }
    }
}

// ── Step 2: Sign-up name form ────────────────────────────────────────────────

/// Sign-up details form — step 2 when the device key is not yet registered.
///
/// Shows the confirmed server URL (read-only), username + display name fields,
/// and a "Create Account" button.  A "← Back" link returns to step 1.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn SignupDetailsForm(
    mut step: Signal<ConnectStep>,
    server_url: String,
    existing_accounts: Vec<IdentityAccount>,
    ctx: SignupContext,
    on_complete: Callback<SignupCompleted>,
) -> Element {
    let mut username           = use_signal(String::new);
    let mut email_input        = use_signal(String::new);
    let mut display_name_input = use_signal(String::new);
    let mut connecting         = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let t = ctx.t;

    rsx! {
        div { class: "signup-form",
            p { class: "settings-hint",
                if existing_accounts.is_empty() {
                    "{t(\"plugin-poly-signup-no-account-desc\")}"
                } else {
                    "{t(\"plugin-poly-signup-another-account-desc\")}"
                }
            }
            p { class: "signup-server-badge", code { "{server_url}" } }

            label { class: "settings-label",
                "{t(\"plugin-poly-signup-username-label\")}"
            }
            input {
                class: "settings-input",
                value: "{username}",
                placeholder: "{t(\"plugin-poly-signup-username-placeholder\")}",
                disabled: *connecting.read(),
                oninput: move |e: Event<FormData>| username.set(e.value()),
            }
            label { class: "settings-label",
                "{t(\"plugin-poly-signup-email-label\")}"
            }
            input {
                class: "settings-input",
                r#type: "email",
                value: "{email_input}",
                placeholder: "{t(\"plugin-poly-signup-email-placeholder\")}",
                disabled: *connecting.read(),
                oninput: move |e: Event<FormData>| email_input.set(e.value()),
            }
            label { class: "settings-label",
                "{t(\"plugin-poly-signup-displayname-label\")}"
            }
            input {
                class: "settings-input",
                value: "{display_name_input}",
                placeholder: "{t(\"plugin-poly-signup-displayname-placeholder\")}",
                disabled: *connecting.read(),
                oninput: move |e: Event<FormData>| display_name_input.set(e.value()),
            }
            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }
            div { class: "signup-form-actions",
                {
                    let back_server_url = server_url.clone();
                    let back_accounts = existing_accounts.clone();
                    rsx! {
                button {
                    class: "btn btn-secondary",
                    disabled: *connecting.read(),
                    onclick: move |_| {
                        if back_accounts.is_empty() {
                            step.set(ConnectStep::Url);
                        } else {
                            step.set(ConnectStep::ExistingAccounts {
                                server_url: back_server_url.clone(),
                                accounts: back_accounts.clone(),
                            });
                        }
                    },
                    "{t(\"plugin-poly-signup-back-btn\")}"
                }
                    }
                }
                button {
                    class: "btn btn-primary",
                    disabled: *connecting.read()
                        || username.read().trim().is_empty()
                        || email_input.read().trim().is_empty(),
                    onclick: move |_| {
                        let url   = server_url.clone();
                        let user  = username.read().trim().to_string();
                        let email = email_input.read().trim().to_string();
                        let dname = {
                            let d = display_name_input.read().trim().to_string();
                            if d.is_empty() { user.clone() } else { d }
                        };
                        let key   = ctx.private_key.clone();
                        let no_key = t("plugin-poly-signup-no-identity").to_string();
                        connecting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match do_signup(&url, &user, &email, &dname, key, &no_key).await {
                                Ok((session, backend)) => {
                                    on_complete.call(SignupCompleted::new(session, backend));
                                }
                                Err(e) => {
                                    error_msg.set(Some(e));
                                    connecting.set(false);
                                }
                            }
                        });
                    },
                    if *connecting.read() {
                        "{t(\"plugin-poly-signup-connecting\")}"
                    } else {
                        "{t(\"plugin-poly-create-account-btn\")}"
                    }
                }
            }
        }
    }
}

// ── Authentication logic ─────────────────────────────────────────────────────

/// Result of a silent sign-in probe.
enum AccountDiscoveryResult {
    /// Existing accounts were found for this identity key.
    ExistingAccounts(Vec<IdentityAccount>),
    /// Key is unknown on this server — user needs to sign up.
    NeedsSignup,
    /// Non-auth failure (network error, bad URL, etc.).
    Error(String),
}

/// Discover all existing accounts for this identity key.
///
/// Pure logic — no Dioxus, no poly-core.
async fn discover_accounts(
    server_url: &str,
    private_key: Option<Vec<u8>>,
    no_key_msg: &str,
) -> AccountDiscoveryResult {
    let Ok(key_bytes) = resolve_key(private_key, no_key_msg) else {
        return AccountDiscoveryResult::Error(no_key_msg.to_string());
    };

    let backend = PolyServerBackend::new(server_url, key_bytes);
    match backend.http().list_accounts().await {
        Ok(accounts) if accounts.is_empty() => AccountDiscoveryResult::NeedsSignup,
        Ok(accounts) => AccountDiscoveryResult::ExistingAccounts(accounts),
        Err(e) => AccountDiscoveryResult::Error(format!("{e}")),
    }
}

/// Sign in to an existing Poly Server account selected for this identity key.
async fn do_signin(
    server_url: &str,
    selected_user_id: Option<String>,
    private_key: Option<Vec<u8>>,
    no_key_msg: &str,
) -> Result<
    (
        poly_client::Session,
        Box<dyn poly_client::ClientBackend + Send + Sync>,
    ),
    String,
> {
    let key_bytes = resolve_key(private_key, no_key_msg).map_err(|_| no_key_msg.to_string())?;

    let mut backend = PolyServerBackend::new(server_url, key_bytes);
    let credentials = AuthCredentials::PolyServer {
        server_url: server_url.to_string(),
        private_key_bytes: key_bytes.to_vec(),
        username: None,
        email: None,
        display_name: None,
        selected_user_id,
        is_signup: false,
    };

    let session = backend
        .authenticate(credentials)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok((session, Box::new(backend)))
}

/// Perform new-account signup against the poly server.
///
/// Pure logic — no Dioxus, no poly-core.
async fn do_signup(
    server_url: &str,
    username: &str,
    email: &str,
    display_name: &str,
    private_key: Option<Vec<u8>>,
    no_key_msg: &str,
) -> Result<
    (
        poly_client::Session,
        Box<dyn poly_client::ClientBackend + Send + Sync>,
    ),
    String,
> {
    let key_bytes = resolve_key(private_key, no_key_msg).map_err(|_| no_key_msg.to_string())?;

    let mut backend = PolyServerBackend::new(server_url, key_bytes);
    let credentials = AuthCredentials::PolyServer {
        server_url: server_url.to_string(),
        private_key_bytes: key_bytes.to_vec(),
        username: Some(username.to_string()),
        email: Some(email.to_string()),
        display_name: Some(display_name.to_string()),
        selected_user_id: None,
        is_signup: true,
    };

    let session = backend
        .authenticate(credentials)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok((session, Box::new(backend)))
}

/// Parse the private key bytes from the `SignupContext`.
fn resolve_key(private_key: Option<Vec<u8>>, _no_key_msg: &str) -> Result<[u8; 32], ()> {
    let bytes = private_key.ok_or(())?;
    bytes.try_into().map_err(|_| ())
}
