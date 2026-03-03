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

use super::account::AccountServerBar;
use super::favorites_sidebar::FavoritesBar;
use super::routes::Route;
use super::voice_banner::VoiceBanner;
use dioxus::prelude::*;

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
    rsx! {
        div { class: "main-layout",
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
