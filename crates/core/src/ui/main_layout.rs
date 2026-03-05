//! Main application layout — router layout wrapping all views.
//!
//! Provides the fixed chrome shared across all views:
//! - Voice connection banner (full width)
//! - Back/Forward navigation bar (native platforms only)
//! - Server sidebar (always visible)
//! - Outlet for route-specific content
//!
//! Route-specific content (channel lists, chat views, settings) is rendered
//! by nested layout components (DmsLayout, ServerLayout) or directly by
//! route page components via the Dioxus Router's Outlet.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::account::{AccountServerBar, ServerContextMenu};
use super::favorites_sidebar::FavoritesBar;
use super::routes::{Route, sync_route_to_app_state};
use super::voice_banner::VoiceBanner;
use crate::state::AppState;
use dioxus::prelude::*;
use dioxus_router::use_route;

/// Navigation bar component — only renders on native platforms (desktop/mobile).
/// On web, the browser's native back/forward buttons handle navigation.
// DECISION(DX-ROUTER-1): NavBar uses navigator().go_back()/go_forward()
// instead of custom AppState history stack.
#[component]
fn NavBar() -> Element {
    #[cfg(feature = "native-nav")]
    {
        use crate::i18n::t;
        return rsx! {
            div { class: "nav-bar-top",
                button {
                    class: "nav-btn",
                    onclick: move |_| {
                        navigator().go_back();
                    },
                    title: "{t(\"nav-back\")}",
                    "◀"
                }
                button {
                    class: "nav-btn",
                    onclick: move |_| {
                        navigator().go_forward();
                    },
                    title: "{t(\"nav-forward\")}",
                    "▶"
                }
            }
        };
    }

    #[cfg(not(feature = "native-nav"))]
    {
        return rsx! {
            Fragment {}
        };
    }
}

/// Main application layout — router layout component.
///
/// Renders the fixed chrome (voice banner, nav bar, server sidebar)
/// and delegates route-specific content to the [`Outlet`].
///
/// Desktop: voice banner + (nav bar | server sidebar | outlet)
/// Mobile: TBD
#[component]
pub fn MainLayout() -> Element {
    let mut app_state: Signal<AppState> = use_context();

    // DECISION(DX-ROUTER-3): Dioxus 0.7 web router does not fire on_update for the
    // initial browser URL when the Router component first mounts — only for subsequent
    // navigation events. Without this call, active_account_id stays None after F5,
    // causing AccountServerBar (Bar 2) to permanently vanish.
    //
    // Calling sync_route_to_app_state here (synchronously during render) via use_route()
    // ensures AppState.nav is always in sync with the URL on every render, including the
    // very first one. Children (AccountServerBar, ChannelList, etc.) see the correct
    // nav state immediately without a flash.
    //
    // use_route() creates a reactive subscription: whenever the route changes, MainLayout
    // re-renders, sync_route_to_app_state runs again, and all children update.
    // NOTE: writing to a Signal during render is safe here because MainLayout does not
    // read app_state via .read() in its own render body — only in event handlers.
    let route = use_route::<Route>();
    sync_route_to_app_state(&route, app_state);

    // Persist per-account last-visited routes to storage whenever they change.
    // This fires after every route navigation (because sync_route_to_app_state
    // updates account_last_routes inside AppState, which re-renders MainLayout).
    // The spawn ensures the async storage write doesn't block the render.
    use_effect(move || {
        let routes_snapshot = app_state.read().nav.account_last_routes.clone();
        if routes_snapshot.is_empty() {
            return;
        }
        spawn(async move {
            if let Some(storage) = crate::STORAGE.get()
                && let Err(e) = storage.set_account_last_routes(&routes_snapshot).await
            {
                tracing::warn!("Failed to persist account last routes: {e}");
            }
        });
    });

    // WebKit2GTK (used by Wry/desktop) requires:
    //   1. `dataTransfer.setData()` called **synchronously** in `dragstart` — or the entire
    //      drag operation is silently cancelled before it begins.
    //   2. `dragover.preventDefault()` called **synchronously** — Dioxus handlers fire via
    //      IPC and always arrive too late for the browser to accept them.
    //
    // Inject capture-phase JS listeners at the document level once, before any drag starts.
    // These run synchronously in the browser JS engine (before Dioxus IPC round-trips),
    // satisfying the WebKit requirements. Dioxus's own ondragstart / ondragover / ondrop
    // handlers still fire afterwards and update ChatData state correctly.
    use_effect(move || {
        let _ = document::eval(
            "if (!window.__polyDragInit) {\
                window.__polyDragInit = true;\
                document.addEventListener('dragstart', function(e) {\
                    if (e.dataTransfer) {\
                        try { e.dataTransfer.setData('text/plain', 'poly-drag'); } catch(_) {}\
                    }\
                }, true);\
                document.addEventListener('dragover', function(e) {\
                    e.preventDefault();\
                }, true);\
                document.addEventListener('drop', function(e) {\
                    e.preventDefault();\
                }, true);\
            }",
        );
    });

    rsx! {
        div {
            class: "main-layout",
            // Dismiss context menu when clicking outside of it
            onclick: move |_| {
                if app_state.read().context_menu.is_some() {
                    app_state.write().context_menu = None;
                }
            },
            // Floating server right-click context menu (position: fixed, above sidebars)
            ServerContextMenu {}
            // Voice connection banner — spans full width when connected
            VoiceBanner {}
            // Main body: nav + sidebar + route content
            div { class: "main-layout-body",
                // Back/Forward navigation — only on native platforms (not web)
                NavBar {}
                // Left: Favorites Bar (Bar 1 — always visible)
                FavoritesBar {}
                // Left: Account Server Bar (Bar 2 — when an account is active)
                AccountServerBar {}
                // Route content: DmsLayout, ServerLayout, or standalone views
                Outlet::<Route> {}
            } // end main-layout-body
        }
    }
}
