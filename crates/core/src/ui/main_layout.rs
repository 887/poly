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

use super::account::{AccountServerBar, ChannelContextMenu, ServerContextMenu, UserProfileModal};
use super::favorites_sidebar::FavoritesBar;
use super::routes::{Route, route_targets_unknown_account, sync_route_to_app_state};
use super::voice_banner::VoiceBanner;
use crate::client_manager::ClientManager;
use crate::state::{AppState, SettingsSection};
use dioxus::prelude::*;
use dioxus_router::use_route;
use poly_ui_macros::context_menu;

const MOBILE_DRAWER_RUNTIME_JS: Asset = asset!("assets/scripts/mobile_drawer_runtime.js", AssetOptions::js());
const MOBILE_DRAWER_CLOSE_JS: &str = "window.__polySetMobileDrawerOpen?.(false);";
const MOBILE_RIGHT_WING_CLOSE_JS: &str = "window.__polySetMobileRightWingOpen?.(false);";
const DRAG_BRIDGE_RUNTIME_JS: Asset = asset!("assets/scripts/drag_bridge_runtime.js", AssetOptions::js());
const SCROLL_RUNTIME_JS: Asset = asset!("assets/scripts/scroll_runtime.js", AssetOptions::js());

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserRuntime {
    WasmDom,
    #[cfg(not(target_arch = "wasm32"))]
    NativeStub,
}

const fn browser_runtime() -> BrowserRuntime {
    #[cfg(target_arch = "wasm32")]
    {
        BrowserRuntime::WasmDom
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        BrowserRuntime::NativeStub
    }
}

fn init_mobile_drawer_runtime() {
    match browser_runtime() {
        BrowserRuntime::WasmDom => {
            spawn(async move {
                let _ = crate::ui::load_js_asset(MOBILE_DRAWER_RUNTIME_JS).await;
            });
            spawn(async move {
                let _ = crate::ui::load_js_asset(SCROLL_RUNTIME_JS).await;
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        // DECISION(DX-MOBILE-1): poly-core cannot yet distinguish Wry from
        // Blitz at this layer, so native renderers get an explicit no-op stub
        // until a renderer-specific split-shell runtime exists.
        BrowserRuntime::NativeStub => {}
    }
}

pub(crate) fn close_mobile_drawer() {
    if browser_runtime() == BrowserRuntime::WasmDom {
        let _ = document::eval(MOBILE_DRAWER_CLOSE_JS);
    }
}

pub(crate) fn mobile_left_drawer_open() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        const MOBILE_LEFT_OPEN_CLASS: &str = "poly-mobile-left-wing-open";
        const MOBILE_LEFT_DRAGGING_CLASS: &str = "poly-mobile-left-wing-dragging";

        let Some(window) = web_sys::window() else {
            return false;
        };

        return window
            .document()
            .and_then(|document| document.query_selector(".poly-app").ok().flatten())
            .and_then(|root| root.get_attribute("class"))
            .is_some_and(|classes| {
                classes.split_whitespace().any(|class| {
                    class == MOBILE_LEFT_OPEN_CLASS || class == MOBILE_LEFT_DRAGGING_CLASS
                })
            });
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

fn close_mobile_right_wing() {
    if browser_runtime() == BrowserRuntime::WasmDom {
        let _ = document::eval(MOBILE_RIGHT_WING_CLOSE_JS);
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn runtime_mobile_ui_active() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };

    window
        .document()
        .and_then(|document| document.query_selector(".poly-app").ok().flatten())
        .and_then(|root| root.get_attribute("class"))
        .is_some_and(|classes| {
            classes
                .split_whitespace()
                .any(|class| class == "poly-layout-mode-force-mobile")
                || (classes
                    .split_whitespace()
                    .any(|class| class == "poly-layout-mode-auto-width")
                    && window
                        .inner_width()
                        .ok()
                        .and_then(|value| value.as_f64())
                        .is_some_and(|width| width <= 640.0))
                || (classes
                    .split_whitespace()
                    .any(|class| class == "poly-layout-mode-auto-portrait")
                    && {
                        let width = window
                            .inner_width()
                            .ok()
                            .and_then(|value| value.as_f64())
                            .unwrap_or_default();
                        let height = window
                            .inner_height()
                            .ok()
                            .and_then(|value| value.as_f64())
                            .unwrap_or_default();
                        height > width
                    })
        })
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) const fn runtime_mobile_ui_active() -> bool {
    false
}

/// Navigation bar component — only renders on native platforms (desktop/mobile).
/// On web, the browser's native back/forward buttons handle navigation.
// DECISION(DX-ROUTER-1): NavBar uses navigator().go_back()/go_forward()
// instead of custom AppState history stack.
#[context_menu(None)]
#[rustfmt::skip]
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
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn MainLayout() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

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

    use_effect(move || {
        init_mobile_drawer_runtime();
    });

    let route_key = format!("{route}");
    use_effect(move || {
        let _ = &route_key;
        crate::ui::preserve_layout_override_query_in_url();
        close_mobile_right_wing();
        if runtime_mobile_ui_active() {
            let mut state = app_state.write();
            state.nav.right_sidebar_visible = false;
            state.nav.dm_right_sidebar_visible = false;
        }
    });

    use_effect(move || {
        if route_targets_unknown_account(&route, &client_manager.read()) {
            app_state.write().settings_section = SettingsSection::Accounts;
            navigator().replace(Route::SettingsRoute);
        }
    });

    // Persist per-account last-visited routes to storage whenever they change.
    // This fires after every route navigation (because sync_route_to_app_state
    // updates account_last_routes inside AppState, which re-renders MainLayout).
    // The spawn ensures the async storage write doesn't block the render.
    use_effect(move || {
        let routes_snapshot = app_state.read().nav.account_last_routes.clone();
        if routes_snapshot.is_empty() {
            return;
        }
        let dm_routes_snapshot = app_state.read().nav.account_last_dm_routes.clone();
        spawn(async move {
            if let Some(storage) = crate::STORAGE.get()
                && let Err(e) = storage.set_account_last_routes(&routes_snapshot).await
            {
                tracing::warn!("Failed to persist account last routes: {e}");
            }
            if let Some(storage) = crate::STORAGE.get()
                && let Err(e) = storage.set_account_last_dm_routes(&dm_routes_snapshot).await
            {
                tracing::warn!("Failed to persist account last DM routes: {e}");
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
        if browser_runtime() == BrowserRuntime::WasmDom {
            spawn(async move {
                let _ = crate::ui::load_js_asset(DRAG_BRIDGE_RUNTIME_JS).await;
            });
        }
    });

    rsx! {
        div {
            class: "main-layout",
            // Root context-menu guard per plan-context-menu-quality-control.md §4.5.1.
            // Suppresses the native browser menu everywhere. Surfaces that want a
            // custom menu (or the native menu for images) must `stop_propagation()`
            // on oncontextmenu *before* this handler runs.
            oncontextmenu: move |evt| evt.prevent_default(),
            // Dismiss context menu when clicking outside of it
            onclick: move |_| {
                if app_state.read().context_menu.is_some() {
                    app_state.write().context_menu = None;
                }
                if app_state.read().channel_context_menu.is_some() {
                    app_state.write().channel_context_menu = None;
                }
            },
            // Floating server right-click context menu (position: fixed, above sidebars)
            ServerContextMenu {}
            // Floating channel right-click / long-press context menu
            ChannelContextMenu {}
            // Stacked context menus (plan §4.1.2) — Phase A host for new menus
            crate::ui::context_menu::host::ContextMenuStack {}
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
            // Global user profile modal — now rendered inside router context
            // so action buttons can navigate safely.
            UserProfileModal {}
        }
    }
}
