//! Microsoft Teams signup/account-add UI form.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::TeamsClient;

/// Public authenticate helper — called by the form and integration tests.
pub async fn authenticate(
    base_url: String,
    token: String,
) -> Result<SignupCompleted, String> {
    let mut backend = TeamsClient::with_base_url(base_url);
    let session = backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted {
        session,
        backend: Box::new(backend),
    })
}

/// Password-based authenticate helper — used by the local test server.
pub async fn authenticate_with_password(
    base_url: String,
    email: String,
    password: String,
) -> Result<SignupCompleted, String> {
    let mut backend = TeamsClient::with_base_url(base_url);
    let session = backend
        .authenticate(AuthCredentials::EmailPassword { email, password })
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted {
        session,
        backend: Box::new(backend),
    })
}

fn sheep_auth(
    u: String,
    email: String,
    password: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate_with_password(u, email, password).await })
}

fn walrus_auth(
    u: String,
    email: String,
    password: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { authenticate_with_password(u, email, password).await })
}

/// Test accounts for the Teams local dev server (port 9103).
pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🐑",
            label: "Sheep",
            server_label: "Teams — localhost:9103",
            base_url: "http://localhost:9103",
            username: "sheep@contoso.com",
            password: "testpass123",
            authenticate: sheep_auth,
        },
        TestAccountEntry {
            icon: "🦭",
            label: "Walrus",
            server_label: "Teams — localhost:9103",
            base_url: "http://localhost:9103",
            username: "walrus@contoso.com",
            password: "testpass123",
            authenticate: walrus_auth,
        },
    ];
    ACCOUNTS
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        TeamsSignupPage { on_complete, ctx }
    }
}

/// Teams account setup form (OAuth Bearer token).
#[component]
fn TeamsSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let _t = ctx.t;
    let mut token = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        div { class: "signup-form",
            div { class: "signup-header",
                h2 { "Add Microsoft Teams Account" }
                p { class: "signup-note",
                    "Enter your Microsoft Teams access token to connect."
                }
            }
            if let Some(err) = error_msg.read().as_ref() {
                div { class: "signup-error", "{err}" }
            }
            div { class: "signup-field",
                label { "Access Token" }
                input {
                    r#type: "password",
                    placeholder: "Microsoft Teams Bearer token",
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
                            match authenticate("https://graph.microsoft.com".to_string(), token_val).await {
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
