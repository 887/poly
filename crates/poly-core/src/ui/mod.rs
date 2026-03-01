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

/// Root application component.
///
/// On first render, initialises the storage backend and reads persisted
/// settings to decide whether to show the setup wizard or the main layout.
/// Until storage is ready, a blank loading screen is shown (typically <50 ms).
#[component]
pub fn App() -> Element {
    // Global app state
    let mut app_state = use_signal(AppState::default);
    // True once storage has been initialised and settings loaded.
    let mut storage_ready = use_signal(|| false);

    // Initialise storage exactly once. Stores the handle in the global
    // `STORAGE` OnceLock so that event handlers and coroutines can reach it
    // without prop-drilling.
    // DECISION(DX-STORAGE-4): storage init in use_future ensures it runs after
    // the component mounts but before the first meaningful render completes.
    use_future(move || async move {
        match crate::storage::Storage::init().await {
            Ok(storage) => {
                // Persist the handle globally.
                let _ = crate::STORAGE.set(storage.clone());

                // Read persisted settings to decide initial view.
                match storage.get_app_settings().await {
                    Ok(settings) if settings.setup_complete => {
                        tracing::info!("Storage: setup already complete, going to main layout");
                        app_state.write().is_setup_complete = true;
                        app_state.write().nav.view = View::DmsFriends;
                    }
                    Ok(_) => {
                        tracing::info!("Storage: no previous setup found, showing wizard");
                    }
                    Err(e) => {
                        tracing::warn!("Storage: failed to read app_settings: {e}");
                    }
                }
                storage_ready.set(true);
            }
            Err(e) => {
                // Storage failure is non-fatal — fall back to in-memory only.
                tracing::error!("Storage init failed: {e}. Running without persistence.");
                storage_ready.set(true);
            }
        }
    });

    rsx! {
        document::Link { rel: "stylesheet", href: CSS }
        div { class: "poly-app",
            if !*storage_ready.read() {
                // Brief loading state while storage opens (<50 ms typically).
                div { class: "storage-loading" }
            } else if !app_state.read().is_setup_complete {
                SetupWizard {
                    on_complete: move |account_id: String| {
                        app_state.write().is_setup_complete = true;
                        app_state.write().nav.view = View::DmsFriends;

                        // Persist setup completion to storage (fire-and-forget).
                        spawn(async move {
                            if let Some(s) = crate::STORAGE.get() {
                                let settings = crate::storage::AppSettings {
                                    setup_complete: true,
                                    account_id,
                                    locale: "en".to_string(),
                                    theme: "neutral-dark".to_string(),
                                };
                                if let Err(e) = s.set_app_settings(&settings).await {
                                    tracing::error!("Failed to persist app settings: {e}");
                                } else {
                                    tracing::info!("App settings persisted to storage ✓");
                                }
                            }
                        });
                    },
                }
            } else {
                MainLayout { app_state }
            }
        }
    }
}

