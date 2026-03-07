//! Electron-only custom title bar.
//!
//! Rendered only when the renderer detects the preload bridge exposed by the
//! Electron shell. This gives the app a themed, frameless top chrome instead of
//! the native OS window border.

use crate::i18n::t;
use dioxus::prelude::*;

#[component]
pub fn ElectronTitleBar() -> Element {
    let mut is_electron = use_signal(|| false);

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

    rsx! {
        div {
            class: "electron-titlebar",
            ondoubleclick: move |_| {
                let _ = document::eval("window.polyElectron?.toggleMaximize?.();");
            },
            div { class: "electron-titlebar-drag-region",
                span { class: "electron-titlebar-title", "{t(\"app-title\")}" }
            }
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
