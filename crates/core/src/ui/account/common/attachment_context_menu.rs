//! Attachment (image) right-click context menu component.
//!
//! Rendered at the `MainLayout` level so it is never clipped by sidebars.
//! Opened by right-clicking an image attachment in chat or inside the
//! full-screen `MessageMediaViewerOverlay`.
//!
//! State lives in `AppState.attachment_context_menu`. The `oncontextmenu`
//! handler on the host element writes `Some(AttachmentContextMenuState)`.
//! A global click on the `MainLayout` root clears it.
//!
//! ## Menu items (Discord-parity)
//! - Copy Image — best-effort fetch + `navigator.clipboard.write([blob])`
//! - Save Image — synthetic `<a href download>` click
//! - Copy Media Link — `navigator.clipboard.writeText(url)`
//! - Open Media Link — `window.open(url, '_blank')`

use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Attachment right-click context menu.
///
/// Reads `AppState.attachment_context_menu` and renders a floating div at
/// the stored coordinates. Renders nothing when the state is `None`.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AttachmentContextMenu() -> Element {
    let mut app_state: Signal<AppState> = use_context();

    let Some(menu) = app_state.read().attachment_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let url = menu.url.clone();
    let filename = menu.filename.clone();

    let close = move || {
        app_state.write().attachment_context_menu = None;
    };

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.write().attachment_context_menu = None;
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }

        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |evt| evt.stop_propagation(),

            // Copy Image — fetch the bytes and put them on the clipboard as a blob.
            {
                let u = url.clone();
                let mut close = close;
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-copy-image"),
                        onclick: move |_| {
                            let js = format!(
                                "(async () => {{\n  try {{\n    const r = await fetch({url});\n    const b = await r.blob();\n    await navigator.clipboard.write([new ClipboardItem({{[b.type]: b}})]);\n  }} catch (e) {{ console.warn('copy image failed:', e); }}\n}})();",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                            );
                            let _eval = document::eval(&js);
                            close();
                        },
                    }
                }
            }

            // Save Image — trigger a download via a synthetic anchor click.
            {
                let u = url.clone();
                let f = filename.clone();
                let mut close = close;
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-save-image"),
                        onclick: move |_| {
                            let js = format!(
                                "(() => {{\n  const a = document.createElement('a');\n  a.href = {url};\n  a.download = {name};\n  a.target = '_blank';\n  a.rel = 'noopener noreferrer';\n  document.body.appendChild(a);\n  a.click();\n  a.remove();\n}})();",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                                name = serde_json::to_string(&f).unwrap_or_else(|_| "\"\"".into()),
                            );
                            let _eval = document::eval(&js);
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Media Link.
            {
                let u = url.clone();
                let mut close = close;
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-copy-link"),
                        onclick: move |_| {
                            let js = format!(
                                "navigator.clipboard.writeText({url}).catch((e) => console.warn('copy link failed:', e));",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                            );
                            let _eval = document::eval(&js);
                            close();
                        },
                    }
                }
            }

            // Open Media Link.
            {
                let u = url.clone();
                let mut close = close;
                rsx! {
                    AttachmentMenuItem {
                        label: t("attachment-menu-open-link"),
                        onclick: move |_| {
                            let js = format!(
                                "window.open({url}, '_blank', 'noopener,noreferrer');",
                                url = serde_json::to_string(&u).unwrap_or_else(|_| "\"\"".into()),
                            );
                            let _eval = document::eval(&js);
                            close();
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
