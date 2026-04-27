//! Right-click / long-press context menu for a group DM in the DM list.
//!
//! Layout matches Discord's group menu:
//!   Mark as Read
//!   ─
//!   Edit Group / Invite Friends to Group DM
//!   ─
//!   Mute Conversation / Leave Conversation
//!   ─
//!   Copy Channel ID

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal, ChatData};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use crate::ui::client_ui::toast::{ToastMessage, push_toast};
use dioxus::prelude::*;
use poly_client::ToastTone;
use poly_ui_macros::{context_menu, ui_action};
use std::time::Duration;

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn GroupDmContextMenu() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let Some(menu) = app_state.read().group_dm_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let channel_id = menu.channel_id.clone();
    let account_id = menu.account_id.clone();
    let mark_read_disabled = menu.unread_count == 0;
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
            style: "left: min({x}px, calc(100vw - 220px)); top: min({y}px, calc(100vh - 280px));",
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read — host-side. Greyed when nothing to mark.
            {
                let cid = channel_id.clone();
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("channel-menu-mark-read"),
                        disabled: mark_read_disabled,
                        onclick: move |_| {
                            if mark_read_disabled { return; }
                            mark_channel_as_read(chat_data, &cid);
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Edit Group — needs an inline form; for now, toast "coming soon"
            {
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("group-dm-menu-edit"),
                        onclick: move |_| {
                            if let Some(q) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                                push_toast(q, ToastMessage::new("dm-action-coming-soon", ToastTone::Info));
                            }
                            close();
                        },
                    }
                }
            }

            // Invite Friends to Group DM — needs a friend picker modal
            {
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("group-dm-menu-invite"),
                        onclick: move |_| {
                            if let Some(q) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                                push_toast(q, ToastMessage::new("dm-action-coming-soon", ToastTone::Info));
                            }
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Mute Conversation — backend `mute_conversation` / `unmute_conversation`
            {
                let cid = channel_id.clone();
                let aid = account_id.clone();
                rsx! {
                    GroupDmMenuItem {
                        label: if muted() { t("group-dm-menu-unmute") } else { t("group-dm-menu-mute") },
                        onclick: move |_| {
                            let cid = cid.clone();
                            let aid = aid.clone();
                            let was_muted = muted();
                            muted.toggle();
                            spawn(async move {
                                if let Some(handle) = client_manager.read().get_backend(&aid)
                                    && let Ok(backend) = handle
                                        .read_with_timeout(Duration::from_secs(5))
                                        .await
                                {
                                    let _ = if was_muted {
                                        backend.unmute_conversation(&cid).await
                                    } else {
                                        backend.mute_conversation(&cid, None).await
                                    };
                                }
                            });
                        },
                    }
                }
            }

            // Leave Conversation — backend `leave_group_dm`
            {
                let cid = channel_id.clone();
                let aid = account_id.clone();
                let mut close = close;
                rsx! {
                    GroupDmMenuItem {
                        label: t("group-dm-menu-leave"),
                        danger: true,
                        onclick: move |_| {
                            let cid = cid.clone();
                            let aid = aid.clone();
                            spawn(async move {
                                if let Some(handle) = client_manager.read().get_backend(&aid)
                                    && let Ok(backend) = handle
                                        .read_with_timeout(Duration::from_secs(5))
                                        .await
                                {
                                    let _ = backend.leave_group_dm(&cid).await;
                                }
                            });
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
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if disabled {
        "context-menu-item disabled"
    } else if danger {
        "context-menu-item danger"
    } else {
        "context-menu-item"
    };
    rsx! {
        div {
            class: "{class}",
            onclick: move |evt| if !disabled { onclick.call(evt); },
            span { "{label}" }
        }
    }
}
