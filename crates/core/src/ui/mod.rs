//! UI components for Poly.
//!
//! All Dioxus UI components live here. The main entry point is [`App`],
//! which renders the setup wizard or the main app layout.
//!
//! ## Multi-Backend UI Architecture
//!
//! Account-scoped components live in `account/` with a **per-backend**
//! override structure:
//! - `account/common/` — Shared components used by ALL backends
//! - `account/demo/` — Demo backend UI overrides
//! - `account/stoat/` — Stoat backend UI overrides
//! - `account/discord/` — Discord backend UI overrides
//! - `account/matrix/` — Matrix backend UI overrides
//! - `account/teams/` — Teams backend UI overrides
//! - `account/poly_native/` — Poly native server UI overrides
//!
//! Dispatch is by `BackendType` match at render time. See
//! `docs/multi-client-architecture.md` for the full architecture guide.
//!
//! ## Component Hierarchy
//! - [`App`] — Root component (setup wizard or main layout)
//!   - [`SetupWizard`] — First-launch key generation
//!   - [`MainLayout`] — 4-column desktop layout
//!     - [`FavoritesBar`] — Left server icon list
//!     - [`account::ChannelList`] — Channel list for selected server
//!       - [`account::VoiceBar`] — Voice connection status bar
//!       - [`account::AccountBar`] — User info + quick controls
//!     - [`account::ChatView`] — Messages and input (text channels)
//!     - [`account::VoiceChannelView`] — Voice/video call view (voice channels)
//!     - [`account::EmojiPicker`] — Emoji grid for reactions and input
//!     - [`account::UserSidebar`] — Right user list
//!
//! ## Module layout
//! | Module | Contents |
//! |---|---|
//! | `account` | Multi-backend account-scoped UI (common + per-backend) |
//! | `account::common` | Shared components across all backends |
//! | `account::demo` | Demo backend overrides |
//! | `account::stoat` | Stoat backend overrides |
//! | `account::discord` | Discord backend overrides |
//! | `account::matrix` | Matrix backend overrides |
//! | `account::teams` | Teams backend overrides |
//! | `account::poly_native` | Poly native server overrides |
//! | `account::server` | Server-scoped UI (context menu, settings) |
//! | `account::settings` | Account-scoped settings (notifications) |
//! | `settings` | App-level settings page |
//! | `favorites_sidebar` | Left-most server icon list |
//! | `main_layout` | 4-column desktop shell |
//! | `voice_banner` | Top-spanning voice connection banner |
//! | `setup_wizard` | First-launch key generation wizard |
//!
//! ## 150-line component rule
//! Every `#[component]` fn body in any file under `src/ui/` MUST stay under
//! **150 lines** of RSX + logic. Extract sub-components rather than growing.
//! **NEVER hardcode demo/test data in UI components** — all data must flow
//! through the `ClientBackend` trait via `ClientManager`.

pub mod account;
mod favorites_sidebar;
mod main_layout;
pub mod routes;
mod settings;
mod setup_wizard;
mod voice_banner;

pub use account::{AccountSwitcher, FriendsPanel};
pub use main_layout::MainLayout;
pub use routes::Route;
pub use setup_wizard::SetupWizard;

use crate::client_manager::ClientManager;
use crate::state::{AppState, ChatData, SettingsSection, View};
use dioxus::prelude::*;
use routes::sync_route_to_app_state;

/// Compiled stylesheet asset — watched by Dioxus hot-reload.
const CSS: Asset = asset!("assets/tailwind.css");

// ── App — async helpers ──────────────────────────────────────────────────────

/// Initialise storage, apply persisted theme + locale, and decide the initial view.
///
/// Called once via `use_future` on App mount. Always sets `storage_ready` to
/// `true` when done — failures fall back to in-memory-only mode.
// DECISION(DX-STORAGE-4): storage init in use_future ensures it runs after
// the component mounts but before the first meaningful render completes.
async fn init_storage(
    mut theme_config: Signal<crate::theme::ThemeConfig>,
    mut storage_ready: Signal<bool>,
    mut app_state: Signal<AppState>,
    mut locale_sig: Signal<String>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
) {
    match crate::storage::Storage::init().await {
        Ok(storage) => {
            let _ = crate::STORAGE.set(storage.clone());
            if let Err(e) = storage.run_migrations().await {
                tracing::warn!("Storage migration error: {e}");
            }
            match storage.get_theme_config().await {
                Ok(config) => theme_config.set(config),
                Err(e) => tracing::warn!("Failed to load saved theme config: {e}"),
            }
            match storage.get_app_settings().await {
                Ok(settings) if settings.setup_complete => {
                    tracing::info!("Storage: setup complete, going to main layout");
                    crate::i18n::set_locale(&settings.locale);
                    *locale_sig.write() = settings.locale.clone();
                    app_state.write().is_setup_complete = true;
                    app_state.write().nav.view = View::DmsFriends;
                    // Restore favorited servers so Bar 1 repopulates immediately
                    // on launch — before the server list is fetched from the network.
                    if !settings.favorited_server_ids.is_empty() {
                        chat_data.write().favorited_server_ids =
                            settings.favorited_server_ids.clone();
                        tracing::info!(
                            "Restored {} favorited server(s) from storage",
                            settings.favorited_server_ids.len()
                        );
                    }
                    // Restore the demo client if it was active when the app last closed.
                    // toggle_demo activates all demo data; the Router's Root component
                    // then redirects to /demo/demo/dms once it mounts.
                    if settings.demo_active {
                        favorites_sidebar::toggle_demo(client_manager, chat_data).await;
                    }
                }
                Ok(_) => tracing::info!("Storage: no setup found, showing wizard"),
                Err(e) => tracing::warn!("Storage: failed to read app_settings: {e}"),
            }
            storage_ready.set(true);
        }
        Err(e) => {
            tracing::error!("Storage init failed: {e}. Running without persistence.");
            storage_ready.set(true);
        }
    }
}

/// Persist a completed setup to storage: account ID, locale, and default theme.
async fn persist_setup_completion(account_id: String) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    let locale = crate::i18n::current_locale();
    let settings = crate::storage::AppSettings {
        setup_complete: true,
        account_id,
        locale,
        theme: "neutral-dark".to_string(),
        // demo_active remains false — setup wizard completion means a real account;
        // demo is managed separately by the 🧪 toggle.
        demo_active: false,
        // New install has no favorites yet.
        favorited_server_ids: Vec::new(),
    };
    if let Err(e) = s.set_app_settings(&settings).await {
        tracing::error!("Failed to persist app settings: {e}");
    } else {
        tracing::info!("App settings persisted ✓");
    }
    if let Err(e) = s
        .set_theme_config(&crate::theme::ThemeConfig::default())
        .await
    {
        tracing::error!("Failed to persist default theme config: {e}");
    }
}

// ── App component ─────────────────────────────────────────────────────────────

/// Root application component.
///
/// Shows a blank loading screen while storage initialises (<50 ms), then
/// routes to the setup wizard or the main layout based on saved settings.
///
/// ## Context provided to children
/// - `Signal<String>` — current locale (from [`crate::i18n::provide_locale_context`])
/// - `Signal<crate::theme::ThemeConfig>` — active theme (from [`provide_context`])
/// - `Signal<ClientManager>` — client manager for active backends
/// - `Signal<ChatData>` — reactive chat data store
#[component]
pub fn App() -> Element {
    let mut app_state = use_signal(AppState::default);
    let storage_ready = use_signal(|| false);
    // DECISION(DX-I18N-1): Signal<String> context; use_locale() in children subscribes.
    crate::i18n::provide_locale_context();
    let locale_sig = crate::i18n::use_locale();
    // DECISION(DX-THEME-1): Signal<ThemeConfig> context + <style> injection.
    let theme_config: Signal<crate::theme::ThemeConfig> =
        use_signal(crate::theme::ThemeConfig::default);
    provide_context(theme_config);

    // DECISION(DX-2.5.1): ClientManager + ChatData as context Signals.
    let client_manager: Signal<ClientManager> = use_signal(ClientManager::new);
    provide_context(client_manager);
    let chat_data: Signal<ChatData> = use_signal(ChatData::default);
    provide_context(chat_data);

    // Provide app_state as context so child components subscribe independently
    // via use_context() instead of receiving it as a prop (which enables Dioxus
    // prop-comparison skip optimization that can suppress signal-triggered re-renders).
    provide_context(app_state);

    use_future(move || async move {
        init_storage(
            theme_config,
            storage_ready,
            app_state,
            locale_sig,
            client_manager,
            chat_data,
        )
        .await;
    });
    let theme_css = crate::theme::generate_css(&theme_config.read());
    rsx! {
        document::Link { rel: "stylesheet", href: CSS }
        style { id: "poly-theme", "{theme_css}" }
        div { class: "poly-app",
            if !*storage_ready.read() {
                div { class: "storage-loading" }
            } else if !app_state.read().is_setup_complete {
                SetupWizard {
                    on_complete: move |account_id: String| {
                        app_state.write().is_setup_complete = true;
                        spawn(async move {
                            // Router handles initial route via on_update redirect.
                            // TODO(phase-2.7): If no accounts exist, prompt user to
                            // add one via a modal or navigate to settings after mount.
                            persist_setup_completion(account_id).await;
                        });
                    },
                }
            } else {
                Router::<Route> {
                    config: move || {
                        dioxus_router::RouterConfig::default()
                            .on_update(move |state: dioxus_router::GenericRouterContext<Route>| {
                                let route = state.current();
                                sync_route_to_app_state(&route, app_state);

                                // If the route is account-scoped and that account no
                                // longer exists (demo toggled off, account removed, …)
                                // redirect to Settings › Accounts rather than rendering
                                // a broken empty shell.
                                let route_account_id: Option<&str> = match &route {
                                    Route::DmsHome { account_id, .. }
                                    | Route::DmChat { account_id, .. }
                                    | Route::ServerHome { account_id, .. }
                                    | Route::ServerChat { account_id, .. }
                                    | Route::ServerSettingsRoute { account_id, .. }
                                    | Route::FriendsRoute { account_id, .. }
                                    | Route::AccountSettingsRoute { account_id, .. } => {
                                        Some(account_id.as_str())
                                    }
                                    _ => None,
                                };
                                if let Some(aid) = route_account_id {
                                    // Only redirect to Settings if accounts are actually loaded
                                    // but the requested account is unknown.
                                    //
                                    // Guarding on `!ids.is_empty()` handles two race conditions
                                    // on hard refresh (F5):
                                    //   1. Router fires on_update before init_storage runs
                                    //      → client_manager is empty → skip redirect.
                                    //   2. init_storage sets is_setup_complete=true (Router
                                    //      mounts early) and then yields into async toggle_demo
                                    //      → client_manager still empty → skip redirect.
                                    //
                                    // Once toggle_demo completes and accounts populate,
                                    // the signal change re-triggers on_update. At that point
                                    // ids is non-empty and the "demo" account IS known →
                                    // no redirect, AccountServerBar (Bar 2) stays visible.
                                    let ids = client_manager.read().active_account_ids();
                                    if !ids.is_empty() {
                                        let is_known = ids.iter().any(|id| id == aid);
                                        if !is_known {
                                            let mut as_ = app_state;
                                            as_.write().settings_section =
                                                SettingsSection::Accounts;
                                            return Some(
                                                NavigationTarget::Internal(Route::SettingsRoute),
                                            );
                                        }

                                        // Redirect root path and catch-all 404 to the best
                                        // active account's DMs route.
                                        //
                                        // Priority:
                                        //   1. Demo account (if active)     → /demo/demo/dms
                                        //   2. First real account (future)  → /:backend/:id/dms
                                        //   3. No accounts at all           → /settings (Accounts tab)
                                        //
                                        // TODO(phase-2.7): Read last-active account from AppSettings
                                        // and prefer real accounts over demo when multiple exist.
                                    }
                                }
                                if matches!(route, Route::PageNotFound { .. } | Route::Root) {
                                    let cm = client_manager.read();
                                    if cm.demo_active {
                                        return Some( // No active accounts — land on Settings › Accounts so
                                            NavigationTarget::Internal(Route::DmsHome { // Signal<T>: Copy — shadow into a local mut binding
                                                backend: "demo".to_string(),
                                                instance_id: "demo".to_string(),
                                                account_id: "demo-cat".to_string(),
                                            }),
                                        );
                                    }
                                    drop(cm);
                                    let mut as_ = app_state;
                                    as_.write().settings_section = SettingsSection::Accounts;
                                    return Some(NavigationTarget::Internal(Route::SettingsRoute));
                                }
                                None
                            })
                    },
                }
            }
        }
    }
}
