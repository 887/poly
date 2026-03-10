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
    let entries: Vec<(String, String, String)> = client_manager
        .read()
        .signup_entries
        .iter()
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
    let mut client_manager = use_context::<Signal<ClientManager>>();
    let mut chat_data      = use_context::<Signal<ChatData>>();

    // Load private key async — hooks must run unconditionally before any returns.
    let key_resource = use_resource(move || async move {
        let storage = crate::STORAGE.get()?;
        storage.get_identity_key().await.ok().flatten()
    });

    // Find the render fn pointer — copy before releasing the borrow.
    let render_fn = client_manager
        .read()
        .signup_entries
        .iter()
        .find(|e| e.slug == client.as_str())
        .map(|e| e.render);

    let right_content: Element = if let Some(render) = render_fn {
        // Blank content while the key resource is still loading (usually < 1 frame).
        match *key_resource.read() {
            None => rsx! { div { class: "signup-content" } },
            Some(opt_key) => {
                let ctx = SignupContext {
                    private_key: opt_key.map(|k| k.to_vec()),
                    t: crate::i18n::t,
                    navigate_back: navigate_back_to_settings,
                };
                // Build host-side on_complete callback.
                // Phase 1 (auth) is done by the plugin; Phases 2+3 are owned here.
                let on_complete = Callback::new(move |completed: SignupCompleted| {
                    let backend_handle: BackendHandle =
                        Arc::new(tokio::sync::RwLock::new(completed.backend));
                    let session = completed.session;
                    spawn(async move {
                        let account_id  = session.id.clone();
                        let instance_id = session.instance_id.clone();

                        // Persist the account token so it survives app restarts.
                        if let Some(storage) = crate::STORAGE.get() {
                            let at = crate::storage::AccountToken {
                                backend: "poly".to_string(),
                                account_id: account_id.clone(),
                                token: session.token.clone(),
                                display_name: session.user.display_name.clone(),
                                instance_id: session.backend_url.clone(),
                            };
                            if let Err(e) = storage.upsert_account_token(&at).await {
                                tracing::warn!("Failed to persist poly account token: {e}");
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
                        client_manager.write().commit_poly_server(
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
                                let mut cd = chat_data.write();
                                for srv in &servers {
                                    if !cd.favorited_server_ids.contains(&srv.id) {
                                        cd.favorited_server_ids.push(srv.id.clone());
                                    }
                                }
                                cd.servers.extend(servers);
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
                                    if !chat_data.read().friends.iter().any(|f| f.id == friend.id) {
                                        chat_data.write().friends.push(friend);
                                    }
                                }
                            }
                        }
                        navigator().push(Route::DmsHome {
                            backend: "poly".to_string(),
                            instance_id,
                            account_id,
                        });
                    });
                });
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
