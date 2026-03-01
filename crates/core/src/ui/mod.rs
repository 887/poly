//! UI components for Poly.
//!
//! All Dioxus UI components live here. The main entry point is [`App`],
//! which renders the setup wizard or the main app layout.
//!
//! ## Component Hierarchy
//! - [`App`] — Root component (setup wizard or main layout)
//!   - [`SetupWizard`] — First-launch key generation
//!   - [`MainLayout`] — 4-column desktop layout
//!     - [`ServerSidebar`] — Left server icon list
//!     - [`ChannelList`] — Channel list for selected server
//!     - [`ChatView`] — Messages and input
//!     - [`UserSidebar`] — Right user list

mod channel_list;
mod chat_view;
mod main_layout;
mod notifications;
mod server_sidebar;
mod settings;
mod setup_wizard;
mod user_sidebar;

pub use main_layout::MainLayout;
pub use setup_wizard::SetupWizard;

use crate::state::{AppState, View};
use dioxus::prelude::*;

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
    use_future(move || async move {
        init_storage(theme_config, storage_ready, app_state, locale_sig).await;
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
                        app_state.write().nav.view = View::DmsFriends;
                        spawn(async move {
                            persist_setup_completion(account_id).await;
                        });
                    },
                }
            } else {
                MainLayout { app_state }
            }
        }
    }
}
