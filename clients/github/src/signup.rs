//! GitHub signup page component — `/signup/github`.
//!
//! Two modes: github.com (default) and GitHub Enterprise (custom hostname).
//! Both modes delegate auth to the user's local `gh` CLI; we just check that
//! `gh auth status` succeeds for the chosen instance.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};
use poly_ui_macros::context_menu;

use crate::GitHubClient;

/// Authenticate against a GitHub test server using the `/test/auth/token` bypass.
pub async fn test_authenticate(
    base_url: String,
    username: String,
    _password: String,
) -> Result<SignupCompleted, String> {
    use poly_host_bridge::http::HttpClient;
    let http = HttpClient::new();
    let resp = http
        .post(format!("{}/test/auth/token", base_url))
        .header("Content-Type", "application/json")
        .body(format!(r#"{{"username":"{}"}}"#, username))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = body
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or_else(|| "no token in response".to_string())?
        .to_string();

    let mut backend = GitHubClient::with_http(&base_url);
    let session = backend
        .authenticate(AuthCredentials::Token(token))
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

fn penguin_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { test_authenticate(u, e, p).await })
}

fn chameleon_auth(
    u: String,
    e: String,
    p: String,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<poly_client::SignupCompleted, String>>>,
> {
    Box::pin(async move { test_authenticate(u, e, p).await })
}

/// Test accounts for the GitHub local dev server (port 9107).
pub fn get_test_accounts() -> &'static [poly_client::TestAccountEntry] {
    use poly_client::TestAccountEntry;
    const ACCOUNTS: &[TestAccountEntry] = &[
        TestAccountEntry {
            icon: "🐧",
            label: "Penguin",
            server_label: "GitHub — localhost:9107",
            base_url: "http://localhost:9107",
            username: "penguin",
            password: "testpass123",
            authenticate: penguin_auth,
        },
        TestAccountEntry {
            icon: "🦎",
            label: "Chameleon",
            server_label: "GitHub — localhost:9107",
            base_url: "http://localhost:9107",
            username: "chameleon",
            password: "testpass123",
            authenticate: chameleon_auth,
        },
    ];
    ACCOUNTS
}

/// Run `gh api /user` against the chosen instance and build a session.
pub async fn authenticate(hostname: Option<String>) -> Result<SignupCompleted, String> {
    let mut backend = match hostname {
        Some(host) if !host.is_empty() => GitHubClient::enterprise(host),
        _ => GitHubClient::dotcom(),
    };
    let session = backend
        .authenticate(AuthCredentials::Token(String::new()))
        .await
        .map_err(|e| e.to_string())?;
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

/// Render entry-point stored in `SignupEntry::render`.
pub fn signup_render_fn(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    rsx! {
        GitHubSignupPage { on_complete, ctx }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum GhMode {
    Dotcom,
    Enterprise,
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn GitHubSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let t = ctx.t;
    let mut mode = use_signal(|| GhMode::Dotcom);
    let mut hostname = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        h2 { class: "signup-form-title", "{t(\"plugin-github-signup-title\")}" }

        div { class: "signup-tabs",
            button {
                class: if *mode.read() == GhMode::Dotcom { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| mode.set(GhMode::Dotcom),
                "{t(\"plugin-github-signup-tab-dotcom\")}"
            }
            button {
                class: if *mode.read() == GhMode::Enterprise { "signup-tab active" } else { "signup-tab" },
                onclick: move |_| mode.set(GhMode::Enterprise),
                "{t(\"plugin-github-signup-tab-enterprise\")}"
            }
        }

        div { class: "signup-form",
            if *mode.read() == GhMode::Dotcom {
                p { class: "signup-form-desc", "{t(\"plugin-github-signup-dotcom-desc\")}" }
            } else {
                p { class: "signup-form-desc", "{t(\"plugin-github-signup-enterprise-desc\")}" }
                label { class: "settings-label", "{t(\"plugin-github-signup-hostname-label\")}" }
                input {
                    class: "settings-input",
                    value: "{hostname}",
                    placeholder: "{t(\"plugin-github-signup-hostname-placeholder\")}",
                    disabled: *submitting.read(),
                    oninput: move |e: Event<FormData>| hostname.set(e.value()),
                }
            }

            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }

            button {
                class: "btn btn-primary",
                disabled: *submitting.read()
                    || (*mode.read() == GhMode::Enterprise && hostname.read().trim().is_empty()),
                onclick: move |_| {
                    let host = if *mode.read() == GhMode::Enterprise {
                        Some(hostname.read().trim().to_string())
                    } else {
                        None
                    };
                    submitting.set(true);
                    error_msg.set(None);
                    spawn(async move {
                        match authenticate(host).await {
                            Ok(completed) => on_complete.call(completed),
                            Err(error) => {
                                error_msg.set(Some(error));
                                submitting.set(false);
                            }
                        }
                    });
                },
                if *submitting.read() {
                    "{t(\"plugin-github-signup-connecting\")}"
                } else {
                    "{t(\"plugin-github-signup-connect-btn\")}"
                }
            }
        }
    }
}
