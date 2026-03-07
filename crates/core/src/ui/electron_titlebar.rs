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

    // Derive the current page title from app state and chat data.
    // Mirrors what Discord shows: channel/DM name, page name, or app name.
    let title = {
        let state = app_state.read();
        // During setup wizard and initial loading, always show the app name.
        if !state.is_setup_complete {
            t("app-title")
        } else {
            let data = chat_data.read();
            match state.nav.view {
                View::DmsFriends => data
                    .current_channel
                    .as_ref()
                    .map(|ch| ch.name.clone())
                    .unwrap_or_else(|| t("nav-dms")),
                View::Friends => t("nav-friends"),
                View::Notifications => t("notifications-title"),
                View::Settings => t("settings-title"),
                View::Server => {
                    if let Some(ch) = &data.current_channel {
                        format!("# {}", ch.name)
                    } else if let Some(sv) = &data.current_server {
                        sv.name.clone()
                    } else {
                        t("app-title")
                    }
                }
                View::Setup => t("app-title"),
            }
        }
    };

    rsx! {
        div {
            class: "electron-titlebar",
            ondoubleclick: move |_| {
                let _ = document::eval("window.polyElectron?.toggleMaximize?.();");
            },

            // ── Left: back / forward navigation buttons ──────────────────
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

            // ── Center: draggable region with absolutely-centered page title ──
            // The title is `position: absolute; left: 50%` relative to the
            // `.electron-titlebar` (which has `position: relative`) so it is
            // always centered over the *entire* bar, independent of the widths
            // of the nav buttons and window controls on either side.
            div { class: "electron-titlebar-drag-region",
                span { class: "electron-titlebar-title", "{title}" }
            }

            // ── Right: window controls ────────────────────────────────────
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
}
