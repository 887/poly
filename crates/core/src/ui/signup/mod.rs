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
        let mut session = completed.session;
        // Quick-add test accounts sign in against local mock servers that
        // don't serve real avatar PNGs. Overlay a bundled animal portrait
        // (Owl, Axolotl, Stoat, …) so the sidebar and Settings → Accounts
        // list show the cute icon instead of a first-letter bubble.
        #[cfg(feature = "demo")]
        if session.user.avatar_url.is_none()
            || session
                .user
                .avatar_url
                .as_deref()
                .is_some_and(|u| !u.starts_with("http"))
        {
            if let Some(url) = poly_demo::data::test_animal_avatar(&session.user.display_name) {
                session.user.avatar_url = Some(url);
            }
        }
        spawn(async move {
            // Guard: reject duplicate session IDs (e.g. two anonymous HN accounts).
            if client_manager.read().sessions.contains_key(&session.id) {
                tracing::warn!("signup: session '{}' already exists — ignoring duplicate", session.id);
                return;
            }
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
                    chat_data.write().notifications.extend(notifs.into_iter().filter(|n| !n.read));
                }
                if let Ok(friends) = guard.get_friends().await {
                    for friend in friends {
                        let already = chat_data.read().friends.get(&account_id).map_or(false, |v| v.iter().any(|f| f.id == friend.id));
                        if !already {
                            chat_data.write().friends.entry(account_id.clone()).or_default().push(friend);
                        }
                    }
                }
                if let Ok(blocked) = guard.get_blocked_users().await {
                    chat_data.write().blocked_users.insert(account_id.clone(), blocked);
                }
                if let Ok(policy) = guard.get_content_policy().await {
                    chat_data.write().content_policy = policy;
                }
            }
            let caps = poly_client::capabilities_for_slug(&backend_slug);
            let landing = match caps.landing {
                poly_client::LandingPage::ServerOverview => Route::ServerOverviewRoute {
                    backend: backend_slug,
                    instance_id,
                    account_id,
                },
                poly_client::LandingPage::FirstServer => {
                    let first_server = chat_data.read().servers.iter()
                        .find(|s| s.account_id == account_id)
                        .map(|s| s.id.clone());
                    if let Some(server_id) = first_server {
                        Route::ServerHome {
                            backend: backend_slug,
                            instance_id,
                            account_id,
                            server_id,
                        }
                    } else {
                        Route::DmsHome {
                            backend: backend_slug,
                            instance_id,
                            account_id,
                        }
                    }
                }
                poly_client::LandingPage::DirectMessages => Route::DmsHome {
                    backend: backend_slug,
                    instance_id,
                    account_id,
                },
            };
            navigator().push(landing);
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
    let entries: Vec<(String, String, String, String)> = manager
        .signup_entries
        .iter()
        .filter(|entry| !disabled.iter().any(|slug| slug == entry.slug))
        .map(|e| (e.slug.to_string(), t(e.name_key), e.icon.to_string(), t(e.desc_key)))
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
            for (slug, name, icon, desc) in entries {
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
                            div { class: "signup-nav-item-text",
                                span { class: "signup-nav-item-name", "{name}" }
                                span { class: "signup-nav-item-desc", "{desc}" }
                            }
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

/// Panel shown at `/signup/test` — quick-add buttons for all registered test accounts.
#[component]
fn TestAccountsPanel() -> Element {
    let client_manager = use_context::<Signal<ClientManager>>();
    let chat_data = use_context::<Signal<ChatData>>();
    let on_complete = build_on_complete(client_manager, chat_data);
    let entries: Vec<poly_client::TestAccountEntry> = client_manager
        .read()
        .test_account_entries
        .to_vec();
    let statuses: Signal<Vec<String>> = use_signal(|| {
        entries.iter().map(|_| String::new()).collect()
    });

    rsx! {
        div { class: "signup-content",
            h2 { class: "signup-form-title", "Test Accounts" }
            p { class: "signup-form-desc",
                "Quick-add test server accounts for development. "
                "Requires test servers running on localhost."
            }
            div { class: "test-accounts-grid",
                for (idx, acct) in entries.iter().enumerate() {
                    {
                        let icon = acct.icon;
                        let label = acct.label.to_string();
                        let server_label = acct.server_label.to_string();
                        let base_url = acct.base_url.to_string();
                        let username = acct.username.to_string();
                        let password = acct.password.to_string();
                        let auth_fn = acct.authenticate;
                        let status = statuses.read().get(idx).cloned().unwrap_or_default();
                        let on_complete2 = on_complete.clone();
                        rsx! {
                            div { class: "test-account-card",
                                span { class: "test-account-icon", "{icon}" }
                                div { class: "test-account-info",
                                    span { class: "test-account-name", "{label}" }
                                    span { class: "test-account-url", "{server_label}" }
                                }
                                button {
                                    class: "btn btn-primary test-account-btn",
                                    disabled: status == "loading",
                                    onclick: move |_| {
                                        let bu = base_url.clone();
                                        let un = username.clone();
                                        let pw = password.clone();
                                        let oc = on_complete2.clone();
                                        let mut statuses2 = statuses;
                                        statuses2.write()[idx] = "loading".to_string();
                                        spawn(async move {
                                            match (auth_fn)(bu, un, pw).await {
                                                Ok(completed) => {
                                                    statuses2.write()[idx] = "ok".to_string();
                                                    oc.call(completed);
                                                }
                                                Err(e) => {
                                                    statuses2.write()[idx] = format!("error: {e}");
                                                }
                                            }
                                        });
                                    },
                                    if status == "loading" { "Connecting…" } else { "Add Account" }
                                }
                                if !status.is_empty() && status != "loading" {
                                    p {
                                        class: if status.starts_with("error") { "test-account-error" } else { "test-account-success" },
                                        "{status}"
                                    }
                                }
                            }
                        }
                    }
                }
                if entries.is_empty() {
                    p { class: "signup-form-desc", "No test accounts registered. Enable test plugins to see test accounts here." }
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
                TestAccountsPanel {}
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
