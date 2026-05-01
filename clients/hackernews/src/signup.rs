//! Hacker News signup page component.
//!
//! Two modes:
//! - **Anonymous (read-only)** — no credentials needed; browse top stories,
//!   Ask HN, Show HN, jobs. Posting is disabled.
//! - **Sign in** — POST to `news.ycombinator.com/login` with username +
//!   password (see `auth::login`). On success the returned `user=…` cookie
//!   is stored in `Session.token` and used for comment / submit POSTs.
//!   Multiple accounts are supported — the host's `ClientManager` spawns
//!   one `HackerNewsClient` per account.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend, SignupCompleted, SignupContext};

use crate::HackerNewsClient;
use poly_ui_macros::{context_menu, ui_action};

/// Build a guest SignupCompleted immediately (no network call needed).
#[must_use]
pub fn complete_as_guest() -> SignupCompleted {
    let mut backend = HackerNewsClient::new();
    let session = backend.guest_session();
    SignupCompleted::new(session, Box::new(backend))
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        HackerNewsSignupPage { on_complete, ctx }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum HnMode {
    Anonymous,
    SignIn,
}

#[ui_action(inherit)]
#[context_menu(allow_default)]
#[rustfmt::skip]
#[component]
fn HackerNewsSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut mode = use_signal(|| HnMode::Anonymous);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-hackernews-signup-title\")}" }

        div { class: "signup-tabs",
            button {
                class: if *mode.read() == HnMode::Anonymous { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| {
                    mode.set(HnMode::Anonymous);
                    error.set(None);
                },
                "{t(\"plugin-hackernews-signup-tab-anonymous\")}"
            }
            button {
                class: if *mode.read() == HnMode::SignIn { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| {
                    mode.set(HnMode::SignIn);
                    error.set(None);
                },
                "{t(\"plugin-hackernews-signup-tab-signin\")}"
            }
        }

        div { class: "signup-form",
            if *mode.read() == HnMode::Anonymous {
                p { class: "signup-form-desc", "{t(\"plugin-hackernews-signup-anonymous-desc\")}" }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| on_complete.call(complete_as_guest()),
                    "{t(\"plugin-hackernews-signup-anonymous-btn\")}"
                }
            } else {
                p { class: "signup-form-desc", "{t(\"plugin-hackernews-signup-signin-desc\")}" }
                label { class: "settings-label", "{t(\"plugin-hackernews-signup-username-label\")}" }
                input {
                    class: "settings-input",
                    r#type: "text",
                    value: "{username}",
                    placeholder: "{t(\"plugin-hackernews-signup-username-placeholder\")}",
                    autocomplete: "username",
                    oninput: move |e: Event<FormData>| username.set(e.value()),
                }
                label { class: "settings-label", "{t(\"plugin-hackernews-signup-password-label\")}" }
                input {
                    class: "settings-input",
                    r#type: "password",
                    value: "{password}",
                    placeholder: "{t(\"plugin-hackernews-signup-password-placeholder\")}",
                    autocomplete: "current-password",
                    oninput: move |e: Event<FormData>| password.set(e.value()),
                }
                if let Some(err) = error.read().clone() {
                    p { class: "signup-form-error", "{err}" }
                }
                button {
                    class: "btn btn-primary",
                    disabled: *submitting.read()
                        || username.read().trim().is_empty()
                        || password.read().is_empty(),
                    onclick: move |_| {
                        let uname = username.read().trim().to_string();
                        let pw = password.read().clone();
                        submitting.set(true);
                        error.set(None);
                        spawn(async move {
                            let mut backend = HackerNewsClient::new();
                            let result = backend
                                .authenticate(AuthCredentials::EmailPassword {
                                    email: uname,
                                    password: pw,
                                })
                                .await;
                            submitting.set(false);
                            match result {
                                Ok(session) => {
                                    on_complete.call(SignupCompleted::new(
                                        session,
                                        Box::new(backend),
                                    ));
                                }
                                Err(e) => error.set(Some(e.to_string())),
                            }
                        });
                    },
                    if *submitting.read() {
                        "{t(\"plugin-hackernews-signup-signing-in\")}"
                    } else {
                        "{t(\"plugin-hackernews-signup-signin-btn\")}"
                    }
                }
            }
        }
    }
}
