//! Account signup flow — full-page, outside MainLayout.
//!
//! Rendered at `/signup` (backend picker) and `/signup/:client` (per-backend form).
//!
//! ## Layout
//!
//! Uses a three-panel layout matching the Settings page structure:
//! - **Left sidebar** (`server-sidebar` / FavoritesBar) — account icons, server favorites, app settings
//! - **Middle sidebar** (`signup-nav`) — "← Back", "Add Account" title, backend list
//! - **Right panel** (`signup-content`) — selected backend's form, or a placeholder
//!
//! Both `/signup` and `/signup/:client` render the same three-panel shell.
//! The selected client slug drives which backend form is shown on the right.
//! The FavoritesBar is always visible, giving users context of their accounts
//! and servers while adding a new account.
//!
//! ## Architecture
//!
//! The signup flow is **plugin-driven**: core has zero compile-time knowledge of
//! which backends exist.  Each compiled-in or WASM plugin registers a
//! [`SignupEntry`](crate::client_manager::SignupEntry) at startup via
//! [`ClientManager::register_signup_entry`].
//!
//! ## Adding a new backend
//! 1. Define `fn my_render(Callback<SignupCompleted>, SignupContext) -> Element`
//!    in the client crate.
//! 2. Call `client_manager.register_signup_entry(SignupEntry { slug: "my", ..., render: my_render })`.
//! 3. Add FTL keys in the plugin-owned `locales/` directory.
//!
//! ## 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines.

use crate::client_manager::{BackendHandle, ClientManager};
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::favorites_sidebar::FavoritesBar;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{SignupCompleted, SignupContext};
use std::collections::HashMap;
use std::sync::Arc;

// ── Shared signup-commit callback builder ───────────────────────────────────

/// Build the `on_complete` callback that commits a newly authenticated backend
/// account into the app state (Signal writes, server cache, navigation).
///
/// Used by both the normal per-backend signup flow and the quick test-account panel.
fn build_on_complete(
    mut client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
) -> Callback<SignupCompleted> {
    Callback::new(move |completed: SignupCompleted| {
        let backend_handle: BackendHandle =
            Arc::new(tokio::sync::RwLock::new(completed.backend));
        let session = completed.session;
        spawn(async move {
            let backend_slug = session.backend.slug().to_string();
            let account_id  = session.id.clone();
            let instance_id = session.instance_id.clone();
            let display_name = session.user.display_name.clone();

            // Persist the account token so it survives app restarts.
            if let Some(storage) = crate::STORAGE.get() {
                let at = crate::storage::AccountToken {
                    backend: backend_slug.clone(),
                    account_id: account_id.clone(),
                    token: session.token.clone(),
                    display_name: session.user.display_name.clone(),
                    instance_id: session.backend_url.clone(),
                };
                if let Err(e) = storage.upsert_account_token(&at).await {
                    tracing::warn!("Failed to persist backend account token: {e}");
                }
            }

            // Build server→account map (async, no Signal lock held).
            let mut server_map = HashMap::new();
            {
                let guard = backend_handle.read().await;
                if let Ok(servers) = guard.get_servers().await {
                    for srv in &servers {
                        server_map.insert(srv.id.clone(), account_id.clone());
                    }
                }
            }
            // Phase 2: sync Signal writes — no await while lock is held.
            client_manager.write().commit_backend_account(
                account_id.clone(),
                session.clone(),
                backend_handle.clone(),
                server_map,
            );
            chat_data.write().account_sessions.insert(account_id.clone(), session);
            // Phase 3: async data load — no Signal lock held during awaits.
            {
                let guard = backend_handle.read().await;
                if let Ok(servers) = guard.get_servers().await {
                    let cache_records: Vec<crate::storage::OfflineServerRecord> =
                        servers
                            .iter()
                            .map(|srv| crate::storage::OfflineServerRecord {
                                id: srv.id.clone(),
                                name: srv.name.clone(),
                                icon_url: srv.icon_url.clone(),
                                banner_url: srv.banner_url.clone(),
                                backend: backend_slug.clone(),
                                account_id: account_id.clone(),
                                account_display_name: display_name.clone(),
                            })
                            .collect();
                    let new_ids: Vec<String> =
                        servers.iter().map(|s| s.id.clone()).collect();

                    let mut cd = chat_data.write();
                    for id in &new_ids {
                        if !cd.favorited_server_ids.contains(id) {
                            cd.favorited_server_ids.push(id.clone());
                        }
                    }
                    cd.servers.extend(servers);
                    let all_fav_ids = cd.favorited_server_ids.clone();
                    drop(cd);

                    if let Some(storage) = crate::STORAGE.get()
                        && let Err(e) =
                            storage.upsert_offline_server_cache(&cache_records).await
                    {
                        tracing::warn!("Failed to cache server metadata: {e}");
                    }
                    crate::ui::favorites_sidebar::persist_favorites(all_fav_ids).await;
                }
            }
            {
                let guard = backend_handle.read().await;
                if let Ok(dms) = guard.get_dm_channels().await {
                    chat_data.write().dm_channels.extend(dms);
                }
                if let Ok(groups) = guard.get_groups().await {
                    chat_data.write().groups.extend(groups);
                }
                if let Ok(notifs) = guard.get_notifications().await {
                    chat_data.write().notifications.extend(notifs);
                }
                if let Ok(friends) = guard.get_friends().await {
                    for friend in friends {
                        let already = chat_data.read().friends.get(&account_id).map_or(false, |v| v.iter().any(|f| f.id == friend.id));
                        if !already {
                            chat_data.write().friends.entry(account_id.clone()).or_default().push(friend);
                        }
                    }
                }
            }
            navigator().push(Route::DmsHome {
                backend: backend_slug,
                instance_id,
                account_id,
            });
        });
    })
}

/// Ensure the Poly signup flow always has an identity key available.
async fn ensure_poly_signup_identity(client: &str) -> Option<Vec<u8>> {
    let storage = crate::STORAGE.get()?;
    let existing = storage.get_identity_key().await.ok().flatten();
    if existing.is_some() || client != "poly" {
        return existing.map(|key| key.to_vec());
    }

    let identity = crate::crypto::Identity::generate();
    let account_id = identity.public_identity().account_id;
    let key_bytes = identity.private_key_bytes();
    storage.set_identity_key(&key_bytes).await.ok()?;

    if let Ok(mut settings) = storage.get_app_settings().await {
        settings.account_id = account_id;
        let _ = storage.set_app_settings(&settings).await;
    }

    Some(key_bytes.to_vec())
}

// ── Navigation helper ────────────────────────────────────────────────────────

/// Navigate back to Settings.
///
/// Passed as [`SignupContext::navigate_back`] so plugins can offer a back-
/// button without depending on poly-core routes directly.
fn navigate_back_to_settings() {
    navigator().push(Route::SettingsRoute);
}

// ── Left sidebar ─────────────────────────────────────────────────────────────

/// Left sidebar for the Add Account page.
///
/// Lists all registered backend types (Poly Server, Matrix, …).
/// The `selected_slug` entry is highlighted like the active nav item in Settings.
#[rustfmt::skip]
#[component]
fn AddAccountNav(selected_slug: Option<String>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager = use_context::<Signal<ClientManager>>();
    let manager = client_manager.read();
    let disabled = manager.disabled_native_backends.clone();
    let entries: Vec<(String, String, String)> = manager
        .signup_entries
        .iter()
        .filter(|entry| !disabled.iter().any(|slug| slug == entry.slug))
        .map(|e| (e.slug.to_string(), t(e.name_key), e.icon.to_string()))
        .collect();

    rsx! {
        nav { class: "signup-nav",
            div { class: "signup-nav-header",
                Link {
                    to: Route::SettingsRoute,
                    class: "signup-nav-back",
                    "{t(\"signup-picker-back\")}"
                }
                h3 { class: "signup-nav-title", "{t(\"signup-picker-title\")}" }
                p { class: "signup-nav-subtitle", "{t(\"signup-picker-description\")}" }
            }
            for (slug, name, icon) in entries {
                {
                    let is_active = selected_slug.as_deref() == Some(slug.as_str());
                    let class = if is_active { "signup-nav-item active" } else { "signup-nav-item" };
                    rsx! {
                        div {
                            class,
                            onclick: move |_| {
                                navigator().push(Route::ClientSignup { client: slug.clone() });
                            },
                            span { class: "signup-nav-icon", "{icon}" }
                            span { "{name}" }
                        }
                    }
                }
            }
            // Test accounts quick-add link
            {
                let is_active = selected_slug.as_deref() == Some("test");
                let class = if is_active { "signup-nav-item active" } else { "signup-nav-item" };
                rsx! {
                    div { class: "signup-nav-separator" }
                    div {
                        class,
                        onclick: move |_| {
                            navigator().push(Route::ClientSignup { client: "test".to_string() });
                        },
                        span { class: "signup-nav-icon", "\u{1F9EA}" }
                        span { "Test Accounts" }
                    }
                }
            }
        }
    }
}

// ── Test account quick-add panel ─────────────────────────────────────────────

/// Test account definition for the quick-add panel.
struct TestAccount {
    icon: &'static str,
    /// Display name for the account (e.g. username).
    label: &'static str,
    /// Human-readable backend name shown as a badge (e.g. "Stoat", "Matrix").
    backend_label: &'static str,
    /// Backend slug used for routing (unused at runtime but documents the backend).
    backend: &'static str,
    base_url: &'static str,
    email: &'static str,
    password: &'static str,
    /// When true the card is shown but the button is disabled (backend not yet compiled in).
    disabled: bool,
}

const TEST_ACCOUNTS: &[TestAccount] = &[
    // ── Stoat (localhost:9101) ──────────────────────────────────────────
    TestAccount {
        icon: "\u{1F9A6}", label: "Stoat", backend_label: "Stoat",
        backend: "stoat", base_url: "http://localhost:9101",
        email: "stoat", password: "testpass123", disabled: false,
    },
    TestAccount {
        icon: "\u{1F99D}", label: "Raccoon", backend_label: "Stoat",
        backend: "stoat", base_url: "http://localhost:9101",
        email: "raccoon", password: "testpass123", disabled: false,
    },
    // ── Matrix (localhost:8448) ─────────────────────────────────────────
    TestAccount {
        icon: "\u{1F7E9}", label: "Alice", backend_label: "Matrix",
        backend: "matrix", base_url: "http://localhost:8448",
        email: "@alice:localhost", password: "testpass123", disabled: !cfg!(feature = "matrix"),
    },
    // ── Hacker News (public API, no auth) ──────────────────────────────
    TestAccount {
        icon: "\u{1F536}", label: "Guest", backend_label: "Hacker News",
        backend: "hackernews", base_url: "https://hacker-news.firebaseio.com",
        email: "", password: "", disabled: !cfg!(feature = "hackernews"),
    },
    // ── Lemmy (localhost:8536) ──────────────────────────────────────────
    TestAccount {
        icon: "\u{1F43E}", label: "Lemmy User", backend_label: "Lemmy",
        backend: "lemmy", base_url: "http://localhost:8536",
        email: "testuser", password: "testpass123", disabled: !cfg!(feature = "lemmy"),
    },
    // ── Discord stub (localhost:9102) ───────────────────────────────────
    TestAccount {
        icon: "\u{1F7E3}", label: "Discord User", backend_label: "Discord",
        backend: "discord", base_url: "http://localhost:9102",
        email: "discord_test", password: "testpass123", disabled: !cfg!(feature = "discord"),
    },
    // ── Teams stub (localhost:9103) ─────────────────────────────────────
    TestAccount {
        icon: "\u{1F7E6}", label: "Teams User", backend_label: "Microsoft Teams",
        backend: "teams", base_url: "http://localhost:9103",
        email: "teams_test", password: "testpass123", disabled: !cfg!(feature = "teams"),
    },
];

/// Authenticate a test account using the appropriate backend client.
///
/// Dispatches to the correct client crate based on `backend`.
/// Non-stoat backends are feature-gated; returns an error if not compiled in.
async fn test_account_authenticate(
    backend: &str,
    base_url: String,
    email: String,
    password: String,
) -> Result<poly_client::SignupCompleted, String> {
    match backend {
        "stoat" => poly_stoat::signup::authenticate(base_url, email, password).await,
        #[cfg(feature = "lemmy")]
        "lemmy" => poly_lemmy::signup::authenticate(base_url, email, password).await,
        #[cfg(feature = "hackernews")]
        "hackernews" => Ok(poly_hackernews::signup::complete_as_guest()),
        other => Err(format!("backend '{other}' not available in this build")),
    }
}

/// Panel shown when `?test=true` — quick-add buttons for test server accounts.
#[cfg(feature = "stoat")]
#[component]
fn TestAccountsPanel() -> Element {
    let client_manager = use_context::<Signal<ClientManager>>();
    let chat_data = use_context::<Signal<ChatData>>();
    let on_complete = build_on_complete(client_manager, chat_data);
    let mut statuses: Signal<Vec<String>> = use_signal(|| {
        TEST_ACCOUNTS.iter().map(|_| String::new()).collect()
    });

    rsx! {
        div { class: "signup-content",
            h2 { class: "signup-form-title", "Test Accounts" }
            p { class: "signup-form-desc",
                "Quick-add test server accounts for development. "
                "Requires test servers running on localhost."
            }
            div { class: "test-accounts-grid",
                for (idx, acct) in TEST_ACCOUNTS.iter().enumerate() {
                    {
                        let status = statuses.read().get(idx).cloned().unwrap_or_default();
                        let email = acct.email.to_string();
                        let base_url = acct.base_url.to_string();
                        let password = acct.password.to_string();
                        let backend = acct.backend.to_string();
                        let label = acct.label;
                        let icon = acct.icon;
                        let backend_label = acct.backend_label;
                        let account_disabled = acct.disabled;
                        let is_busy = status == "connecting...";
                        rsx! {
                            div {
                                class: if account_disabled { "test-account-card test-account-disabled" } else { "test-account-card" },
                                div { class: "test-account-header",
                                    span { class: "test-account-icon", "{icon}" }
                                    div { class: "test-account-info",
                                        span { class: "test-account-name", "{label}" }
                                        span { class: "test-account-backend", "{backend_label}" }
                                        span { class: "test-account-url", "{base_url}" }
                                    }
                                }
                                if account_disabled {
                                    p { class: "test-account-unavailable", "Backend not compiled in this build" }
                                } else {
                                    button {
                                        class: "btn btn-primary test-account-btn",
                                        disabled: is_busy,
                                        onclick: {
                                            let on_complete = on_complete.clone();
                                            move |_| {
                                                if let Some(slot) = statuses.write().get_mut(idx) {
                                                    *slot = "connecting...".to_string();
                                                }
                                                let email = email.clone();
                                                let base_url = base_url.clone();
                                                let password = password.clone();
                                                let backend = backend.clone();
                                                let on_complete = on_complete.clone();
                                                spawn(async move {
                                                    match test_account_authenticate(
                                                        &backend, base_url, email, password,
                                                    ).await {
                                                        Ok(completed) => {
                                                            if let Some(slot) = statuses.write().get_mut(idx) {
                                                                *slot = "connected!".to_string();
                                                            }
                                                            on_complete.call(completed);
                                                        }
                                                        Err(e) => {
                                                            if let Some(slot) = statuses.write().get_mut(idx) {
                                                                *slot = format!("error: {e}");
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        if is_busy { "Connecting..." } else { "Add Account" }
                                    }
                                }
                                if !status.is_empty() && status != "connecting..." {
                                    p {
                                        class: if status.starts_with("error") { "test-account-error" } else { "test-account-success" },
                                        "{status}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Picker page (/signup) ─────────────────────────────────────────────────────

/// Backend picker — `/signup` — three-panel layout, no backend selected yet.
///
/// Shows the favorites bar on the left, backend sidebar, and a placeholder
/// in the right panel prompting the user to pick an account type.
#[rustfmt::skip]
#[component]
pub(crate) fn SignupPickerPage() -> Element {
    rsx! {
        div { class: "add-account-page",
            FavoritesBar {}
            AddAccountNav { selected_slug: None }
            div { class: "signup-content",
                div { class: "signup-placeholder",
                    div { class: "signup-placeholder-icon", "🔷" }
                    p { class: "signup-placeholder-text", "{t(\"signup-picker-description\")}" }
                }
            }
        }
    }
}

#[cfg(feature = "stoat")]
fn test_panel_or_fallback() -> Element {
    rsx! { TestAccountsPanel {} }
}

#[cfg(not(feature = "stoat"))]
fn test_panel_or_fallback() -> Element {
    rsx! {
        div { class: "signup-content",
            p { "Test mode requires the stoat feature to be enabled." }
        }
    }
}

// ── Per-backend form (/signup/:client) ───────────────────────────────────────

/// Per-backend signup dispatch — `/signup/:client` — three-panel layout.
///
/// Shows the favorites bar on the left, the selected backend highlighted in the sidebar,
/// and its form in the right panel. Core handles all state commitment via the
/// `on_complete` callback.
#[rustfmt::skip]
#[component]
pub(crate) fn ClientSignupPage(client: String) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager = use_context::<Signal<ClientManager>>();
    let chat_data      = use_context::<Signal<ChatData>>();

    // Load private key async — hooks must run unconditionally before any returns.
    let key_resource = use_resource({
        let client_slug = client.clone();
        move || {
            let client_slug = client_slug.clone();
            async move { ensure_poly_signup_identity(&client_slug).await }
        }
    });

    // Find the render fn pointer — copy before releasing the borrow.
    let render_fn = {
        let manager = client_manager.read();
        if manager
            .disabled_native_backends
            .iter()
            .any(|slug| slug == &client)
        {
            None
        } else {
            manager
                .signup_entries
                .iter()
                .find(|e| e.slug == client.as_str())
                .map(|e| e.render)
        }
    };

    // `/signup/test` — quick-add test accounts panel.
    if client == "test" {
        return rsx! {
            div { class: "add-account-page",
                FavoritesBar {}
                AddAccountNav { selected_slug: Some("test".to_string()) }
                { test_panel_or_fallback() }
            }
        };
    }

    let right_content: Element = if let Some(render) = render_fn {
        // Blank content while the key resource is still loading (usually < 1 frame).
        match key_resource.read().clone() {
            None => rsx! { div { class: "signup-content" } },
            Some(opt_key) => {
                let ctx = SignupContext {
                    private_key: opt_key,
                    t: crate::i18n::t,
                    navigate_back: navigate_back_to_settings,
                };
                let on_complete = build_on_complete(client_manager, chat_data);
                rsx! {
                    div { class: "signup-content",
                        { render(on_complete, ctx) }
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "signup-content",
                div { class: "signup-placeholder",
                    p { class: "signup-stub-notice", "Unknown backend: {client}" }
                }
            }
        }
    };

    rsx! {
        div { class: "add-account-page",
            FavoritesBar {}
            AddAccountNav { selected_slug: Some(client.clone()) }
            { right_content }
        }
    }
}
