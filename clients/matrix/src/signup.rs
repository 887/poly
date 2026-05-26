//! Matrix signup and test account helpers.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, IsBackend as _, SignupCompleted, SignupContext};
use poly_ui_macros::{context_menu, ui_action};

use crate::MatrixClient;

/// Default homeserver URL pre-filled in the signup form.
const DEFAULT_HOMESERVER: &str = "https://matrix.org";

/// Authenticate against a Matrix homeserver. Public so test panels can call it.
pub async fn authenticate(
    base_url: String,
    username: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let mut backend = MatrixClient::with_homeserver(base_url).map_err(|e| e.to_string())?;
    let session = backend
        .authenticate(AuthCredentials::EmailPassword {
            email: username,
            password,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

fn owl_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

fn axolotl_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

/// Test accounts for the Matrix local dev server (port 9100).
#[must_use]
pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🦉",
            label: "Owl",
            server_label: "Matrix — localhost:9100",
            base_url: "http://localhost:9100",
            username: "owl",
            password: "testpass123",
            backend_slug: "matrix",
            authenticate: owl_auth,
        },
        TestAccountEntry {
            icon: "🦎",
            label: "Axolotl",
            server_label: "Matrix — localhost:9100",
            base_url: "http://localhost:9100",
            username: "axolotl",
            password: "testpass123",
            backend_slug: "matrix",
            authenticate: axolotl_auth,
        },
    ];
    ACCOUNTS
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        MatrixSignupPage { on_complete, ctx }
    }
}

/// Full Matrix login form — homeserver URL + username + password.
#[context_menu(allow_default)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn MatrixSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut base_url = use_signal(|| DEFAULT_HOMESERVER.to_string());
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-matrix-signup-title\")}" }
        p { class: "signup-form-desc", "{t(\"plugin-matrix-signup-description\")}" }

        div { class: "signup-form",
            label { class: "settings-label", "{t(\"plugin-matrix-signup-url-label\")}" }
            input {
                class: "settings-input",
                value: "{base_url}",
                placeholder: "{t(\"plugin-matrix-signup-url-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| base_url.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-matrix-signup-username-label\")}" }
            input {
                class: "settings-input",
                value: "{username}",
                placeholder: "{t(\"plugin-matrix-signup-username-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| username.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-matrix-signup-password-label\")}" }
            input {
                class: "settings-input",
                r#type: "password",
                value: "{password}",
                placeholder: "{t(\"plugin-matrix-signup-password-placeholder\")}",
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
                    || username.read().trim().is_empty()
                    || password.read().is_empty(),
                onclick: move |_| {
                    let next_url = base_url.read().trim().to_string();
                    let next_user = username.read().trim().to_string();
                    let next_pass = password.read().to_string();
                    submitting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match authenticate(next_url, next_user, next_pass).await {
                            Ok(completed) => on_complete.call(completed),
                            Err(error) => {
                                error_msg.set(Some(error));
                                submitting.set(false);
                            }
                        }
                    });
                },
                if *submitting.read() {
                    "{t(\"plugin-matrix-signup-connecting\")}"
                } else {
                    "{t(\"plugin-matrix-signup-connect-btn\")}"
                }
            }
        }
    }
}
