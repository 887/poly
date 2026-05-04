//! Attachment (image) right-click context menu component.
//!
//! Rendered via the `ContextMenuStack` host at the `MainLayout` level.
//! Opened by right-clicking an image attachment in chat or inside the
//! full-screen `MessageMediaViewerOverlay`. State is pushed onto
//! `AppState.context_menu_stack`.
//!
//! ## Menu items (Discord-parity)
//! - Copy Image — best-effort fetch + `navigator.clipboard.write([blob])`
//! - Save Image — synthetic `<a href download>` click
//! - Copy Media Link — `navigator.clipboard.writeText(url)`
//! - Open Media Link — `window.open(url, '_blank')`

use crate::i18n::t;
use crate::state::AttachmentContextMenuState;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Attachment right-click context menu — stack-based inner component.
///
/// Receives the deserialized `AttachmentContextMenuState` from the stack host
/// and a `close` callback to pop itself off the stack.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AttachmentContextMenuInner(menu: AttachmentContextMenuState, close: EventHandler<()>) -> Element {
    let x = menu.x;
    let y = menu.y;
    let url = menu.url.clone();
    let filename = menu.filename.clone();

    rsx! {
        div {
            class: "context-menu",
            // z-index: sit above the media viewer overlay (.poly-media-viewer-overlay
            // is z-index 2100). Without this override the menu renders behind
            // the viewer and the user sees no menu at all.
            style: "left: {x}px; top: {y}px; z-index: 2200;",
            onclick: move |evt| evt.stop_propagation(),

            // Copy Image — fetch the bytes and put them on the clipboard as a blob.
            {
                let u = url.clone();
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-copy-image"),
                        onclick: move |_| {
                            let js = format!(
                                "(async () => {{\n  try {{\n    const r = await fetch({url});\n    const b = await r.blob();\n    await navigator.clipboard.write([new ClipboardItem({{[b.type]: b}})]);\n  }} catch (e) {{ console.warn('copy image failed:', e); }}\n}})();",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                            );
                            // lint-allow-unused: Eval is fire-and-forget here (Copy + Future).
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = document::eval(&js);
                            close.call(());
                        },
                    }
                }
            }

            // Save Image — trigger a download via a synthetic anchor click.
            {
                let u = url.clone();
                let f = filename.clone();
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-save-image"),
                        onclick: move |_| {
                            let js = format!(
                                "(() => {{\n  const a = document.createElement('a');\n  a.href = {url};\n  a.download = {name};\n  a.target = '_blank';\n  a.rel = 'noopener noreferrer';\n  document.body.appendChild(a);\n  a.click();\n  a.remove();\n}})();",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                                name = serde_json::to_string(&f).unwrap_or_else(|_| "\"\"".into()),
                            );
                            // lint-allow-unused: Eval is fire-and-forget here (Copy + Future).
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = document::eval(&js);
                            close.call(());
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Media Link.
            {
                let u = url.clone();
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-copy-link"),
                        onclick: move |_| {
                            let js = format!(
                                "navigator.clipboard.writeText({url}).catch((e) => console.warn('copy link failed:', e));",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                            );
                            // lint-allow-unused: Eval is fire-and-forget here (Copy + Future).
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = document::eval(&js);
                            close.call(());
                        },
                    }
                }
            }

            // Open Media Link.
            {
                let u = url.clone();
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-open-link"),
                        onclick: move |_| {
                            let js = format!(
                                "window.open({url}, '_blank', 'noopener,noreferrer');",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                            );
                            // lint-allow-unused: Eval is fire-and-forget here (Copy + Future).
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = document::eval(&js);
                            close.call(());
                        },
                    }
                }
            }
        }
    }
}

/// A single clickable item inside the attachment context menu.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn AttachmentMenuItem(
    label: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "context-menu-item",
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
        }
    }
}
