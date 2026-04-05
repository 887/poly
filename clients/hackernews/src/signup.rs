//! Hacker News signup page component.
//!
//! HN is read-only and requires no credentials. The signup flow is a single
//! "Add Hacker News" button that immediately creates a guest session.

use dioxus::prelude::*;
use poly_client::{SignupCompleted, SignupContext};

use crate::HackerNewsClient;

/// Build a guest SignupCompleted immediately (no network call needed).
pub fn complete_as_guest() -> SignupCompleted {
    let mut backend = HackerNewsClient::new();
    let session = backend.guest_session();
    SignupCompleted {
        session,
        backend: Box::new(backend),
    }
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        HackerNewsSignupPage { on_complete, ctx }
    }
}

/// One-click "Add Hacker News" signup page.
#[component]
fn HackerNewsSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    rsx! {
        h2 { class: "signup-form-title", "Add Hacker News" }
        p { class: "signup-form-desc",
            "Browse Hacker News stories and comments. No account required."
        }

        div { class: "signup-form",
            button {
                class: "signup-nav-back",
                onclick: move |_| (ctx.navigate_back)(),
                "{t(\"plugin-hackernews-signup-back\")}"
            }

            p {
                "Hacker News is a read-only feed. Click below to start browsing the top stories, Ask HN, Show HN, and job posts."
            }

            button {
                class: "btn btn-primary",
                onclick: move |_| {
                    on_complete.call(complete_as_guest());
                },
                "Add Hacker News"
            }
        }
    }
}
