//! Microsoft Teams signup/account-add UI form.

use dioxus::prelude::*;
use poly_client::{AuthCredentials, ClientBackend as _, SignupCompleted, SignupContext};

use crate::TeamsClient;
use crate::auth::{self, DeviceCodeResponse, TokenResponse};
use poly_ui_macros::context_menu;

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
    Ok(SignupCompleted::new(session, Box::new(backend)))
}

/// Complete a Microsoft Graph OAuth sign-in — takes a fresh `TokenResponse`
/// from the device-code or PKCE flow and threads `refresh_token`, expiry,
/// and scope into `SignupCompleted` for persistence.
pub async fn authenticate_oauth(
    base_url: String,
    tokens: TokenResponse,
) -> Result<SignupCompleted, String> {
    let mut backend = TeamsClient::with_base_url(base_url);
    let session = backend
        .authenticate(AuthCredentials::OAuth {
            token: tokens.access_token.clone(),
        })
        .await
        .map_err(|e| e.to_string())?;
    let expires_at = chrono::Utc::now()
        + chrono::Duration::seconds(tokens.expires_in.min(i64::MAX as u64) as i64);
    Ok(SignupCompleted {
        session,
        backend: Box::new(backend),
        refresh_token: tokens.refresh_token,
        token_expires_at: Some(expires_at.to_rfc3339()),
        scope: tokens.scope,
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
    Ok(SignupCompleted::new(session, Box::new(backend)))
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

#[derive(Clone, Copy, PartialEq)]
enum TeamsTab {
    Microsoft,
    Token,
}

#[context_menu(inherit)]
/// Teams account setup form — Microsoft OAuth (device code) or raw Bearer token.
#[component]
fn TeamsSignupPage(on_complete: Callback<SignupCompleted>, ctx: SignupContext) -> Element {
    let _t = ctx.t;
    let tab = use_signal(|| TeamsTab::Microsoft);

    rsx! {
        h2 { class: "signup-form-title", "Add Microsoft Teams Account" }

        div { class: "signup-tabs",
            button {
                class: if *tab.read() == TeamsTab::Microsoft { "signup-tab active" } else { "signup-tab" },
                onclick: {
                    let mut tab = tab;
                    move |_| tab.set(TeamsTab::Microsoft)
                },
                "Microsoft account"
            }
            button {
                class: if *tab.read() == TeamsTab::Token { "signup-tab active" } else { "signup-tab" },
                onclick: {
                    let mut tab = tab;
                    move |_| tab.set(TeamsTab::Token)
                },
                "Access token"
            }
        }

        if *tab.read() == TeamsTab::Microsoft {
            TeamsOAuthTab { on_complete }
        } else {
            TeamsTokenTab { on_complete }
        }
    }
}

#[context_menu(inherit)]
/// Raw Bearer-token sign-in path (kept for testing / dev scenarios).
#[component]
fn TeamsTokenTab(on_complete: Callback<SignupCompleted>) -> Element {
    let mut token = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        div { class: "signup-form",
            p { class: "signup-form-desc",
                "Paste a Microsoft Graph Bearer token to connect directly."
            }
            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }
            label { class: "settings-label", "Access Token" }
            input {
                class: "settings-input",
                r#type: "password",
                placeholder: "Microsoft Graph Bearer token",
                value: "{token}",
                disabled: *submitting.read(),
                oninput: move |e| token.set(e.value()),
            }
            button {
                class: "btn btn-primary",
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

#[context_menu(inherit)]
/// Device-code sign-in against `login.microsoftonline.com`.
#[component]
fn TeamsOAuthTab(on_complete: Callback<SignupCompleted>) -> Element {
    let mut code: Signal<Option<DeviceCodeResponse>> = use_signal(|| None);
    let mut starting = use_signal(|| false);
    let mut polling = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| None);

    rsx! {
        div { class: "signup-form",
            p { class: "signup-form-desc",
                "Sign in with your Microsoft work, school, or personal account. A short code will appear — open the link and enter it to authorize Poly."
            }
            if let Some(err) = error_msg.read().as_ref() {
                p { class: "settings-error", "{err}" }
            }

            if let Some(dc) = code.read().as_ref() {
                div { class: "signup-device-code",
                    p { "1. Open "
                        a { href: "{dc.verification_uri}", target: "_blank", "{dc.verification_uri}" }
                    }
                    p { "2. Enter this code:" }
                    div { class: "settings-input",
                        style: "font-family: monospace; font-size: 1.5rem; letter-spacing: 0.2em; text-align: center;",
                        "{dc.user_code}"
                    }
                    if *polling.read() {
                        p { class: "signup-form-desc", "Waiting for you to finish sign-in in the browser…" }
                    }
                }
            } else {
                button {
                    class: "btn btn-primary",
                    disabled: *starting.read(),
                    onclick: move |_| {
                        starting.set(true);
                        error_msg.set(None);
                        spawn(async move {
                            match auth::start_device_code(
                                auth::DEFAULT_TENANT,
                                auth::DEFAULT_CLIENT_ID,
                                auth::DEFAULT_SCOPES,
                            )
                            .await
                            {
                                Ok(dc) => {
                                    let device_code = dc.device_code.clone();
                                    let interval = dc.interval.max(1);
                                    let expires_in = dc.expires_in;
                                    code.set(Some(dc));
                                    polling.set(true);
                                    starting.set(false);
                                    let started = std::time::Instant::now();
                                    loop {
                                        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                                        if started.elapsed().as_secs() > expires_in {
                                            error_msg.set(Some("Sign-in code expired — try again.".into()));
                                            polling.set(false);
                                            code.set(None);
                                            return;
                                        }
                                        match auth::poll_device_code_token(
                                            auth::DEFAULT_TENANT,
                                            auth::DEFAULT_CLIENT_ID,
                                            &device_code,
                                        )
                                        .await
                                        {
                                            Ok(Some(tokens)) => {
                                                match authenticate_oauth(
                                                    "https://graph.microsoft.com".to_string(),
                                                    tokens,
                                                )
                                                .await
                                                {
                                                    Ok(completed) => {
                                                        on_complete.call(completed);
                                                    }
                                                    Err(e) => {
                                                        error_msg.set(Some(e));
                                                        polling.set(false);
                                                        code.set(None);
                                                    }
                                                }
                                                return;
                                            }
                                            Ok(None) => continue,
                                            Err(e) => {
                                                error_msg.set(Some(e.to_string()));
                                                polling.set(false);
                                                code.set(None);
                                                return;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error_msg.set(Some(e.to_string()));
                                    starting.set(false);
                                }
                            }
                        });
                    },
                    if *starting.read() { "Starting…" } else { "Sign in with Microsoft" }
                }
            }
        }
    }
}
