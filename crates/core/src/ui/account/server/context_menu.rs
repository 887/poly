//! Server right-click context menu component.
//!
//! Rendered at the `MainLayout` level (above sidebar overflow) so it is
//! never clipped by `overflow: hidden` on the sidebar containers.
//!
//! State lives in `AppState.context_menu`. Any `oncontextmenu` handler
//! in a server icon sets `app_state.write().context_menu = Some(...)`.
//! A global click on the `MainLayout` root div clears it.
//!
//! ## Menu items
//! - Mark as Read
//! - Invite to Server
//! - Unmute / Mute Server
//! - Notification Settings → ServerSettings::Notifications
//! - Hide Muted Channels (toggle)
//! - Show All Channels (toggle)
//! - Privacy Settings → ServerSettings::General (stub)
//! - Edit Per-server Profile → ServerSettings::Profile
//! - Leave Server → ServerSettings::General (inline confirm there)
//! - Copy Server ID

use super::super::super::routes::Route;
use super::super::backend_server_context_menu_extras;
use crate::i18n::t;
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
use poly_client::BackendType;

/// Server right-click context menu.
///
/// Reads `AppState.context_menu` and renders a floating div at the stored
/// coordinates. Renders nothing when `context_menu` is `None`.
#[component]
pub fn ServerContextMenu() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let Some(menu) = app_state.read().context_menu.clone() else {
        return rsx! {};
    };

    let server_id = menu.server_id.clone();
    let _server_name = menu.server_name.clone();
    let account_id = menu.account_id.clone();
    let backend_slug = menu.backend_slug.clone();

    // Per-menu local toggles (in-memory for now)
    let mut hide_muted = use_signal(|| false);
    let mut show_all = use_signal(|| true);
    let mut muted = use_signal(|| false);

    // Is this server in the user's view?
    // TODO(phase-3): adapt menu items based on favorites state
    let _is_favorited = chat_data.read().favorited_server_ids.contains(&server_id);

    let close = move || {
        app_state.write().context_menu = None;
    };

    let x = menu.x;
    let y = menu.y;

    rsx! {
        // Transparent backdrop that closes the menu on click
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.write().context_menu = None;
            },
        }

        // The context menu itself
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            // Stop click from bubbling to backdrop
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read
            ContextMenuItem {
                label: t("server-menu-mark-read"),
                onclick: {
                    let mut close = close;
                    move |_| {
                        // TODO(phase-3): mark all channels read via backend
                        close();
                    }
                },
            }

            // Invite to Server
            ContextMenuItem {
                label: t("server-menu-invite"),
                onclick: {
                    let mut close = close;
                    move |_| {
                        // TODO(phase-3): generate invite URL
                        close();
                    }
                },
            }

            div { class: "context-menu-separator" }

            // Mute / Unmute toggle
            ContextMenuItem {
                label: if muted() { t("server-menu-unmute") } else { t("server-menu-mute") },
                onclick: move |_| muted.toggle(),
            }

            // Notification Settings → server settings notifications tab
            {
                let sid = server_id.clone();
                let aid = account_id.clone();
                let bslug = backend_slug.clone();
                let mut close = close;
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-notif-settings"),
                        has_arrow: true,
                        onclick: move |_| {
                            close();
                            navigator()
                                .push(Route::ServerSettingsRoute {
                                    backend: bslug.clone(),
                                    account_id: aid.clone(),
                                    server_id: sid.clone(),
                                });
                        },
                    }
                }
            }

            // Hide Muted Channels toggle
            ContextMenuToggle {
                label: t("server-menu-hide-muted"),
                checked: hide_muted(),
                onclick: move |_| hide_muted.toggle(),
            }

            // Show All Channels toggle
            ContextMenuToggle {
                label: t("server-menu-show-all"),
                checked: show_all(),
                onclick: move |_| show_all.toggle(),
            }

            div { class: "context-menu-separator" }

            // Privacy Settings → server settings general (stub)
            {
                let sid = server_id.clone();
                let aid = account_id.clone();
                let bslug = backend_slug.clone();
                let mut close = close;
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-privacy"),
                        onclick: move |_| {
                            close();
                            navigator()
                                .push(Route::ServerSettingsRoute {
                                    backend: bslug.clone(),
                                    account_id: aid.clone(),
                                    server_id: sid.clone(),
                                });
                        },
                    }
                }
            }

            // Edit Per-server Profile → server settings profile tab
            {
                let sid = server_id.clone();
                let aid = account_id.clone();
                let bslug = backend_slug.clone();
                let mut close = close;
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-edit-profile"),
                        onclick: move |_| {
                            close();
                            navigator()
                                .push(Route::ServerSettingsRoute {
                                    backend: bslug.clone(),
                                    account_id: aid.clone(),
                                    server_id: sid.clone(),
                                });
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Leave Server → server settings general tab (has inline confirm)
            {
                let sid = server_id.clone();
                let aid = account_id.clone();
                let bslug = backend_slug.clone();
                let mut close = close;
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-leave"),
                        danger: true,
                        onclick: move |_| {
                            close();
                            navigator()
                                .push(Route::ServerSettingsRoute {
                                    backend: bslug.clone(),
                                    account_id: aid.clone(),
                                    server_id: sid.clone(),
                                });
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Server ID
            {
                let sid = server_id.clone();
                let mut close = close;
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-copy-id"),
                        onclick: move |_| {
                            let sid2 = sid.clone();
                            let _eval = document::eval(&format!("navigator.clipboard.writeText('{sid2}')"));
                            close();
                        },
                    }
                }
            }

            // ── Backend-specific extras ───────────────────────────────
            // Dispatches to per-backend context menu modules (demo/, stoat/,
            // discord/, matrix/, teams/, poly_native/) based on BackendType.
            // DECISION(D20): Per-backend UI dispatch by BackendType match.
            {
                let backend = BackendType::from_slug(&backend_slug);
                rsx! {
                    {backend_server_context_menu_extras(backend, &server_id, &account_id)}
                }
            }
        }
    }
}

/// A single clickable item inside the context menu.
#[component]
fn ContextMenuItem(
    label: String,
    #[props(default = false)] danger: bool,
    #[props(default = false)] has_arrow: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: if danger { "context-menu-item danger" } else { "context-menu-item" },
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
            if has_arrow {
                span { class: "context-menu-arrow", "›" }
            }
        }
    }
}

/// A toggleable item inside the context menu (checkbox style).
#[component]
fn ContextMenuToggle(label: String, checked: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            class: "context-menu-item context-menu-toggle",
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
            span { class: if checked { "context-menu-toggle-box checked" } else { "context-menu-toggle-box" },
                if checked {
                    "☑"
                } else {
                    "☐"
                }
            }
        }
    }
}
