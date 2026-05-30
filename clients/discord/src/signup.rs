//! Discord signup/account-add UI form.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, IsBackend as _, SignupCompleted, SignupContext};


use crate::DiscordClient;
use poly_ui_macros::{context_menu, ui_action};

/// Public authenticate helper — token-based (real Discord + Spacebar with pre-issued tokens).
pub async fn authenticate(
    base_url: String,
    token: String,
) -> Result<SignupCompleted, String> {
    let mut backend = DiscordClient::with_base_url(base_url);
    let session = backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

/// Password-based authenticate helper — used by Spacebar/Fosscord and the local test server.
pub async fn authenticate_with_password(
    base_url: String,
    email: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let mut backend = DiscordClient::with_base_url(base_url);
    let session = backend
        .authenticate(AuthCredentials::EmailPassword { email, password })
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

fn koala_auth(
    u: String,
    email: String,
    password: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate_with_password(u, email, password).await })
}

fn kangaroo_auth(
    u: String,
    email: String,
    password: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate_with_password(u, email, password).await })
}

/// Test accounts for the Discord local dev server (port 9102).
#[must_use]
pub const fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🐨",
            label: "Koala",
            server_label: "Discord — localhost:9102",
            base_url: "http://localhost:9102",
            username: "koala",
            password: "testpass123",
            backend_slug: "discord",
            authenticate:koala_auth,
        },
        TestAccountEntry {
            icon: "🦘",
            label: "Kangaroo",
            server_label: "Discord — localhost:9102",
            base_url: "http://localhost:9102",
            username: "kangaroo",
            password: "testpass123",
            backend_slug: "discord",
            authenticate:kangaroo_auth,
        },
    ];
    ACCOUNTS
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        DiscordSignupPage { on_complete, ctx }
    }
}

#[ui_action(inherit)]
#[context_menu(allow_default)]
/// Discord account setup form (token-based auth).
#[component]
fn DiscordSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
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
