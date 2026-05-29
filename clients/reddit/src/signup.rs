//! Reddit signup / login page — `/signup/reddit`.
//!
//! Renders the host-facing login UI for the native Reddit backend.
//! Supports username+password login (test backend) and session-cookie
//! login (production path via bring-your-own-cookie).

use dioxus::prelude::*;
use poly_client::{AuthCredentials, IsBackend as _, SignupCompleted, SignupContext};

use poly_ui_macros::{context_menu, ui_action};

use crate::backend::RedditBackend;
use crate::RedditClient;

/// Authenticate against a Reddit instance using username + password.
///
/// Public so test panels can call it.
pub async fn authenticate(
    base_url: String,
    username: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let client = RedditClient::with_base_url(base_url).map_err(|e| e.to_string())?;
    let mut backend = RedditBackend::new(client);
    let session = backend
        .authenticate(AuthCredentials::EmailPassword {
            email: username,
            password,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

fn cat_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

fn dog_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate(u, e, p).await })
}

/// Test accounts for the Reddit local dev server (port 9108).
#[must_use]
pub const fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🐱",
            label: "Cat",
            server_label: "Reddit — localhost:9108",
            base_url: "http://localhost:9108",
            username: "cat",
            password: "testpass123",
            backend_slug: "reddit",
            authenticate: cat_auth,
        },
        TestAccountEntry {
            icon: "🐶",
            label: "Dog",
            server_label: "Reddit — localhost:9108",
            base_url: "http://localhost:9108",
            username: "dog",
            password: "testpass123",
            backend_slug: "reddit",
            authenticate: dog_auth,
        },
    ];
    ACCOUNTS
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        RedditSignupPage { on_complete, ctx }
    }
}

/// Full Reddit login form — username + password.
#[context_menu(allow_default)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn RedditSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut base_url = use_signal(|| "https://old.reddit.com".to_string());
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-reddit-signup-title\")}" }
        p { class: "signup-form-desc", "{t(\"plugin-reddit-signup-description\")}" }

        div { class: "signup-form",
            label { class: "settings-label", "{t(\"plugin-reddit-signup-url-label\")}" }
            input {
                class: "settings-input",
                value: "{base_url}",
                placeholder: "https://old.reddit.com",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| base_url.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-reddit-signup-username-label\")}" }
            input {
                class: "settings-input",
                value: "{username}",
                placeholder: "{t(\"plugin-reddit-signup-username-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| username.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-reddit-signup-password-label\")}" }
            input {
                class: "settings-input",
                r#type: "password",
                value: "{password}",
                placeholder: "{t(\"plugin-reddit-signup-password-placeholder\")}",
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
                    let next_base_url = base_url.read().trim().to_string();
                    let next_username = username.read().trim().to_string();
                    let next_password = password.read().to_string();
                    submitting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match authenticate(next_base_url, next_username, next_password).await {
                            Ok(completed) => on_complete.call(completed),
                            Err(error) => {
                                error_msg.set(Some(error));
                                submitting.set(false);
                            }
                        }
                    });
                },
                if *submitting.read() {
                    "{t(\"plugin-reddit-signup-connecting\")}"
                } else {
                    "{t(\"plugin-reddit-signup-connect-btn\")}"
                }
            }
        }
    }
}
