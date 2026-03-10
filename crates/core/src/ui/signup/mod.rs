//! Account signup flow — full-page, outside MainLayout.
//!
//! Rendered at `/signup` (backend picker) and `/signup/:client` (per-backend page).
//!
//! ## Architecture
//!
//! The signup flow is **plugin-driven**: core has zero compile-time knowledge of
//! which backends exist. Each compiled-in or WASM plugin registers a
//! [`SignupEntry`](crate::client_manager::SignupEntry) at startup via
//! [`ClientManager::register_signup_entry`](crate::client_manager::ClientManager::register_signup_entry).
//!
//! The picker page reads `client_manager.signup_entries` at render time.  
//! The dispatch page calls `entry.render(on_complete, ctx)` for the matched slug.
//!
//! ## Currently registered (at startup in `poly_core::init`)
//! - `poly` — full Poly Server signup/sign-in (two-phase async auth)
//!
//! ## Adding a new backend
//! 1. Define a render fn `fn my_render(Callback<SignupCompleted>, SignupContext) -> Element`
//!    in the client crate (e.g. `poly-my-client/src/signup.rs`).
//! 2. Call `client_manager.register_signup_entry(SignupEntry { slug: "my", ...,
//!    render: my_render })`.
//! 3. Add FTL keys in the plugin-owned `locales/` directory, NOT in core locale files.
//!
//! ## 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines.

use crate::client_manager::{BackendHandle, ClientManager};
use crate::i18n::t;
use crate::state::ChatData;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{SignupCompleted, SignupContext};
use std::collections::HashMap;
use std::sync::Arc;

/// Navigate back to the signup backend picker.
///
/// Used as [`SignupContext::navigate_back`] so plugins can offer a consistent
/// "← back" experience without depending on poly-core routes directly.
fn navigate_back_to_picker() {
    navigator().push(Route::SignupPicker);
}

/// Backend picker — `/signup` full-page component.
///
/// Lists all backends registered at startup via `ClientManager.signup_entries`.
/// Clicking a backend card navigates to `/signup/:client`.
#[rustfmt::skip]
#[component]
pub(crate) fn SignupPickerPage() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let client_manager = use_context::<Signal<ClientManager>>();
    // Snapshot the entries (all &'static str, cheap to clone into owned Strings)
    let entries: Vec<(String, String, String, String)> = client_manager
        .read()
        .signup_entries
        .iter()
        .map(|e| (
            e.slug.to_string(),
            t(e.name_key).to_string(),
            t(e.desc_key).to_string(),
            e.icon.to_string(),
        ))
        .collect();

    rsx! {
        div { class: "signup-page-root",
            div { class: "signup-card",
                Link {
                    to: Route::SettingsRoute,
                    class: "signup-back-link",
                    "{t(\"signup-picker-back\")}"
                }
                h2 { class: "signup-card-title", "{t(\"signup-picker-title\")}" }
                p { class: "signup-card-desc", "{t(\"signup-picker-description\")}" }
                div { class: "signup-backend-list",
                    for (slug, name, desc, icon) in entries {
                        BackendCard {
                            key: "{slug}",
                            slug,
                            name,
                            desc,
                            icon,
                        }
                    }
                }
            }
        }
    }
}

/// A single backend card button in the picker.
#[rustfmt::skip]
#[component]
fn BackendCard(slug: String, name: String, desc: String, icon: String) -> Element {
    rsx! {
        button {
            class: "signup-backend-btn",
            onclick: move |_| {
                navigator().push(Route::ClientSignup { client: slug.clone() });
            },
            span { class: "signup-backend-icon", "{icon}" }
            div { class: "signup-backend-info",
                div { class: "signup-backend-name", "{name}" }
                div { class: "signup-backend-desc", "{desc}" }
            }
            span { class: "signup-backend-arrow", "›" }
        }
    }
}

/// Per-backend signup dispatch — `/signup/:client` full-page component.
///
/// Looks up the `client` slug in `ClientManager.signup_entries`, loads the
/// local Ed25519 private key from storage, then calls the registered `render`
/// function with an `on_complete` callback and a [`SignupContext`].
///
/// Core has no compile-time knowledge of which backends are available; the
/// host only supplies the callback that commits state after successful auth.
#[rustfmt::skip]
#[component]
pub(crate) fn ClientSignupPage(client: String) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let mut client_manager = use_context::<Signal<ClientManager>>();
    let mut chat_data   = use_context::<Signal<ChatData>>();

    // Load private key async — hooks must be called unconditionally before early returns.
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

    let Some(render) = render_fn else {
        return rsx! {
            div { class: "signup-page-root",
                div { class: "signup-card",
                    Link {
                        to: Route::SignupPicker,
                        class: "signup-back-link",
                        "{t(\"signup-stub-back\")}"
                    }
                    p { class: "signup-stub-notice", "Unknown backend: {client}" }
                }
            }
        };
    };

    // Show a blank placeholder while the key loads (usually < 1 frame).
    let private_key: Option<Vec<u8>> = match *key_resource.read() {
        None => return rsx! { div { class: "signup-page-root", div { class: "signup-card" } } },
        Some(opt_key) => opt_key.map(|k| k.to_vec()),
    };

    let ctx = SignupContext {
        private_key,
        t: crate::i18n::t,
        navigate_back: navigate_back_to_picker,
    };

    // Build the host-side on_complete callback.
    // Phase 1 (auth) is done by the plugin; here we handle Phase 2 (sync commit)
    // and Phase 3 (async data load), then navigate to the new account's home.
    let on_complete = Callback::new(move |completed: SignupCompleted| {
        let backend_handle: BackendHandle = Arc::new(tokio::sync::RwLock::new(completed.backend));
        let session = completed.session;
        spawn(async move {
            let account_id  = session.id.clone();
            let instance_id = session.instance_id.clone();
            // Build server→account map before committing (async, no Signal lock).
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
            // Phase 3: async data loading — no Signal lock held during awaits.
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

    render(on_complete, ctx)
}
