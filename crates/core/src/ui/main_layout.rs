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
use super::routes::{Route, route_targets_unknown_account, sync_route_to_app_state};
use super::voice_banner::VoiceBanner;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use dioxus::prelude::*;
use dioxus_router::use_route;

fn init_mobile_drawer_runtime() {
    let _ = document::eval(
        r#"if (!window.__polyMobileDrawerInit) {
            window.__polyMobileDrawerInit = true;
            window.__polySetMobileDrawerOpen = function(open) {
                const root = document.querySelector('.poly-app');
                if (!root) return;
                root.classList.toggle('poly-mobile-drawer-open', Boolean(open));

                const server = document.querySelector('.server-sidebar');
                const account = document.querySelector('.account-server-bar');
                const channel = document.querySelector('.channel-list-wrapper');
                const backdrop = document.querySelector('.mobile-drawer-backdrop');
                const openBtn = document.querySelector('.mobile-drawer-open-btn');
                const closeBtn = document.querySelector('.mobile-drawer-close-btn');

                if (server) {
                    server.style.setProperty('left', '-72px', 'important');
                    server.style.setProperty('inset-inline-start', '-72px', 'important');
                    server.style.setProperty('margin-left', open ? '72px' : '0px', 'important');
                    server.style.removeProperty('transform');
                }

                if (account) {
                    account.style.setProperty('left', '-72px', 'important');
                    account.style.setProperty('inset-inline-start', '-72px', 'important');
                    account.style.setProperty('margin-left', open ? '144px' : '0px', 'important');
                    account.style.removeProperty('transform');
                }

                if (channel) {
                    channel.style.setProperty('left', '-100vw', 'important');
                    channel.style.setProperty('inset-inline-start', '-100vw', 'important');
                    channel.style.setProperty('margin-left', open ? 'calc(100vw + 144px)' : '0px', 'important');
                    channel.style.removeProperty('transform');
                }

                if (backdrop) {
                    backdrop.style.setProperty('display', open ? 'block' : 'none', 'important');
                    backdrop.style.setProperty('visibility', open ? 'visible' : 'hidden', 'important');
                    backdrop.style.setProperty('opacity', open ? '1' : '0', 'important');
                    backdrop.style.setProperty('pointer-events', open ? 'auto' : 'none', 'important');
                    backdrop.style.setProperty('background', open ? 'rgba(0, 0, 0, 0.34)' : 'transparent', 'important');
                }

                if (openBtn) {
                    openBtn.style.setProperty('opacity', open ? '0' : '0.94', 'important');
                    openBtn.style.setProperty('pointer-events', open ? 'none' : 'auto', 'important');
                }

                if (closeBtn) {
                    closeBtn.style.setProperty('opacity', open ? '1' : '0', 'important');
                    closeBtn.style.setProperty('pointer-events', open ? 'auto' : 'none', 'important');
                }
            };

            let tracking = null;

            document.addEventListener('touchstart', function(e) {
                const root = document.querySelector('.poly-app');
                if (!root) return;
                const isMobileUi = root.classList.contains('poly-force-mobile') || window.innerWidth <= 640;
                if (!isMobileUi || !e.touches || e.touches.length !== 1) {
                    tracking = null;
                    return;
                }

                const touch = e.touches[0];
                const drawerOpen = root.classList.contains('poly-mobile-drawer-open');
                const x = touch.clientX;
                const y = touch.clientY;

                if (!drawerOpen && x <= 24) {
                    tracking = { mode: 'open', startX: x, startY: y };
                    return;
                }

                if (drawerOpen && x <= Math.min(window.innerWidth, 360)) {
                    tracking = { mode: 'close', startX: x, startY: y };
                    return;
                }

                tracking = null;
            }, { passive: true });

            document.addEventListener('touchend', function(e) {
                if (!tracking || !e.changedTouches || e.changedTouches.length !== 1) {
                    tracking = null;
                    return;
                }

                const root = document.querySelector('.poly-app');
                if (!root) {
                    tracking = null;
                    return;
                }

                const touch = e.changedTouches[0];
                const dx = touch.clientX - tracking.startX;
                const dy = Math.abs(touch.clientY - tracking.startY);

                if (dy <= 80) {
                    if (tracking.mode === 'open' && dx >= 60) {
                        window.__polySetMobileDrawerOpen(true);
                    } else if (tracking.mode === 'close' && dx <= -60) {
                        window.__polySetMobileDrawerOpen(false);
                    }
                }

                tracking = null;
            }, { passive: true });
        }"#,
    );
}

fn open_mobile_drawer() {
    let _ = document::eval("window.__polySetMobileDrawerOpen?.(true);");
}

fn close_mobile_drawer() {
    let _ = document::eval("window.__polySetMobileDrawerOpen?.(false);");
}

/// Navigation bar component — only renders on native platforms (desktop/mobile).
/// On web, the browser's native back/forward buttons handle navigation.
// DECISION(DX-ROUTER-1): NavBar uses navigator().go_back()/go_forward()
// instead of custom AppState history stack.
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
        close_mobile_drawer();
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
            button {
                class: "mobile-drawer-open-btn",
                title: t("mobile-nav-open"),
                onclick: move |_| open_mobile_drawer(),
                "☰"
            }
            button {
                class: "mobile-drawer-close-btn",
                title: t("mobile-nav-close"),
                onclick: move |_| close_mobile_drawer(),
                "✕"
            }
            div {
                class: "mobile-drawer-backdrop",
                onclick: move |_| close_mobile_drawer(),
            }
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
