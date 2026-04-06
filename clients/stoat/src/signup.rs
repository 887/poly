//! Stoat signup / login page component — `/signup/stoat`.
//!
//! This is the host-facing login UI for the native Stoat backend.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::{OFFICIAL_STOAT_BASE_URL, StoatClient};

/// Authenticate against a Stoat/Revolt server. Public so test panels can call it.
pub async fn authenticate(
    base_url: String,
    email: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let mut backend = StoatClient::with_base_url(base_url).map_err(|error| error.to_string())?;
    let session = backend
        .authenticate(AuthCredentials::EmailPassword { email, password })
        .await
        .map_err(|error| error.to_string())?;

    Ok(SignupCompleted {
        session,
        backend: Box::new(backend),
    })
}

fn stoat_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

fn raccoon_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

/// Test accounts for the Stoat local dev server (port 9101).
pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🦦",
            label: "Stoat",
            server_label: "Stoat — localhost:9101",
            base_url: "http://localhost:9101",
            username: "stoat",
            password: "testpass123",
            authenticate: stoat_auth,
        },
        TestAccountEntry {
            icon: "🦝",
            label: "Raccoon",
            server_label: "Stoat — localhost:9101",
            base_url: "http://localhost:9101",
            username: "raccoon",
            password: "testpass123",
            authenticate: raccoon_auth,
        },
    ];
    ACCOUNTS
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        StoatSignupPage { on_complete, ctx }
    }
}

/// Full Stoat login form.
#[rustfmt::skip]
#[component]
fn StoatSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut base_url = use_signal(|| OFFICIAL_STOAT_BASE_URL.to_string());
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-stoat-signup-title\")}" }
        p { class: "signup-form-desc", "{t(\"plugin-stoat-signup-description\")}" }

        div { class: "signup-form",
            label { class: "settings-label", "{t(\"plugin-stoat-signup-url-label\")}" }
            input {
                class: "settings-input",
                value: "{base_url}",
                placeholder: "{t(\"plugin-stoat-signup-url-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| base_url.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-stoat-signup-email-label\")}" }
            input {
                class: "settings-input",
                r#type: "email",
                value: "{email}",
                placeholder: "{t(\"plugin-stoat-signup-email-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| email.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-stoat-signup-password-label\")}" }
            input {
                class: "settings-input",
                r#type: "password",
                value: "{password}",
                placeholder: "{t(\"plugin-stoat-signup-password-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| password.set(e.value()),
            }

            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }

            button {
                class: "btn btn-primary",
                disabled: *submitting.read()
                    || base_url.read().trim().is_empty()
                    || email.read().trim().is_empty()
                    || password.read().is_empty(),
                onclick: move |_| {
                    let next_base_url = base_url.read().trim().to_string();
                    let next_email = email.read().trim().to_string();
                    let next_password = password.read().to_string();
                    submitting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match authenticate(next_base_url, next_email, next_password).await {
                            Ok(completed) => on_complete.call(completed),
                            Err(error) => {
                                error_msg.set(Some(error));
                                submitting.set(false);
                            }
                        }
                    });
                },
                if *submitting.read() {
                    "{t(\"plugin-stoat-signup-connecting\")}" 
                } else {
                    "{t(\"plugin-stoat-signup-connect-btn\")}" 
                }
            }
        }
    }
}
