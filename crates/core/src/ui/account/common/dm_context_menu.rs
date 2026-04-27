//! Right-click / long-press context menu for a 1-on-1 DM in the DM list.
//!
//! Layout matches Discord's per-friend menu (see screenshot in PR thread):
//!   Mark as Read
//!   ─────────
//!   Profile
//!   Start a Call
//!   Add Note
//!   Add Friend Nickname
//!   Close DM
//!   ─────────
//!   Invite to Server
//!   Remove Friend
//!   Ignore
//!   Block
//!   ─────────
//!   Mute @username
//!   ─────────
//!   Copy Display Name
//!   Copy User ID
//!   Copy Channel ID
//!
//! Items wired today: Mark as Read, Profile, Start a Call, Mute (local
//! toggle), all Copy operations. The remaining items emit a debug trace
//! and close — they need backend hooks (`remove-friend`, `block-user`,
//! `ignore-user`, `close-dm`, `set-friend-nickname`, `set-user-note`,
//! `invite-user-to-server`) that aren't in `ClientBackend` yet. The UI is
//! kept fully populated so visual parity with Discord lands now and
//! backend wiring is a drop-in next pass.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal, ChatData};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use crate::ui::account::common::direct_call::{
    DirectCallRequest, start_direct_call_from_active_account,
};
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
    let client_manager: BatchedSignal<ClientManager> = use_context();

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

    let stub = move |action: &'static str| {
        tracing::debug!(target: "poly::context_menu", "dm-menu stub: {action}");
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
            style: "left: min({x}px, calc(100vw - 220px)); top: min({y}px, calc(100vh - 520px));",
            onclick: move |evt| evt.stop_propagation(),

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

            div { class: "context-menu-separator" }

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

            // Start a Call
            {
                let uid = user_id.clone();
                let dname = display_name.clone();
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-start-call"),
                        onclick: move |_| {
                            let target = User {
                                id: uid.clone(),
                                display_name: dname.clone(),
                                avatar_url: None,
                                presence: PresenceStatus::Offline,
                                backend: poly_client::BackendType::from("demo"),
                            };
                            start_direct_call_from_active_account(
                                DirectCallRequest {
                                    target_user: target,
                                    start_video: false,
                                    allow_add_to_active_temporary: true,
                                },
                                app_state,
                                chat_data,
                                client_manager,
                            );
                            close();
                        },
                    }
                }
            }

            // Add Note (TODO: backend `set-user-note`)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-add-note"),
                        onclick: move |_| { stub("add-note"); close(); },
                    }
                }
            }

            // Add Friend Nickname (TODO: backend `set-friend-nickname`)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-add-nickname"),
                        onclick: move |_| { stub("add-nickname"); close(); },
                    }
                }
            }

            // Close DM (TODO: backend `close-dm`)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-close"),
                        onclick: move |_| { stub("close-dm"); close(); },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Invite to Server (TODO: submenu of joined servers)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-invite-to-server"),
                        onclick: move |_| { stub("invite-to-server"); close(); },
                    }
                }
            }

            // Remove Friend (TODO: backend `remove-friend`)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-remove-friend"),
                        onclick: move |_| { stub("remove-friend"); close(); },
                    }
                }
            }

            // Ignore (TODO: backend `ignore-user`)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-ignore"),
                        onclick: move |_| { stub("ignore"); close(); },
                    }
                }
            }

            // Block (TODO: backend `block-user`)
            {
                let mut close = close;
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-block"),
                        danger: true,
                        onclick: move |_| { stub("block"); close(); },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Mute @username (local toggle)
            DmMenuItem {
                label: format!(
                    "{} @{}",
                    if muted() { t("dm-menu-unmute") } else { t("dm-menu-mute") },
                    display_name,
                ),
                onclick: move |_| muted.toggle(),
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
