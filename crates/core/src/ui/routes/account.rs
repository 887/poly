//! Account / auth route adapter components.
//!
//! Covers the root redirect, signup flow, reauth, 404 catch-all, and the
//! debug-assertions route-coverage counter.

use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

use super::Route;

/// Root redirect — desktop memory history starts at "/".
///
/// Uses `use_effect` to navigate away on mount since the `on_update`
/// callback may not process its redirect return value on the very first
/// render in Dioxus memory-history mode.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn Root() -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    // CRITICAL: use_hook (not use_effect) so this fires EXACTLY ONCE on mount.
    // The previous use_effect captured client_manager.read() which subscribed
    // the effect to ClientManager. Boot registers ~38 signup/plugin-settings/
    // test-account entries — each one re-fires this effect, which re-calls
    // navigator().replace() mid-cascade. The repeated route-replace inside an
    // active render scrambles Dioxus's node-id table and surfaces as the
    // "Cannot set properties of undefined (setting 'textContent')" crash on
    // the next text-edit opcode.
    use_hook(|| {
        let demo_active = client_manager.peek().demo_active;
        if demo_active {
            navigator().replace(Route::DmsHome {
                backend: "demo".to_string(),
                instance_id: "demo".to_string(),
                account_id: "demo-cat".to_string(),
            });
        } else {
            navigator().replace(Route::SettingsRoute);
        }
    });
    rsx! {}
}

/// Backend picker — `/signup` — full-page, outside MainLayout.
///
/// Renders [`crate::ui::signup::SignupPickerPage`] which lists available backends
/// and navigates to `/signup/:client` on selection.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn SignupPicker() -> Element {
    rsx! {
        crate::ui::signup::SignupPickerPage {}
    }
}

/// Per-backend signup page — `/signup/:client` — full-page, outside MainLayout.
///
/// The `client` slug selects which backend signup page to render:
/// - `"poly"` → full Poly server signup/sign-in form
/// - all others → "coming soon" stub
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ClientSignup(client: String) -> Element {
    rsx! {
        crate::ui::signup::ClientSignupPage { client }
    }
}

/// Per-account reauth page — `/:backend/:instance_id/:account_id/reauth`.
///
/// Full-page form (outside MainLayout) that lets the user update the existing
/// account's credentials in place, or remove the account entirely. Used when
/// a stored token has been rejected (401) and the app has marked the
/// connection status as [`ConnectionStatus::Unauthenticated`].
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ReauthAccount(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        crate::ui::signup::ReauthAccountPage { backend, instance_id, account_id }
    }
}

/// Catch-all 404 — on_update redirects before render, but as a belt-and-suspenders
/// fallback this component also redirects to Root on mount.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn PageNotFound(segments: Vec<String>) -> Element {
    // `segments` is provided by the router for the unmatched path but we only
    // need it for the route match; discard it so the unused-variable lint stays clean.
    drop(segments);
    let nav = navigator();
    use_effect(move || { // poly-lint: allow stale-effect-capture — mount-only belt-and-suspenders redirect; nav is a hook return value stable per component instance
        // Belt-and-suspenders: redirect in case on_update hasn't fired yet
        // (e.g. stale browser history URLs from old route formats).
        nav.replace(Route::Root);
    });
    // Brief loading while redirect fires — never visible in practice.
    rsx! { div { class: "storage-loading" } }
}

/// Runtime route-coverage counter (plan-connected-routes §7.4).
///
/// Debug-only. On each call, records the visited `Route` variant in a
/// process-wide set and logs the first observation at `debug` level via
/// `tracing`. Lets a dev session's visited set be diffed against the full
/// `Route` enum to find routes that were declared but never exercised.
#[cfg(debug_assertions)]
pub(super) fn record_route_visit(route: &Route) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static VISITED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let set = VISITED.get_or_init(|| Mutex::new(HashSet::new()));
    let name = Route::route_variant_name(route);
    if let Ok(mut guard) = set.lock() && guard.insert(name) {
        tracing::debug!(target: "poly::route_coverage", "visited route variant: {name}");
    }
}
