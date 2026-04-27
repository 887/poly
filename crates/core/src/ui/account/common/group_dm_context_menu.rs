//! Right-click / long-press context menu for a group DM in the DM list.
//!
//! Mirrors `DmContextMenu` but tailored for group conversations
//! (Edit Group, Invite Friends, Leave Conversation).

use crate::i18n::t;
use crate::state::{AppState, BatchedSignal, ChatData};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn GroupDmContextMenu() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    let Some(menu) = app_state.read().group_dm_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let channel_id = menu.channel_id.clone();
    let mut muted = use_signal(|| false);

    let close = move || {
        app_state.batch(|st| st.group_dm_context_menu = None);
    };

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.batch(|st| st.group_dm_context_menu = None);
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }
        div {
            class: "context-menu",
            // Clamp to viewport so the menu never opens off-screen.
            style: "left: min({x}px, calc(100vw - 220px)); top: min({y}px, calc(100vh - 280px));",
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read
            {
                let cid = channel_id.clone();
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("channel-menu-mark-read"),
                        onclick: move |_| {
                            mark_channel_as_read(chat_data, &cid);
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Edit Group (stub — group settings dialog not yet wired)
            {
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("group-dm-menu-edit"),
                        onclick: move |_| {
                            tracing::debug!(target: "poly::context_menu", "edit-group stub");
                            close();
                        },
                    }
                }
            }

            // Invite Friends (stub)
            {
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("group-dm-menu-invite"),
                        onclick: move |_| {
                            tracing::debug!(target: "poly::context_menu", "invite-friends stub");
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Mute Conversation (local toggle)
            GroupDmMenuItem {
                label: if muted() {
                    t("group-dm-menu-unmute")
                } else {
                    t("group-dm-menu-mute")
                },
                onclick: move |_| muted.toggle(),
            }

            // Leave Conversation (stub — backend leave-group not wired)
            {
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("group-dm-menu-leave"),
                        danger: true,
                        onclick: move |_| {
                            tracing::debug!(target: "poly::context_menu", "leave-group stub");
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Channel ID
            {
                let cid = channel_id.clone();
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
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
fn GroupDmMenuItem(
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
