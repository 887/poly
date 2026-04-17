//! Electron-only custom title bar.
//!
//! Rendered only when the renderer detects the preload bridge exposed by the
//! Electron shell. This gives the app a themed, frameless top chrome with:
//!   - Back / Forward navigation buttons on the left (Discord-style)
//!   - Dynamically generated page title in the absolute center
//!   - Window controls (minimize / maximize / close) on the right
//!
//! The component is mounted above the Router (in the App root) so it appears
//! on all screens — loading screen, setup wizard, and the main layout.
//! Because it sits outside the Router it reads route information from
//! `AppState.nav` / `ChatData.current_channel` via context instead of
//! `use_route()`.

use crate::i18n::t;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

fn current_title(app_state: &AppState, chat_data: &ChatData) -> String {
    if !app_state.is_setup_complete {
        return t("app-title");
    }

    match app_state.nav.view {
        View::DmsFriends => chat_data
            .current_channel
            .as_ref()
            .map(|ch| ch.name.clone())
            .unwrap_or_else(|| t("nav-dms")),
        View::Friends => t("nav-friends"),
        View::Notifications => t("notifications-title"),
        View::Settings => t("settings-title"),
        View::Search => t("search-page-title"),
        View::Server => {
            if let Some(ch) = &chat_data.current_channel {
                format!("# {}", ch.name)
            } else if let Some(sv) = &chat_data.current_server {
                sv.name.clone()
            } else {
                t("app-title")
            }
        }
        View::Setup | View::Signup => t("app-title"),
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ElectronNavButtons() -> Element {
    rsx! {
        div { class: "electron-nav-buttons",
            button {
                class: "electron-window-btn electron-nav-btn",
                title: "{t(\"nav-back\")}",
                onclick: move |_| {
                    let _ = document::eval("history.back();");
                },
                "←"
            }
            button {
                class: "electron-window-btn electron-nav-btn",
                title: "{t(\"nav-forward\")}",
                onclick: move |_| {
                    let _ = document::eval("history.forward();");
                },
                "→"
            }
        }
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ElectronWindowControls() -> Element {
    rsx! {
        div { class: "electron-window-controls",
            button {
                class: "electron-window-btn",
                title: "{t(\"electron-window-minimize\")}",
                onclick: move |_| {
                    let _ = document::eval("window.polyElectron?.minimize?.();");
                },
                "—"
            }
            button {
                class: "electron-window-btn",
                title: "{t(\"electron-window-maximize\")}",
                onclick: move |_| {
                    let _ = document::eval("window.polyElectron?.toggleMaximize?.();");
                },
                "▢"
            }
            button {
                class: "electron-window-btn close",
                title: "{t(\"electron-window-close\")}",
                onclick: move |_| {
                    let _ = document::eval("window.polyElectron?.closeWindow?.();");
                },
                "✕"
            }
        }
    }
}

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ElectronTitleBar() -> Element {
    let mut is_electron = use_signal(|| false);
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    use_future(move || async move {
        let mut eval = document::eval("dioxus.send(Boolean(window.polyElectron?.isElectron));");
        if let Ok(value) = eval.recv::<bool>().await {
            is_electron.set(value);
        }
    });

    if !*is_electron.read() {
        return rsx! {
            Fragment {}
        };
    }

    let title = current_title(&app_state.read(), &chat_data.read());

    // Keep the OS-level window title (taskbar/dock) in sync with the custom titlebar.
    let title_for_doc = title.clone();
    use_effect(move || {
        let escaped = title_for_doc.replace('\\', "\\\\").replace('`', "\\`");
        let _ = document::eval(&format!("document.title = `{escaped}`;"));
    });

    rsx! {
        div {
            class: "electron-titlebar",
            ondoubleclick: move |_| {
                let _ = document::eval("window.polyElectron?.toggleMaximize?.();");
            },
            ElectronNavButtons {}
            div { class: "electron-titlebar-drag-region",
                span { class: "electron-titlebar-title", "{title}" }
            }
            ElectronWindowControls {}
        }
    }
}
