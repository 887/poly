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

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::PolyServerBackend;

// ── Public render entry-point ────────────────────────────────────────────────

/// Render entry-point stored in [`poly_client::SignupEntry::render`].
///
/// Called by the host's `ClientSignupPage` with a completion callback and
/// host-provided context.  Returns a Dioxus [`Element`] for the full
/// signup page.
pub fn signup_render_fn(
    on_complete: Callback<SignupCompleted>,
    ctx: SignupContext,
) -> Element {
    rsx! {
        PolySignupPage { on_complete, ctx }
    }
}

// ── Components ───────────────────────────────────────────────────────────────

/// Full Poly Server signup / sign-in page component.
///
/// Renders a card with a server URL field, mode toggle, and credential
/// fields appropriate for the selected mode.  On success, calls
/// `on_complete` — the host handles all state management after that.
#[rustfmt::skip]
#[component]
fn PolySignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let server_url         = use_signal(|| "http://127.0.0.1:7080".to_string());
    let username           = use_signal(String::new);
    let display_name_input = use_signal(String::new);
    let is_signup          = use_signal(|| true);
    let connecting         = use_signal(|| false);
    let error_msg: Signal<Option<String>> = use_signal(|| None);

    let t             = ctx.t;
    let navigate_back = ctx.navigate_back;

    rsx! {
        div { class: "signup-page-root",
            div { class: "signup-card",
                button {
                    class: "signup-back-link",
                    onclick: move |_| navigate_back(),
                    "{t(\"plugin-poly-signup-back\")}"
                }
                h2 { class: "signup-card-title",
                    "{t(\"plugin-poly-signup-title\")}"
                }
                p { class: "signup-card-desc",
                    "{t(\"plugin-poly-signup-description\")}"
                }
                PolySignupForm {
                    server_url,
                    username,
                    display_name_input,
                    is_signup,
                    connecting,
                    error_msg,
                    ctx,
                    on_complete,
                }
            }
        }
    }
}

/// Inner form — extracted to stay under the 150-line component rule.
#[rustfmt::skip]
#[component]
fn PolySignupForm(
    server_url:          Signal<String>,
    username:            Signal<String>,
    display_name_input:  Signal<String>,
    is_signup:           Signal<bool>,
    connecting:          Signal<bool>,
    error_msg:           Signal<Option<String>>,
    ctx:                 SignupContext,
    on_complete:         Callback<SignupCompleted>,
) -> Element {
    let mut server_url         = server_url;
    let mut username           = username;
    let mut display_name_input = display_name_input;
    let mut is_signup          = is_signup;
    let mut connecting         = connecting;
    let mut error_msg          = error_msg;

    let t = ctx.t;

    let submit_label = if *connecting.read() {
        t("plugin-poly-signup-connecting").to_string()
    } else if *is_signup.read() {
        t("plugin-poly-signup-btn").to_string()
    } else {
        t("plugin-poly-signin-btn").to_string()
    };

    rsx! {
        div { class: "signup-form",
            // ── Server URL ────────────────────────────────────────────────
            label { class: "settings-label",
                "{t(\"plugin-poly-signup-url-label\")}"
            }
            input {
                class: "settings-input",
                value: "{server_url}",
                placeholder: "{t(\"plugin-poly-signup-url-placeholder\")}",
                oninput: move |e: Event<FormData>| server_url.set(e.value()),
            }

            // ── Mode toggle (sign-up vs sign-in) ──────────────────────────
            div { class: "wizard-mode-toggle",
                label { class: "settings-label",
                    input {
                        r#type: "checkbox",
                        checked: *is_signup.read(),
                        onchange: move |_| {
                            let cur = *is_signup.read();
                            is_signup.set(!cur);
                        },
                    }
                    " {t(\"plugin-poly-signup-mode-signup\")}"
                }
            }

            // ── Username + display name (sign-up only) ────────────────────
            if *is_signup.read() {
                label { class: "settings-label",
                    "{t(\"plugin-poly-signup-username-label\")}"
                }
                input {
                    class: "settings-input",
                    value: "{username}",
                    placeholder: "{t(\"plugin-poly-signup-username-placeholder\")}",
                    oninput: move |e: Event<FormData>| username.set(e.value()),
                }
                label { class: "settings-label",
                    "{t(\"plugin-poly-signup-displayname-label\")}"
                }
                input {
                    class: "settings-input",
                    value: "{display_name_input}",
                    placeholder: "{t(\"plugin-poly-signup-displayname-placeholder\")}",
                    oninput: move |e: Event<FormData>| display_name_input.set(e.value()),
                }
            }

            // ── Error display ─────────────────────────────────────────────
            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }

            // ── Submit ────────────────────────────────────────────────────
            button {
                class: "btn btn-primary",
                disabled: *connecting.read(),
                onclick: move |_| {
                    let url    = server_url.read().clone();
                    let user   = username.read().clone();
                    let dname  = display_name_input.read().clone();
                    let signup = *is_signup.read();
                    let key    = ctx.private_key.clone();
                    let no_key = t("plugin-poly-signup-no-identity").to_string();
                    connecting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match do_authenticate(&url, &user, &dname, signup, key, &no_key).await {
                            Ok((session, backend)) => {
                                on_complete.call(SignupCompleted { session, backend });
                                // Host navigates away — this component will be dropped.
                            }
                            Err(e) => {
                                error_msg.set(Some(e));
                                connecting.set(false);
                            }
                        }
                    });
                },
                "{submit_label}"
            }
        }
    }
}

// ── Authentication logic ────────────────────────────────────────────────────

/// Perform the Poly Server authenticate exchange.
///
/// Pure client-side logic — no Dioxus Signals, no poly-core imports.
/// Returns `(Session, backend)` on success; the caller wraps the backend
/// in `Arc<RwLock<...>>`.
async fn do_authenticate(
    server_url:   &str,
    username:     &str,
    display_name: &str,
    is_signup:    bool,
    private_key:  Option<Vec<u8>>,
    no_key_msg:   &str,
) -> Result<(poly_client::Session, Box<dyn poly_client::ClientBackend + Send + Sync>), String> {
    let key_bytes: Vec<u8> = private_key.ok_or_else(|| no_key_msg.to_string())?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| "Identity key must be exactly 32 bytes".to_string())?;

    let mut backend = PolyServerBackend::new(server_url, key_array);
    let credentials = AuthCredentials::PolyServer {
        server_url:        server_url.to_string(),
        private_key_bytes: key_array.to_vec(),
        username:          if is_signup { Some(username.to_string()) } else { None },
        display_name:      if is_signup { Some(display_name.to_string()) } else { None },
        is_signup,
    };

    let session = backend
        .authenticate(credentials)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok((session, Box::new(backend)))
}
