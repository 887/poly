//! Discord signup/account-add UI form.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::DiscordClient;

/// Public authenticate helper — called by the form and integration tests.
pub async fn authenticate(
    base_url: String,
    token: String,
) -> Result<SignupCompleted, String> {
    let mut backend = DiscordClient::with_base_url(base_url);
    let session = backend
        .authenticate(AuthCredentials::Token(token))
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
        DiscordSignupPage { on_complete, ctx }
    }
}

/// Discord account setup form (token-based auth).
#[component]
fn DiscordSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let _t = ctx.t;
    let mut token = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        div { class: "signup-form",
            div { class: "signup-header",
                h2 { "Add Discord Account" }
                p { class: "signup-note",
                    "Enter your Discord user token to connect."
                }
            }
            if let Some(err) = error_msg.read().as_ref() {
                div { class: "signup-error", "{err}" }
            }
            div { class: "signup-field",
                label { "User Token" }
                input {
                    r#type: "password",
                    placeholder: "Discord user token",
                    value: "{token}",
                    disabled: *submitting.read(),
                    oninput: move |e| token.set(e.value()),
                }
            }
            div { class: "signup-actions",
                button {
                    class: "btn-primary",
                    disabled: *submitting.read() || token.read().is_empty(),
                    onclick: move |_| {
                        let token_val = token.read().to_string();
                        submitting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match authenticate("https://discord.com".to_string(), token_val).await {
                                Ok(completed) => on_complete.call(completed),
                                Err(e) => {
                                    error_msg.set(Some(e));
                                    submitting.set(false);
                                }
                            }
                        });
                    },
                    "Connect"
                }
            }
        }
    }
}
