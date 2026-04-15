//! Hacker News signup page component.
//!
//! Supports two modes: anonymous (no credentials needed) and named (enter your
//! HN username to personalize the session). HN is read-only in both cases.

use dioxus::prelude::*;
use poly_client::{SignupCompleted, SignupContext};

use crate::HackerNewsClient;

/// Build a guest SignupCompleted immediately (no network call needed).
pub fn complete_as_guest() -> SignupCompleted {
    let mut backend = HackerNewsClient::new();
    let session = backend.guest_session();
    SignupCompleted::new(session, Box::new(backend))
}

/// Build a named SignupCompleted with a given HN username.
pub fn complete_as_user(username: String) -> SignupCompleted {
    let mut backend = HackerNewsClient::new();
    let session = backend.named_session(username);
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
    Named,
}

/// Two-mode HN signup page: anonymous or with username.
#[rustfmt::skip]
#[component]
fn HackerNewsSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut mode = use_signal(|| HnMode::Anonymous);
    let mut username = use_signal(String::new);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-hackernews-signup-title\")}" }

        div { class: "signup-tabs",
            button {
                class: if *mode.read() == HnMode::Anonymous { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| mode.set(HnMode::Anonymous),
                "{t(\"plugin-hackernews-signup-tab-anonymous\")}"
            }
            button {
                class: if *mode.read() == HnMode::Named { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| mode.set(HnMode::Named),
                "{t(\"plugin-hackernews-signup-tab-account\")}"
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
                p { class: "signup-form-desc", "{t(\"plugin-hackernews-signup-account-desc\")}" }
                label { class: "settings-label", "{t(\"plugin-hackernews-signup-username-label\")}" }
                input {
                    class: "settings-input",
                    value: "{username}",
                    placeholder: "{t(\"plugin-hackernews-signup-username-placeholder\")}",
                    oninput: move |e: Event<FormData>| username.set(e.value()),
                }
                button {
                    class: "btn btn-primary",
                    disabled: username.read().trim().is_empty(),
                    onclick: move |_| {
                        let uname = username.read().trim().to_string();
                        on_complete.call(complete_as_user(uname));
                    },
                    "{t(\"plugin-hackernews-signup-account-btn\")}"
                }
            }
        }
    }
}
