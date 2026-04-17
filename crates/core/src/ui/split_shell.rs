//! Shared split-shell container for left-menu + content views.
//!
//! This normalizes the repeated two-pane layouts used by:
//! - DM / server account shells (`channel-list-wrapper` + route content)
//! - chat right-wing rails (members, contacts, search/pins utility panel)
//! - app settings
//! - account settings
//! - server settings
//! - global search
//!
//! The goal is to keep a single structural wrapper so narrow/mobile drawer
//! behavior can target one shared split container instead of each page
//! hand-rolling its own shell.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;
#[cfg(target_arch = "wasm32")]
const MOBILE_DRAWER_CLOSE_JS: &str = "window.__polySetMobileDrawerOpen?.(false);";

#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_REQUEST_CLOSE_JS: &str = "window.__polyRequestCloseMobileRightWing?.();";

/// Inline right-panel resize handler — evaluated once via `document::eval` (fire-and-forget).
/// Using an external script via `load_js_asset` / `use_future` creates an orphaned Promise when
/// the component re-renders during hot-reload, producing "Unhandled promise rejection" crashes.
#[cfg(target_arch = "wasm32")]
const RIGHT_PANEL_RESIZE_INLINE_JS: &str = r#"
if (!window.__polyRightPanelResizerInit) {
    window.__polyRightPanelResizerInit = true;
    const MIN_WIDTH = 160, MAX_WIDTH = 500;
    let dragging = false;
    document.addEventListener('pointerdown', function(e) {
        const handle = e.target.closest('.right-panel-resizer');
        if (!handle) return;
        dragging = true;
        try { handle.setPointerCapture(e.pointerId); } catch (_) {}
        document.body.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
        e.preventDefault();
    }, true);
    document.addEventListener('pointermove', function(e) {
        if (!dragging) return;
        const w = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, window.innerWidth - e.clientX));
        document.documentElement.style.setProperty('--right-panel-width', w + 'px');
    }, true);
    document.addEventListener('pointerup', function() {
        if (!dragging) return;
        dragging = false;
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
    }, true);
}
"#;

#[derive(Props, Clone, PartialEq)]
pub(crate) struct SplitMenuShellProps {
    root_class: String,
    sidebar_class: String,
    content_class: String,
    sidebar: Element,
    content: Element,
}

#[derive(Props, Clone, PartialEq)]
pub(crate) struct RightWingShellProps {
    panel_class: String,
    content: Element,
}

fn compose_shell_class(base: &str, extra: &str) -> String {
    if extra.is_empty() {
        return base.to_string();
    }
    format!("{base} {extra}")
}

#[cfg(target_arch = "wasm32")]
fn toggle_mobile_left_wing() {
    let _ = document::eval("window.__polyToggleMobileDrawerOpen?.();");
}

#[cfg(target_arch = "wasm32")]
fn close_mobile_left_wing() {
    let _ = document::eval(MOBILE_DRAWER_CLOSE_JS);
}

#[cfg(target_arch = "wasm32")]
fn request_close_mobile_right_wing() {
    let _ = document::eval(MOBILE_RIGHT_WING_REQUEST_CLOSE_JS);
}

#[cfg(not(target_arch = "wasm32"))]
fn toggle_mobile_left_wing() {}

#[cfg(not(target_arch = "wasm32"))]
fn close_mobile_left_wing() {}

#[cfg(not(target_arch = "wasm32"))]
fn request_close_mobile_right_wing() {}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(crate) fn SplitMenuShell(props: SplitMenuShellProps) -> Element {
    let root_class = compose_shell_class("poly-split-shell", &props.root_class);
    let sidebar_class = compose_shell_class(
        "poly-split-sidebar poly-left-drawer-panel",
        &props.sidebar_class,
    );
    let content_class = compose_shell_class("poly-split-content", &props.content_class);

    rsx! {
        div { class: "{root_class}",
            div { class: "{sidebar_class}",
                {props.sidebar}
            }
            button {
                class: "mobile-left-wing-backdrop",
                title: t("action-close"),
                onclick: move |_| close_mobile_left_wing(),
            }
            div { class: "{content_class}",
                button {
                    class: "poly-mobile-left-wing-toggle",
                    title: t("mobile-nav-open"),
                    onclick: move |_| toggle_mobile_left_wing(),
                    "☰"
                }
                div { class: "poly-split-content-stage",
                    {props.content}
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(crate) fn RightWingShell(props: RightWingShellProps) -> Element {
    let panel_class = compose_shell_class(
        "chat-side-column poly-right-wing-panel",
        &props.panel_class,
    );

    // Install the drag-to-resize handler once. Fire-and-forget eval avoids an orphaned
    // Promise (which `load_js_asset` + `use_future` would create on hot-reload re-renders).
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let _ = document::eval(RIGHT_PANEL_RESIZE_INLINE_JS);
    });

    rsx! {
        Fragment {
            button {
                class: "mobile-right-wing-backdrop",
                title: t("action-close"),
                onclick: move |_| request_close_mobile_right_wing(),
            }
            div { class: "{panel_class}",
                // Drag handle — only visible on desktop, sits on the left edge of the panel.
                div { class: "right-panel-resizer" }
                {props.content}
            }
        }
    }
}
