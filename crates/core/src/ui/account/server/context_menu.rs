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
use crate::state::{AccountSessions, AppState, ContextMenuState};
use crate::ui::client_ui::ClientMenu;
use dioxus::prelude::*;
use poly_client::MenuTargetKind;
use poly_ui_macros::{context_menu, ui_action};

/// Server right-click context menu — stack-based inner component.
///
/// Receives the deserialized `ContextMenuState` from the stack host and
/// a `close` callback to pop itself off the stack when the user dismisses.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ServerContextMenuInner(menu: ContextMenuState, close: EventHandler<()>) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();

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
    let is_favorited = account_sessions.read().favorited_server_ids.contains(&server_id);

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

    let x = menu.x;
    let y = menu.y;

    rsx! {
        // The context menu itself — backdrop + dismiss is handled by the stack host.
        div {
            class: "context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read
            ContextMenuItem {
                label: t("server-menu-mark-read"),
                onclick: {
                    move |_| {
                        // TODO(phase-3): mark all channels read via backend
                        close.call(());
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
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-notif-settings"),
                        has_arrow: true,
                        onclick: move |_| {
                            close.call(());
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
                        onconfirm: move |()| close.call(()),
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
                        move |_| {
                            let new_favs = account_sessions.batch(|as_| {
                                if !as_.favorited_server_ids.contains(&sid) {
                                    as_.favorited_server_ids.push(sid.clone());
                                }
                                as_.favorited_server_ids.clone()
                            });
                            spawn(async move {
                                crate::ui::favorites_sidebar::persist_favorites(new_favs)
                                    .await;
                            });
                            close.call(());
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
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-leave"),
                        danger: true,
                        onclick: move |_| {
                            close.call(());
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
                rsx! {
                    ContextMenuItem {
                        label: t("server-menu-copy-id"),
                        onclick: move |_| {
                            let sid2 = sid.clone();
                            #[allow(clippy::let_underscore_must_use)] let _ = document::eval(&format!("navigator.clipboard.writeText('{sid2}')"));
                            close.call(());
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
    onconfirm: EventHandler<()>,
) -> Element {
    let account_sessions: BatchedSignal<AccountSessions> = use_context();

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
                        let new_favs = account_sessions.batch(|as_| {
                            as_.favorited_server_ids.retain(|id| id != &sid);
                            as_.favorited_server_ids.clone()
                        });
                        spawn(async move {
                            crate::ui::favorites_sidebar::persist_favorites(new_favs)
                                .await;
                        });
                        // Close menu via stack pop
                        onconfirm.call(());
                    },
                    "{t(\"remove-favorites-confirm\")}"
                }
            }
        }
    }
}
