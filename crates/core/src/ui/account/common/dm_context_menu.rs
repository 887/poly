//! Right-click / long-press context menu for a 1-on-1 DM in the DM list.
//!
//! State lives in `AppState.dm_context_menu`. Cleared by the global
//! `MainLayout` outside-click handler. Mirrors the channel/avatar menus.

use crate::i18n::t;
use crate::state::{AppState, BatchedSignal, ChatData};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User};
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn DmContextMenu() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    let Some(menu) = app_state.read().dm_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let channel_id = menu.channel_id.clone();
    let user_id = menu.user_id.clone();
    let display_name = menu.display_name.clone();
    let mut muted = use_signal(|| false);

    let close = move || {
        app_state.batch(|st| st.dm_context_menu = None);
    };

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.batch(|st| st.dm_context_menu = None);
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }
        div {
            class: "context-menu",
            // Clamp to viewport so the menu never opens off-screen.
            style: "left: min({x}px, calc(100vw - 220px)); top: min({y}px, calc(100vh - 320px));",
            onclick: move |evt| evt.stop_propagation(),

            div { class: "context-menu-label", "{display_name}" }
            div { class: "context-menu-separator" }

            // Mark as Read
            {
                let cid = channel_id.clone();
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("channel-menu-mark-read"),
                        onclick: move |_| {
                            mark_channel_as_read(chat_data, &cid);
                            close();
                        },
                    }
                }
            }

            // Profile
            {
                let uid = user_id.clone();
                let dname = display_name.clone();
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-profile"),
                        onclick: move |_| {
                            let user = User {
                                id: uid.clone(),
                                display_name: dname.clone(),
                                avatar_url: None,
                                presence: PresenceStatus::Offline,
                                backend: poly_client::BackendType::from("demo"),
                            };
                            open_user_profile(app_state, user);
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Mute (local toggle until backend mute lands)
            DmMenuItem {
                label: format!(
                    "{} @{}",
                    if muted() { t("dm-menu-unmute") } else { t("dm-menu-mute") },
                    display_name,
                ),
                onclick: move |_| muted.toggle(),
            }

            // Close DM (stub — backend hide-conversation not wired)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-close"),
                        onclick: move |_| {
                            tracing::debug!(target: "poly::context_menu", "close-dm stub");
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Display Name
            {
                let dname = display_name.clone();
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-copy-name"),
                        onclick: move |_| {
                            let dn = dname.clone();
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{dn}')"));
                            close();
                        },
                    }
                }
            }

            // Copy User ID
            {
                let uid = user_id.clone();
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-copy-user-id"),
                        onclick: move |_| {
                            let u = uid.clone();
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{u}')"));
                            close();
                        },
                    }
                }
            }

            // Copy Channel ID
            {
                let cid = channel_id.clone();
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("channel-menu-copy-id"),
                        onclick: move |_| {
                            let c = cid.clone();
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{c}')"));
                            close();
                        },
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn DmMenuItem(
    label: String,
    #[props(default = false)] danger: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: if danger { "context-menu-item danger" } else { "context-menu-item" },
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
        }
    }
}
