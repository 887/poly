//! Server right-click context menu component.
//!
//! Rendered at the `MainLayout` level (above sidebar overflow) so it is
//! never clipped by `overflow: hidden` on the sidebar containers.
//!
//! State lives in `AppState.context_menu`. Any `oncontextmenu` handler
//! in a server icon sets `app_state.write().context_menu = Some(...)`.
//! A global click on the `MainLayout` root div clears it.
//!
//! ## Menu items (host-universal)
//! - Mark as Read
//! - Unmute / Mute Server
//! - Notification Settings → ServerSettings::Notifications
//! - Hide Muted Channels (toggle)
//! - Show All Channels (toggle)
//! - Add to Favorites / Remove from Favorites (inline confirm)
//! - Leave Server → ServerSettings::General (inline confirm there)
//! - Copy Server ID
//!
//! Backend-specific items (Invite, Privacy, Per-server Profile, …) are now
//! declared by plugins via the `client-menus` WIT interface and rendered by
//! [`crate::ui::client_ui::ClientMenu`].

use crate::state::BatchedSignal;
use super::super::super::routes::Route;
use crate::i18n::{t, t_args};
use crate::state::{AppState, ChatData};
use crate::ui::client_ui::ClientMenu;
use dioxus::prelude::*;
use poly_client::MenuTargetKind;
use poly_ui_macros::{context_menu, ui_action};

/// Server right-click context menu.
///
/// Reads `AppState.context_menu` and renders a floating div at the stored
/// coordinates. Renders nothing when `context_menu` is `None`.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ServerContextMenu() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    let Some(menu) = app_state.read().context_menu.clone() else {
        return rsx! {};
    };

    let server_id = menu.server_id.clone();
    let server_name = menu.server_name.clone();
    let account_id = menu.account_id.clone();
    let instance_id = menu.instance_id.clone();
    let backend_slug = menu.backend_slug.clone();

    // Per-menu local toggles (in-memory for now)
    let mut hide_muted = use_signal(|| false);
    let mut show_all = use_signal(|| true);
    let mut muted = use_signal(|| false);
    let mut show_remove_confirm = use_signal(|| false);

    // Is this server in the user's favorites?
    let is_favorited = chat_data.read().favorited_server_ids.contains(&server_id);

    // Reset the inline-confirm flag anytime a different server menu is opened.
    {
        let menu_id = server_id.clone();
        // capture a mutable copy of the signal so we can call .set() later
        let mut show_remove_confirm = show_remove_confirm; // Signal is Copy, no clone needed
        use_effect(move || { // poly-lint: allow stale-effect-capture — show_remove_confirm.set() requires FnMut; server context menu remounts for each server; Signal::set() incompatible with use_reactive_effect (Fn-only)
            // `menu_id` is captured so the effect re-runs when the menu target
            // changes (including when a menu closes and reopens for a new server).
            let _ = &menu_id;
            show_remove_confirm.set(false);
            // no cleanup needed
        });
    }

    let close = move || {
        app_state.batch(|st| st.context_menu = None);
    };

    let x = menu.x;
    let y = menu.y;

    rsx! {
        // Transparent backdrop that closes the menu on click AND prevents native context menu
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.batch(|st| st.context_menu = None);
            },
            oncontextmenu: move |evt| {
                // Prevent browser's native context menu from appearing
                evt.prevent_default();
                // Let event propagate so the right-clicked element's handler can fire
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
                let iid = instance_id.clone();
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
                                    instance_id: iid.clone(),
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

            // Add / Remove Favorites
            if is_favorited {
                // Show remove from favorites with inline confirm
                if show_remove_confirm() {
                    RemoveFavoritesConfirm {
                        server_name: server_name.clone(),
                        server_id: server_id.clone(),
                        oncancel: move |_| show_remove_confirm.set(false),
                    }
                } else {
                    ContextMenuItem {
                        label: t("server-menu-remove-favorites"),
                        onclick: move |_| show_remove_confirm.set(true),
                    }
                }
            } else {
                // Add to favorites (no dialog, just add directly)
                ContextMenuItem {
                    label: t("server-menu-add-favorites"),
                    onclick: {
                        let sid = server_id.clone();
                        let mut close = close;
                        move |_| {
                            let new_favs = chat_data.batch(|cd| {
                                if !cd.favorited_server_ids.contains(&sid) {
                                    cd.favorited_server_ids.push(sid.clone());
                                }
                                cd.favorited_server_ids.clone()
                            });
                            spawn(async move {
                                crate::ui::favorites_sidebar::persist_favorites(new_favs)
                                    .await;
                            });
                            close();
                        }
                    },
                }
            }

            div { class: "context-menu-separator" }

            // Leave Server → server settings general tab (has inline confirm)
            {
                let sid = server_id.clone();
                let aid = account_id.clone();
                let iid = instance_id.clone();
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
                                    instance_id: iid.clone(),
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

            // Plugin-declared backend items (D10 — Invite, Privacy,
            // Per-server Profile, …). Rendered by `ClientMenu` which fetches
            // items from the backend's `client-menus` WIT interface.
            ClientMenu {
                target: MenuTargetKind::Server,
                target_id: server_id.clone(),
                account_id: account_id.clone(),
            }
        }
    }
}

/// A single clickable item inside the context menu.
#[ui_action(inherit)]
#[rustfmt::skip]
#[context_menu(inherit)]
#[component]
pub(crate) fn ContextMenuItem(
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
#[ui_action(inherit)]
#[context_menu(inherit)]
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

/// Inline confirm widget for removing a server from favorites.
///
/// Replaces the normal "Remove from Favorites" menu item while showing
/// a confirmation prompt inline. Does NOT use `window.confirm()`.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn RemoveFavoritesConfirm(
    server_name: String,
    server_id: String,
    oncancel: EventHandler<MouseEvent>,
) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    // Pre-compute the title using t_args so the Fluent {$name} placeholder is filled
    let remove_title = t_args("remove-favorites-title", &[("name", server_name.as_str())]);

    rsx! {
        div { class: "remove-favorites-confirm",
            h4 { class: "remove-favorites-confirm-title", "{remove_title}" }
            p { class: "remove-favorites-confirm-body", "{t(\"remove-favorites-body\")}" }
            div { class: "remove-favorites-confirm-actions",
                button {
                    class: "btn-secondary",
                    onclick: move |evt| oncancel.call(evt),
                    "{t(\"remove-favorites-cancel\")}"
                }
                button {
                    class: "btn-danger",
                    onclick: move |_evt| {
                        let sid = server_id.clone();
                        let new_favs = chat_data.batch(|cd| {
                            cd.favorited_server_ids.retain(|id| id != &sid);
                            cd.favorited_server_ids.clone()
                        });
                        spawn(async move {
                            crate::ui::favorites_sidebar::persist_favorites(new_favs)
                                .await;
                        });
                        // Close menu
                        app_state.batch(|st| st.context_menu = None);
                    },
                    "{t(\"remove-favorites-confirm\")}"
                }
            }
        }
    }
}
