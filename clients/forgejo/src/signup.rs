//! Forgejo signup page component — `/signup/forgejo`.
//!
//! Two modes: Codeberg (pre-filled) and custom Forgejo/Gitea instance.
//! Both modes authenticate with a personal access token.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, IsBackend as _, SignupCompleted, SignupContext};

use poly_ui_macros::{context_menu, ui_action};

use crate::ForgejoClient;

/// Authenticate against a Forgejo/Gitea instance with a personal access token.
pub async fn authenticate(
    instance_url: String,
    token: String,
) -> Result<SignupCompleted, String> {
    let mut backend = ForgejoClient::new(&instance_url);
    let session = backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

/// Authenticate against a Forgejo test server using the `/test/auth/token` bypass.
///
/// The test server accepts a username and returns an opaque token; that token
/// is then used with the normal [`authenticate`] path.
pub async fn test_authenticate(
    instance_url: String,
    username: String,
    _password: String,
) -> Result<poly_client::SignupCompleted, String> {
    use poly_host_bridge::http::HttpClient;
    let http = HttpClient::new();
    let resp = http
        .post(format!("{instance_url}/test/auth/token"))
        .header("Content-Type", "application/json")
        .body(format!(r#"{{"username":"{username}"}}"#))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = body
        .get("token")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "no token in response".to_string())?
        .to_string();
    authenticate(instance_url, token).await
}

fn otter_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { test_authenticate(u, e, p).await })
}

fn flamingo_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { test_authenticate(u, e, p).await })
}

/// Test accounts for the Forgejo local dev server (port 9106).
#[must_use] 
pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🦦",
            label: "Otter",
            server_label: "Forgejo — localhost:9106",
            base_url: "http://localhost:9106",
            username: "otter",
            password: "testpass123",
            backend_slug: "forgejo",
            authenticate: otter_auth,
        },
        TestAccountEntry {
            icon: "🦩",
            label: "Flamingo",
            server_label: "Forgejo — localhost:9106",
            base_url: "http://localhost:9106",
            username: "flamingo",
            password: "testpass123",
            backend_slug: "forgejo",
            authenticate: flamingo_auth,
        },
    ];
    ACCOUNTS
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        ForgejoSignupPage { on_complete, ctx }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FjMode {
    Codeberg,
    Custom,
}

#[ui_action(inherit)]
#[context_menu(allow_default)]
#[rustfmt::skip]
#[component]
fn ForgejoSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut mode = use_signal(|| FjMode::Codeberg);
    let mut instance_url = use_signal(|| "https://codeberg.org".to_string());
    let mut token = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-forgejo-signup-title\")}" }

        div { class: "signup-tabs",
            button {
                class: if *mode.read() == FjMode::Codeberg { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| {
                    mode.set(FjMode::Codeberg);
                    instance_url.set("https://codeberg.org".to_string());
                },
                "{t(\"plugin-forgejo-signup-tab-codeberg\")}"
            }
            button {
                class: if *mode.read() == FjMode::Custom { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| mode.set(FjMode::Custom),
                "{t(\"plugin-forgejo-signup-tab-custom\")}"
            }
        }

        div { class: "signup-form",
            if *mode.read() == FjMode::Codeberg {
                p { class: "signup-form-desc", "{t(\"plugin-forgejo-signup-codeberg-desc\")}" }
            } else {
                p { class: "signup-form-desc", "{t(\"plugin-forgejo-signup-custom-desc\")}" }
                label { class: "settings-label", "{t(\"plugin-forgejo-signup-instance-label\")}" }
                input {
                    class: "settings-input",
                    value: "{instance_url}",
                    placeholder: "{t(\"plugin-forgejo-signup-instance-placeholder\")}",
                    disabled: *submitting.read(),
                    oninput: move |e: Event<FormData>| instance_url.set(e.value()),
                }
            }

            label { class: "settings-label", "{t(\"plugin-forgejo-signup-token-label\")}" }
            input {
                class: "settings-input",
                r#type: "password",
                value: "{token}",
                placeholder: "{t(\"plugin-forgejo-signup-token-placeholder\")}",
                disabled: *submitting.read(),
                oninput: move |e: Event<FormData>| token.set(e.value()),
            }

            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }

            button {
                class: "btn btn-primary",
                disabled: *submitting.read()
                    || token.read().trim().is_empty()
                    || (*mode.read() == FjMode::Custom && instance_url.read().trim().is_empty()),
                onclick: move |_| {
                    let url = instance_url.read().trim().to_string();
                    let tok = token.read().trim().to_string();
                    submitting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match authenticate(url, tok).await {
                            Ok(completed) => on_complete.call(completed),
                            Err(error) => {
                                error_msg.set(Some(error));
                                submitting.set(false);
                            }
                        }
                    });
                },
                if *submitting.read() {
                    "{t(\"plugin-forgejo-signup-connecting\")}"
                } else {
                    "{t(\"plugin-forgejo-signup-connect-btn\")}"
                }
            }
        }
    }
}
