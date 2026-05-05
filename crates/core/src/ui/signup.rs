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

mod register_link;
use register_link::RegisterLink;

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandle, ClientManager};
use crate::i18n::t;
use crate::state::{AccountSessions, ChatLists};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::favorites_sidebar::FavoritesBar;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{IsBackend, SignupCompleted, SignupContext};
use std::collections::HashMap;
use std::sync::Arc;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the signup picker page (`/signup`).
#[derive(Debug, Clone)]
pub(crate) enum SignupPickerPageAction {
    /// User selected a backend to sign up with.
    SelectBackend(String),
}

impl UiAction for SignupPickerPageAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: SignupPickerPageAction requires Navigator");
    }
}

/// Actions for the per-backend signup page (`/signup/:client`).
#[derive(Debug, Clone)]
pub(crate) enum ClientSignupPageAction {
    /// Signup flow completed for a backend.
    Complete,
    /// User navigated back.
    Back,
}

impl UiAction for ClientSignupPageAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: ClientSignupPageAction requires Navigator + backend handles");
    }
}

/// Actions for the reauth page.
#[derive(Debug, Clone)]
pub(crate) enum ReauthAccountPageAction {
    /// Reauth completed — refresh credentials for the account.
    Complete,
    /// User confirmed removal of the account.
    RemoveAccount,
    /// User cancelled removal.
    CancelRemove,
}

impl UiAction for ReauthAccountPageAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: ReauthAccountPageAction requires Signal + async handles");
    }
}

// ── Shared signup-commit callback builder ───────────────────────────────────

/// Build the `on_complete` callback that commits a newly authenticated backend
/// account into the app state (Signal writes, server cache, navigation).
///
/// Used by both the normal per-backend signup flow and the quick test-account panel.
pub(crate) fn build_on_complete(
    client_manager: BatchedSignal<ClientManager>,
) -> Callback<SignupCompleted> {
    build_on_complete_inner(client_manager, true)
}

/// Variant that skips the terminal `navigator().push(landing)`. Needed by
/// the debug-mode auto-signin path in `crate::ui::mod`, which runs from a
/// `use_effect` above the Router — no navigator in scope → panic → WASM
/// `unreachable`. Auto-signin doesn't need a route change anyway; the user
/// is already on whatever route they restored to at startup.
pub(crate) fn build_on_complete_no_nav(
    client_manager: BatchedSignal<ClientManager>,
) -> Callback<SignupCompleted> {
    build_on_complete_inner(client_manager, false)
}

fn build_on_complete_inner(
    client_manager: BatchedSignal<ClientManager>,
    nav_on_complete: bool,
) -> Callback<SignupCompleted> {
    // Capture the sub-signals from context so writes route to the correct stores.
    // These are always provided at the App level alongside chat_data.
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    Callback::new(move |completed: SignupCompleted| {
        let backend_handle: BackendHandle =
            Arc::new(tokio::sync::RwLock::new(completed.backend));
        let refresh_token = completed.refresh_token.clone();
        let token_expires_at = completed.token_expires_at.clone();
        let scope = completed.scope.clone();
        let mut session = completed.session;
        // Quick-add test accounts sign in against local mock servers that
        // don't serve real avatar PNGs. Overlay a bundled animal portrait
        // (Owl, Axolotl, Stoat, …) so the sidebar and Settings → Accounts
        // list show the cute icon instead of a first-letter bubble.
        #[cfg(feature = "demo")]
        if (session.user.avatar_url.is_none()
            || session
                .user
                .avatar_url
                .as_deref()
                .is_some_and(|u| !u.starts_with("http")))
            && let Some(url) = poly_demo::data::test_animal_avatar(&session.user.display_name) {
                session.user.avatar_url = Some(url);
            }
        spawn(async move {
            // Guard: reject duplicate session IDs (e.g. two anonymous HN accounts).
            // EXCEPTION: a Disconnected/Unauthenticated session is an offline
            // placeholder created by `account_restore::restore_native_accounts`
            // when its stored token failed to authenticate. Allow signup to
            // replace those — otherwise re-signing-in (which gives us a fresh
            // valid token) silently no-ops and the account stays offline.
            {
                use poly_client::ConnectionStatus;
                let cm = client_manager.read();
                if cm.sessions.contains_key(&session.id) {
                    let connected = matches!(
                        cm.connection_statuses.get(&session.id),
                        Some(ConnectionStatus::Connected) | Some(ConnectionStatus::Connecting),
                    );
                    if connected {
                        tracing::warn!("signup: session '{}' already connected — ignoring duplicate", session.id);
                        return;
                    }
                    tracing::info!("signup: replacing offline placeholder for session '{}'", session.id);
                    drop(cm);
                    let aid = session.id.clone();
                    client_manager.batch(move |cm| { cm.take_account(&aid); });
                }
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
                    refresh_token: refresh_token.clone(),
                    token_expires_at: token_expires_at.clone(),
                    scope: scope.clone(),
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
            {
                let aid = account_id.clone();
                let sess = session.clone();
                let bh = backend_handle.clone();
                client_manager.batch(move |cm| cm.commit_backend_account(aid, sess, bh, server_map));
            }
            {
                let aid = account_id.clone();
                account_sessions.batch(move |as_| { as_.account_sessions.insert(aid, session); });
            }
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
                    chat_lists.batch(move |cl| {
                        for srv in servers {
                            if !cl.servers.iter().any(|s| s.id == srv.id) {
                                cl.push_server(srv);
                            }
                        }
                    });

                    if let Some(storage) = crate::STORAGE.get()
                        && let Err(e) =
                            storage.upsert_offline_server_cache(&cache_records).await
                    {
                        tracing::warn!("Failed to cache server metadata: {e}");
                    }
                }
            }
            {
                let guard = backend_handle.read().await;
                let dms = guard.get_dm_channels().await.ok();
                let groups = guard.get_groups().await.ok();
                let notifs = guard.get_notifications().await.ok();
                let friends = guard.get_friends().await.ok();
                // Use capability accessor (H.1 — ContentPolicyBackend).
                // Returns None for all current backends; preserved for future opt-ins.
                let (blocked, policy) = if let Some(cp) = guard.as_content_policy() {
                    (
                        cp.get_blocked_users().await.ok(),
                        cp.get_content_policy().await.ok(),
                    )
                } else {
                    (None, None)
                };
                let aid = account_id.clone();
                chat_lists.batch(move |cl| {
                    if let Some(dms) = dms {
                        cl.dm_channels.extend(dms);
                    }
                    if let Some(groups) = groups {
                        cl.groups.extend(groups);
                    }
                    if let Some(notifs) = notifs {
                        cl.notifications.extend(notifs.into_iter().filter(|n| !n.read));
                    }
                    if let Some(friends) = friends {
                        for friend in friends {
                            let already = cl.friends.get(&aid).is_some_and(|v| v.iter().any(|f| f.id == friend.id));
                            if !already {
                                cl.friends.entry(aid.clone()).or_default().push(friend);
                            }
                        }
                    }
                });
                if let Some(blocked) = blocked {
                    let aid = account_id.clone();
                    account_sessions.batch(move |as_| {
                        as_.blocked_users.insert(aid, blocked);
                    });
                }
                if let Some(policy) = policy {
                    account_sessions.batch(move |as_| {
                        as_.content_policy = policy;
                    });
                }
            }
            let caps = client_manager.peek().capabilities_for_slug(&backend_slug);
            let landing = match caps.landing {
                poly_client::LandingPage::Overview => Route::ServerOverviewRoute {
                    backend: backend_slug,
                    instance_id,
                    account_id,
                },
                poly_client::LandingPage::FirstServer => {
                    let first_server = chat_lists.peek().servers.iter()
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
            if nav_on_complete {
                navigator().push(landing);
            } else {
                let _ = landing; // keep computed but don't nav
            }
        });
    })
}

// ── Reauth-mode commit callback ──────────────────────────────────────────────

/// Build an `on_complete` for the per-account reauth flow.
///
/// Differs from [`build_on_complete`] in that it:
/// 1. Forces the incoming `session.id` to the caller-supplied `target_account_id`
///    so the existing row (token, session, backend handle, server map) is updated
///    in place — the stable account identity survives the reauth.
/// 2. Bypasses the duplicate-session check (overwrite is the point).
/// 3. Clears `ConnectionStatus::Unauthenticated` by replacing it with
///    `Connected` on commit.
/// 4. Navigates back to the account's natural landing page.
fn build_on_complete_reauth(
    target_account_id: String,
    client_manager: BatchedSignal<ClientManager>,
) -> Callback<SignupCompleted> {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    Callback::new(move |completed: SignupCompleted| {
        let backend_handle: BackendHandle =
            Arc::new(tokio::sync::RwLock::new(completed.backend));
        let refresh_token = completed.refresh_token.clone();
        let token_expires_at = completed.token_expires_at.clone();
        let scope = completed.scope.clone();
        let mut session = completed.session;
        // Pin the session to the original account id so existing rows overwrite.
        session.id = target_account_id.clone();
        spawn(async move {
            let backend_slug = session.backend.slug().to_string();
            let account_id  = session.id.clone();
            let instance_id = session.instance_id.clone();

            // Persist the fresh token over the stale one.
            if let Some(storage) = crate::STORAGE.get() {
                let at = crate::storage::AccountToken {
                    backend: backend_slug.clone(),
                    account_id: account_id.clone(),
                    token: session.token.clone(),
                    display_name: session.user.display_name.clone(),
                    instance_id: session.backend_url.clone(),
                    refresh_token: refresh_token.clone(),
                    token_expires_at: token_expires_at.clone(),
                    scope: scope.clone(),
                };
                if let Err(e) = storage.upsert_account_token(&at).await {
                    tracing::warn!("Failed to persist reauthenticated token: {e}");
                }
            }

            // Rebuild server→account map (may have changed if server list did).
            let mut server_map = HashMap::new();
            {
                let guard = backend_handle.read().await;
                if let Ok(servers) = guard.get_servers().await {
                    for srv in &servers {
                        server_map.insert(srv.id.clone(), account_id.clone());
                    }
                }
            }
            {
                let aid = account_id.clone();
                let sess = session.clone();
                let bh = backend_handle.clone();
                client_manager.batch(move |cm| cm.commit_backend_account(aid, sess, bh, server_map));
            }
            {
                let aid = account_id.clone();
                account_sessions.batch(move |as_| { as_.account_sessions.insert(aid, session); });
            }

            let caps = client_manager.peek().capabilities_for_slug(&backend_slug);
            let landing = match caps.landing {
                poly_client::LandingPage::Overview => Route::ServerOverviewRoute {
                    backend: backend_slug,
                    instance_id,
                    account_id,
                },
                poly_client::LandingPage::FirstServer => {
                    let first_server = chat_lists.peek().servers.iter()
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

/// Delete a single account fully — runtime state, storage row, and cached chat data.
///
/// Used by the "Remove this account" button on the reauth page.
async fn remove_backend_account_now(
    account_id: String,
    backend_slug: String,
    client_manager: BatchedSignal<ClientManager>,
) {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let aid = account_id.clone();
    let handle = client_manager.batch(move |cm| cm.take_account(&aid));
    if let Some(h) = handle {
        let mut g = h.write().await;
        drop(g.logout().await);
    }
    {
        let aid = account_id.clone();
        chat_lists.batch(move |cl| {
            cl.set_servers(cl.servers.iter().filter(|s| s.account_id != aid).cloned().collect());
            cl.dm_channels.retain(|d| d.account_id != aid);
            cl.groups.retain(|g| g.account_id != aid);
            cl.notifications.retain(|n| n.account_id != aid);
            cl.friends.remove(&aid);
        });
    }
    {
        let aid = account_id.clone();
        let live_server_ids: Vec<String> = chat_lists.peek().servers.iter().map(|s| s.id.clone()).collect();
        account_sessions.batch(move |as_| {
            as_.account_sessions.remove(&aid);
            as_.favorited_server_ids.retain(|id| live_server_ids.contains(id));
        });
    }
    if let Some(storage) = crate::STORAGE.get() {
        drop(storage.remove_account_token(&backend_slug, &account_id).await);
    }
    crate::nav!(Route::SettingsRoute);
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
        drop(storage.set_app_settings(&settings).await);
    }

    Some(key_bytes.to_vec())
}

// ── Navigation helper ────────────────────────────────────────────────────────

/// Navigate back to Settings.
///
/// Passed as [`SignupContext::navigate_back`] so plugins can offer a back-
/// button without depending on poly-core routes directly.
fn navigate_back_to_settings() {
    crate::nav!(Route::SettingsRoute);
}

// ── Left sidebar ─────────────────────────────────────────────────────────────

/// Left sidebar for the Add Account page.
///
/// Lists all registered backend types (Poly Server, Matrix, …).
/// The `selected_slug` entry is highlighted like the active nav item in Settings.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AddAccountNav(selected_slug: Option<String>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager = use_context::<BatchedSignal<ClientManager>>();
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
                                crate::nav!(Route::ClientSignup { client: slug.clone() });
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
                            crate::nav!(Route::ClientSignup { client: "test".to_string() });
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

#[context_menu(inherit)]
/// Panel shown at `/signup/test` — quick-add buttons for all registered test accounts.
#[ui_action(inherit)]
#[component]
fn TestAccountsPanel() -> Element {
    let client_manager = use_context::<BatchedSignal<ClientManager>>();
    let on_complete = build_on_complete(client_manager);
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
                        let on_complete2 = on_complete;
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
                                        let oc = on_complete2;
                                        let mut statuses2 = statuses;
                                        if let Some(s) = statuses2.write().get_mut(idx) {
                                            *s = "loading".to_string();
                                        }
                                        spawn(async move {
                                            match (auth_fn)(bu, un, pw).await {
                                                Ok(completed) => {
                                                    if let Some(s) = statuses2.write().get_mut(idx) {
                                                        *s = "ok".to_string();
                                                    }
                                                    oc.call(completed);
                                                }
                                                Err(e) => {
                                                    if let Some(s) = statuses2.write().get_mut(idx) {
                                                        *s = format!("error: {e}");
                                                    }
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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(SignupPickerPageAction)]
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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(ClientSignupPageAction)]
#[component]
pub(crate) fn ClientSignupPage(client: String) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager = use_context::<BatchedSignal<ClientManager>>();

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
                let on_complete = build_on_complete(client_manager);
                let slug_for_register = client.clone();
                rsx! {
                    div { class: "signup-content",
                        { render(on_complete, ctx) }
                        // Register affordance lives in the right pane (next to
                        // the form) so the picker entries on the left stay
                        // compact. Only renders when the backend declares
                        // External / InApp signup_method; NotSupported is hidden.
                        div { class: "signup-content-register",
                            RegisterLink {
                                backend_slug: slug_for_register,
                                server_url: None,
                            }
                        }
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

// ── Reauth sidebar ───────────────────────────────────────────────────────────

/// Narrow left-nav for the reauth page.
///
/// Unlike the full [`AddAccountNav`] which lists every backend for account
/// creation, this sidebar is scoped to the single account being reauthed —
/// it shows just that backend's entry plus a back link and the account's
/// display name as subtitle. No other backends are offered because the user
/// is not adding a new account, they are renewing credentials for a specific
/// existing one.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ReauthNav(backend_slug: String, display_name: String) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager = use_context::<BatchedSignal<ClientManager>>();
    let entry = client_manager
        .read()
        .signup_entries
        .iter()
        .find(|e| e.slug == backend_slug.as_str())
        .map(|e| (t(e.name_key), e.icon.to_string(), t(e.desc_key)));

    rsx! {
        nav { class: "signup-nav",
            div { class: "signup-nav-header",
                h3 { class: "signup-nav-title", "{t(\"notifications-reconnect\")}" }
                p { class: "signup-nav-subtitle", "{display_name}" }
            }
            if let Some((name, icon, desc)) = entry {
                div { class: "signup-nav-item active",
                    span { class: "signup-nav-icon", "{icon}" }
                    div { class: "signup-nav-item-text",
                        span { class: "signup-nav-item-name", "{name}" }
                        span { class: "signup-nav-item-desc", "{desc}" }
                    }
                }
            }
        }
    }
}

// ── Per-account reauth form (/:backend/:instance_id/:account_id/reauth) ─────

/// Per-account reauth page — `/:backend/:instance_id/:account_id/reauth`.
///
/// Shown when a stored token has been rejected (401). Renders the same
/// per-backend form that the signup flow uses but commits the result over
/// the existing account row instead of creating a new one. Also offers a
/// "Remove this account" button.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(ReauthAccountPageAction)]
#[component]
pub(crate) fn ReauthAccountPage(
    backend: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let _ = instance_id; // carried in the URL for /:backend/:instance_id/:account_id symmetry
    let client_manager = use_context::<BatchedSignal<ClientManager>>();
    let account_sessions = use_context::<BatchedSignal<AccountSessions>>();

    let display_name = account_sessions
        .read()
        .account_sessions
        .get(&account_id).map_or_else(|| account_id.clone(), |s| s.user.display_name.clone());

    let (render_fn, backend_name) = {
        let manager = client_manager.read();
        let entry = manager
            .signup_entries
            .iter()
            .find(|e| e.slug == backend.as_str());
        (entry.map(|e| e.render), entry.map(|e| t(e.name_key)).unwrap_or_default())
    };

    let right_content: Element = if let Some(render) = render_fn {
        let ctx = SignupContext {
            private_key: None,
            t: crate::i18n::t,
            navigate_back: navigate_back_to_settings,
        };
        let on_complete = build_on_complete_reauth(account_id.clone(), client_manager);
        let aid_for_remove = account_id.clone();
        let slug_for_remove = backend.clone();
        let mut confirm_remove = use_signal(|| false);
        rsx! {
            div { class: "signup-content reauth-content",
                h2 { class: "signup-form-title reauth-page-title",
                    "{t(\"notifications-reconnect\")}"
                }
                h3 { class: "reauth-backend-title", "{backend_name}" }
                { render(on_complete, ctx) }
                div { class: "reauth-remove-section",
                    if confirm_remove() {
                        p { class: "reauth-remove-confirm-text",
                            "Remove this account? Local credentials will be deleted."
                        }
                        div { class: "reauth-remove-confirm-row",
                            button {
                                class: "btn btn-danger",
                                onclick: move |_| {
                                    let aid = aid_for_remove.clone();
                                    let slug = slug_for_remove.clone();
                                    spawn(async move {
                                        remove_backend_account_now(aid, slug, client_manager).await;
                                    });
                                },
                                "Yes, remove"
                            }
                            button {
                                class: "btn btn-secondary",
                                onclick: move |_| confirm_remove.set(false),
                                "Cancel"
                            }
                        }
                    } else {
                        button {
                            class: "btn btn-danger reauth-remove-btn",
                            onclick: move |_| confirm_remove.set(true),
                            "{t(\"settings-remove-account\")}"
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "signup-content",
                div { class: "signup-placeholder",
                    p { class: "signup-stub-notice", "Unknown backend: {backend}" }
                }
            }
        }
    };

    rsx! {
        div { class: "add-account-page",
            FavoritesBar {}
            ReauthNav { backend_slug: backend.clone(), display_name: display_name.clone() }
            { right_content }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn signup_page_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<SignupPickerPageAction>();
        let _ = SignupPickerPageAction::SelectBackend("demo".into());
    }

    #[test]
    fn client_signup_page_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ClientSignupPageAction>();
        let _ = ClientSignupPageAction::Complete;
        let _ = ClientSignupPageAction::Back;
    }

    #[test]
    fn reauth_account_page_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ReauthAccountPageAction>();
        let _ = ReauthAccountPageAction::Complete;
        let _ = ReauthAccountPageAction::RemoveAccount;
        let _ = ReauthAccountPageAction::CancelRemove;
    }
}
