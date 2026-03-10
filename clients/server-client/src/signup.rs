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
//!    - Poly tries a silent sign-in using the device Ed25519 key.
//!    - If the key is registered → `on_complete` is called immediately.
//!    - If not registered → user is moved to the sign-up name step.
//! 2. **Sign-up step**: User chooses username + display name, clicks "Create Account".
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

use crate::PolyServerBackend;

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
    /// Server was reachable but the device key is not registered.
    /// The URL that was probed is carried here so Step 2 can show it.
    Signup { server_url: String },
}

// ── Top-level page ───────────────────────────────────────────────────────────

/// Full Poly Server signup page — key-first connect flow.
///
/// The host's `ClientSignupPage` wraps this output in `div.signup-content`
/// which provides scroll, padding, and layout context — so this component
/// renders just a heading, description, and form with no outer card wrapper.
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
                    on_complete,
                }
            },
            ConnectStep::Signup { server_url } => rsx! {
                SignupDetailsForm {
                    step,
                    server_url,
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
/// Tries a silent sign-in with the device key.  On success calls `on_complete`
/// immediately (user already has a registered key on that server).
/// On auth failure transitions to the sign-up name step.
#[rustfmt::skip]
#[component]
fn UrlConnectForm(
    mut step: Signal<ConnectStep>,
    ctx: SignupContext,
    on_complete: Callback<SignupCompleted>,
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
                        match try_signin(&url, key, &no_key).await {
                            TrySigninResult::LoggedIn(session, backend) => {
                                on_complete.call(SignupCompleted { session, backend });
                            }
                            TrySigninResult::NeedsSignup => {
                                step.set(ConnectStep::Signup { server_url: url });
                                connecting.set(false);
                            }
                            TrySigninResult::Error(e) => {
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

// ── Step 2: Sign-up name form ────────────────────────────────────────────────

/// Sign-up details form — step 2 when the device key is not yet registered.
///
/// Shows the confirmed server URL (read-only), username + display name fields,
/// and a "Create Account" button.  A "← Back" link returns to step 1.
#[rustfmt::skip]
#[component]
fn SignupDetailsForm(
    mut step: Signal<ConnectStep>,
    server_url: String,
    ctx: SignupContext,
    on_complete: Callback<SignupCompleted>,
) -> Element {
    let mut username           = use_signal(String::new);
    let mut display_name_input = use_signal(String::new);
    let mut connecting         = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);
    let t = ctx.t;

    rsx! {
        div { class: "signup-form",
            p { class: "settings-hint",
                "{t(\"plugin-poly-signup-no-account-desc\")}"
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
                button {
                    class: "btn btn-secondary",
                    disabled: *connecting.read(),
                    onclick: move |_| step.set(ConnectStep::Url),
                    "{t(\"plugin-poly-signup-back-btn\")}"
                }
                button {
                    class: "btn btn-primary",
                    disabled: *connecting.read() || username.read().trim().is_empty(),
                    onclick: move |_| {
                        let url   = server_url.clone();
                        let user  = username.read().trim().to_string();
                        let dname = {
                            let d = display_name_input.read().trim().to_string();
                            if d.is_empty() { user.clone() } else { d }
                        };
                        let key   = ctx.private_key.clone();
                        let no_key = t("plugin-poly-signup-no-identity").to_string();
                        connecting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match do_signup(&url, &user, &dname, key, &no_key).await {
                                Ok((session, backend)) => {
                                    on_complete.call(SignupCompleted { session, backend });
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
enum TrySigninResult {
    /// Key is registered — session is ready.
    LoggedIn(
        poly_client::Session,
        Box<dyn poly_client::ClientBackend + Send + Sync>,
    ),
    /// Key is unknown on this server — user needs to sign up.
    NeedsSignup,
    /// Non-auth failure (network error, bad URL, etc.).
    Error(String),
}

/// Attempt silent sign-in with the device key.
///
/// Pure logic — no Dioxus, no poly-core.
async fn try_signin(
    server_url: &str,
    private_key: Option<Vec<u8>>,
    no_key_msg: &str,
) -> TrySigninResult {
    let Ok(key_bytes) = resolve_key(private_key, no_key_msg) else {
        return TrySigninResult::Error(no_key_msg.to_string());
    };

    let mut backend = PolyServerBackend::new(server_url, key_bytes);
    let credentials = AuthCredentials::PolyServer {
        server_url: server_url.to_string(),
        private_key_bytes: key_bytes.to_vec(),
        username: None,
        display_name: None,
        is_signup: false,
    };

    match backend.authenticate(credentials).await {
        Ok(session) => TrySigninResult::LoggedIn(session, Box::new(backend)),
        Err(e) => {
            let msg = format!("{e}");
            // Treat any auth/server error as "not registered" and show signup form.
            // Network-level errors contain "error sending request" / "Connection refused".
            if msg.contains("Connection refused")
                || msg.contains("error sending request")
                || msg.contains("failed to lookup address")
                || msg.contains("os error")
            {
                TrySigninResult::Error(msg)
            } else {
                // Auth-level failure → assume key not registered.
                TrySigninResult::NeedsSignup
            }
        }
    }
}

/// Perform new-account signup against the poly server.
///
/// Pure logic — no Dioxus, no poly-core.
async fn do_signup(
    server_url: &str,
    username: &str,
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
        display_name: Some(display_name.to_string()),
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
