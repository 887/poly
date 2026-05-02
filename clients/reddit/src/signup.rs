//! Reddit signup / login page — `/signup/reddit`.
//!
//! Phase A scaffold: placeholder UI. Real signup form (username/password +
//! bring-your-own-cookie path for 2FA accounts) ships in Phase G of
//! `docs/plans/plan-reddit-stub.md`.

use dioxus::prelude::*;
use poly_client::{SignupCompleted, SignupContext};

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(_on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-reddit-signup-title\")}" }
        p { class: "signup-form-desc", "{t(\"plugin-reddit-signup-description\")}" }
        p { class: "signup-form-desc",
            "Reddit signup not yet implemented. See docs/plans/plan-reddit-stub.md Phase G."
        }
    }
}
