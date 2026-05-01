//! `RegisterLink` — "Don't have an account?" affordance for signup screens.
//!
//! Renders per-backend depending on the [`SignupMethod`] the backend declares:
//!
//! - `External(url)` — an `<a>` that opens the external signup URL in a new tab.
//!   The `onclick` handler also fires `window.open` so the link works in Wry
//!   and Electron webviews where `target="_blank"` may be sandboxed.
//! - `InApp(route)` — a Dioxus [`Link`] to the backend's own in-app signup route.
//!   Hidden when the active route is already that route.
//! - `NotSupported` — renders nothing.
//!
//! ## Placement
//!
//! Mounted by [`AddAccountNav`](super) (picker sidebar) and, after Phase D.2,
//! by each backend's per-backend signup form.
//!
//! ## Data-testid
//!
//! Each rendered link carries `data-testid="register-link-{backend_slug}"` so
//! Phase E Playwright specs can locate links without relying on translated text.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use dioxus_router::use_route;
use poly_client::SignupMethod;
use poly_ui_macros::{context_menu, ui_action};

/// "Register" affordance shown on login / signup screens.
///
/// # Props
/// - `backend_slug` — the URL slug that identifies the backend (e.g. `"stoat"`,
///   `"matrix"`). Used to look up the signup method and to build the testid.
/// - `server_url` — optional custom server URL typed by the user (passed to
///   `signup_method_fn` so instance-parameterised backends can return the right
///   URL). `None` on the picker page where no URL has been entered yet.
#[context_menu(inherit)]
#[ui_action(inherit)]
#[component]
pub(crate) fn RegisterLink(backend_slug: String, server_url: Option<String>) -> Element {
    // Read signup_method_fn from the registry. Use .peek() — this is a one-shot
    // snapshot for a hook key, not a reactive subscription. See CLAUDE.md hang #7.
    let signup_method_fn: Option<fn(Option<&str>) -> SignupMethod> = {
        let manager = use_context::<BatchedSignal<ClientManager>>();
        let guard = manager.peek();
        guard
            .signup_entries
            .iter()
            .find(|e| e.slug == backend_slug.as_str())
            .map(|e| e.signup_method)
    };

    let method = signup_method_fn
        .map_or(SignupMethod::NotSupported, |f| f(server_url.as_deref()));

    // use_route must be called unconditionally (hook rules).
    let current_route = use_route::<Route>();

    match method {
        SignupMethod::External(url) => {
            let testid = format!("register-link-{backend_slug}");
            let host = extract_host(&url);
            let label = t("signup-register-link-action")
                .replace("{$service}", &host);
            let js_url = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".into());
            rsx! {
                a {
                    class: "register-link register-link--external",
                    "data-testid": "{testid}",
                    href: "{url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    onclick: move |evt| {
                        evt.prevent_default();
                        // window.open works in all shells:
                        // - Web: opens new browser tab.
                        // - Electron: intercepted by setWindowOpenHandler → shell.openExternal.
                        // - Wry: falls back to system browser via window.open dispatch.
                        let js = format!(
                            "window.open({url}, '_blank', 'noopener,noreferrer');",
                            url = js_url,
                        );
                        let _eval = document::eval(&js);
                    },
                    "{t(\"signup-register-link-prefix\")} "
                    span { class: "register-link__action", "{label}" }
                    span { class: "register-link__arrow", " →" }
                }
            }
        }
        SignupMethod::InApp(route) => {
            let target_route = Route::ClientSignup { client: route.clone() };
            // Hide the link when already on this signup page.
            if current_route == target_route {
                return rsx! {};
            }
            let testid = format!("register-link-{backend_slug}");
            // Wrap in a span so we can attach the data-testid; Link doesn't
            // accept arbitrary HTML attributes.
            rsx! {
                span {
                    "data-testid": "{testid}",
                    Link {
                        class: "register-link register-link--in-app",
                        to: target_route,
                        "{t(\"signup-register-link-prefix\")} "
                        span { class: "register-link__action", "{t(\"signup-register-link-generic\")}" }
                        span { class: "register-link__arrow", " →" }
                    }
                }
            }
        }
        SignupMethod::NotSupported => rsx! {},
    }
}

/// Extract `host[:port]` from a URL string for display in the link label.
///
/// Returns the full URL on parse failure — always produces a human-readable string.
fn extract_host(url: &str) -> String {
    // Simple extraction without pulling in the `url` crate.
    // Pattern: `<scheme>://<host>[/...]`
    url.split("://")
        .nth(1)
        .map_or(url, |after| after.split('/').next().unwrap_or(after))
        .to_string()
}
