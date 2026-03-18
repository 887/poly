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

#[cfg(target_arch = "wasm32")]
const MOBILE_DRAWER_CLOSE_JS: &str = "window.__polySetMobileDrawerOpen?.(false);";

#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_REQUEST_CLOSE_JS: &str = "window.__polyRequestCloseMobileRightWing?.();";

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

#[rustfmt::skip]
#[component]
pub(crate) fn RightWingShell(props: RightWingShellProps) -> Element {
    let panel_class = compose_shell_class(
        "chat-side-column poly-right-wing-panel",
        &props.panel_class,
    );

    rsx! {
        Fragment {
            button {
                class: "mobile-right-wing-backdrop",
                title: t("action-close"),
                onclick: move |_| request_close_mobile_right_wing(),
            }
            div { class: "{panel_class}",
                {props.content}
            }
        }
    }
}
