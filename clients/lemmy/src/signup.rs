//! Lemmy signup / login page component — `/signup/lemmy`.
//!
//! Renders the host-facing login UI for the native Lemmy backend.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::LemmyClient;

/// Authenticate against a Lemmy instance.  Public so test panels can call it.
pub async fn authenticate(
    base_url: String,
    username: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let mut backend = LemmyClient::new(base_url);
    let session = backend
        .authenticate(AuthCredentials::EmailPassword {
            email: username,
            password,
        })
        .await
        .map_err(|e| e.to_string())?;

    Ok(SignupCompleted {
        session,
        backend: Box::new(backend),
    })
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        LemmySignupPage { on_complete, ctx }
    }
}

/// Full Lemmy login form.
#[rustfmt::skip]
#[component]
fn LemmySignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut base_url = use_signal(|| "https://lemmy.ml".to_string());
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-lemmy-signup-title\")}" }
        p { class: "signup-form-desc", "{t(\"plugin-lemmy-signup-description\")}" }

        div { class: "signup-form",
            button {
                class: "signup-nav-back",
                disabled: *submitting.read(),
                onclick: move |_| (ctx.navigate_back)(),
                "{t(\"plugin-lemmy-signup-back\")}"
            }

            label { class: "settings-label", "{t(\"plugin-lemmy-signup-url-label\")}" }
            input {
                class: "settings-input",
                value: "{base_url}",
                placeholder: "{t(\"plugin-lemmy-signup-url-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| base_url.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-lemmy-signup-username-label\")}" }
            input {
                class: "settings-input",
                value: "{username}",
                placeholder: "{t(\"plugin-lemmy-signup-username-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| username.set(e.value()),
            }

            label { class: "settings-label", "{t(\"plugin-lemmy-signup-password-label\")}" }
            input {
                class: "settings-input",
                r#type: "password",
                value: "{password}",
                placeholder: "{t(\"plugin-lemmy-signup-password-placeholder\")}",
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
                    "{t(\"plugin-lemmy-signup-connecting\")}"
                } else {
                    "{t(\"plugin-lemmy-signup-connect-btn\")}"
                }
            }
        }
    }
}
