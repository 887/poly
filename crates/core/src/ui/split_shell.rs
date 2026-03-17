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

use dioxus::prelude::*;

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
            div { class: "{content_class}",
                {props.content}
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
        div { class: "{panel_class}",
            {props.content}
        }
    }
}
