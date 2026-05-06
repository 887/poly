//! Right-click / long-press context menu for a 1-on-1 DM in the DM list.
//!
//! Rendered via the `ContextMenuStack` host. State is pushed onto
//! `AppState.context_menu_stack` by `oncontextmenu` handlers on DM rows
//! (and long-press on mobile). The stack host dispatches here via
//! `register_menu(DM_MENU_TYPE, render_dm)`.
//!
//! Layout matches Discord's per-friend menu:
//!   Mark as Read
//!   ─
//!   Profile / Start a Call / Add Note / Add Friend Nickname / Close DM
//!   ─
//!   Invite to Server / Remove Friend / Ignore / Block
//!   ─
//!   Mute @username
//!   ─
//!   Copy Display Name / Copy User ID / Copy Channel ID

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AccountSessions, AppState, BatchedSignal, ChatLists, ChatViewState, DmContextMenuState, NavState, UiOverlays, VoiceState};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use crate::ui::account::common::direct_call::{
    DirectCallRequest, start_direct_call_from_active_account,
};
use crate::ui::account::common::user_profile_modal::open_user_profile;
use crate::ui::client_ui::toast::{ToastMessage, push_toast};
use dioxus::prelude::*;
use poly_client::{PresenceStatus, ToastTone, User};
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn DmContextMenuInner(menu: DmContextMenuState, close: EventHandler<()>) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let x = menu.x;
    let y = menu.y;
    let channel_id = menu.channel_id.clone();
    let user_id = menu.user_id.clone();
    let display_name = menu.display_name.clone();
    let account_id = menu.account_id.clone();
    let mark_read_disabled = menu.unread_count == 0;
    let mut muted = use_signal(|| false);

    rsx! {
        div {
            class: "context-menu",
            style: "left: min({x}px, calc(100vw - 220px)); top: min({y}px, calc(100vh - 520px));",
            onclick: move |evt| evt.stop_propagation(),

            // Mark as Read — host-side, no backend roundtrip.
            // Greyed out (disabled) when there's nothing to mark.
            {
                let cid = channel_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("channel-menu-mark-read"),
                        disabled: mark_read_disabled,
                        onclick: move |_| {
                            if mark_read_disabled { return; }
                            mark_channel_as_read(chat_lists, chat_view_state, &cid);
                            close.call(());
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Profile
            {
                let uid = user_id.clone();
                let dname = display_name.clone();
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
                            open_user_profile(ui_overlays, user);
                            close.call(());
                        },
                    }
                }
            }

            // Start a Call
            {
                let uid = user_id.clone();
                let dname = display_name.clone();
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
                                nav_state,
                                chat_lists,
                                account_sessions,
                                voice_state,
                                client_manager,
                            );
                            close.call(());
                        },
                    }
                }
            }

            // Add Note (TODO: needs a small inline-prompt UI; for now no-op
            // backend call with empty note clears, which is meaningless without
            // a text input. Stub-and-toast instead.)
            {
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-add-note"),
                        onclick: move |_| {
                            if let Some(q) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                                push_toast(q, ToastMessage::new("dm-action-coming-soon", ToastTone::Info));
                            }
                            close.call(());
                        },
                    }
                }
            }

            // Add Friend Nickname (same pattern as Add Note)
            {
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-add-nickname"),
                        onclick: move |_| {
                            if let Some(q) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                                push_toast(q, ToastMessage::new("dm-action-coming-soon", ToastTone::Info));
                            }
                            close.call(());
                        },
                    }
                }
            }

            // Close DM — backend `close_dm_channel`
            {
                let cid = channel_id.clone();
                let aid = account_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-close"),
                        onclick: move |_| {
                            let cid = cid.clone();
                            let aid = aid.clone();
                            spawn(async move {
                                drop(client_manager.peek().with_backend(&aid, async |b| {
                                    b.close_dm_channel(&cid).await
                                }).await);
                            });
                            close.call(());
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Invite to Server (TODO: submenu of joined servers)
            {
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-invite-to-server"),
                        onclick: move |_| {
                            if let Some(q) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                                push_toast(q, ToastMessage::new("dm-action-coming-soon", ToastTone::Info));
                            }
                            close.call(());
                        },
                    }
                }
            }

            // Remove Friend — backend `remove_friend`
            {
                let uid = user_id.clone();
                let aid = account_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-remove-friend"),
                        onclick: move |_| {
                            let uid = uid.clone();
                            let aid = aid.clone();
                            spawn(async move {
                                drop(client_manager.peek().with_backend(&aid, async |b| {
                                    if let Some(sg) = b.as_social_graph() {
                                        sg.remove_friend(&uid).await
                                    } else {
                                        Ok(())
                                    }
                                }).await);
                            });
                            close.call(());
                        },
                    }
                }
            }

            // Ignore — backend `ignore_user`
            {
                let uid = user_id.clone();
                let aid = account_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-ignore"),
                        onclick: move |_| {
                            let uid = uid.clone();
                            let aid = aid.clone();
                            spawn(async move {
                                drop(client_manager.peek().with_backend(&aid, async |b| {
                                    if let Some(sg) = b.as_social_graph() {
                                        sg.ignore_user(&uid).await
                                    } else {
                                        Ok(())
                                    }
                                }).await);
                            });
                            close.call(());
                        },
                    }
                }
            }

            // Block — backend `block_user`
            {
                let uid = user_id.clone();
                let aid = account_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-block"),
                        danger: true,
                        onclick: move |_| {
                            let uid = uid.clone();
                            let aid = aid.clone();
                            spawn(async move {
                                drop(client_manager.peek().with_backend(&aid, async |b| {
                                    if let Some(sg) = b.as_social_graph() {
                                        sg.block_user(&uid).await
                                    } else {
                                        Ok(())
                                    }
                                }).await);
                            });
                            close.call(());
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Mute @username — backend `mute_conversation` / `unmute_conversation`
            {
                let cid = channel_id.clone();
                let aid = account_id.clone();
                let dname = display_name.clone();
                rsx! {
                    DmMenuItem {
                        label: format!(
                            "{} @{}",
                            if muted() { t("dm-menu-unmute") } else { t("dm-menu-mute") },
                            dname,
                        ),
                        onclick: move |_| {
                            let cid = cid.clone();
                            let aid = aid.clone();
                            let was_muted = muted();
                            muted.toggle();
                            spawn(async move {
                                drop(client_manager.peek().with_backend(&aid, async |b| {
                                    if was_muted {
                                        b.unmute_conversation(&cid).await
                                    } else {
                                        b.mute_conversation(&cid, None).await
                                    }
                                }).await);
                            });
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Display Name
            {
                let dname = display_name.clone();
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-copy-name"),
                        onclick: move |_| {
                            let dn = dname.clone();
                            // lint-allow-unused: Eval is Copy — drop() is no-op, intentionally fire-and-forget
                            #[allow(let_underscore_drop, clippy::let_underscore_must_use)]
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{dn}')"));
                            close.call(());
                        },
                    }
                }
            }

            // Copy User ID
            {
                let uid = user_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("dm-menu-copy-user-id"),
                        onclick: move |_| {
                            let u = uid.clone();
                            // lint-allow-unused: Eval is Copy — drop() is no-op, intentionally fire-and-forget
                            #[allow(let_underscore_drop, clippy::let_underscore_must_use)]
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{u}')"));
                            close.call(());
                        },
                    }
                }
            }

            // Copy Channel ID
            {
                let cid = channel_id.clone();
                rsx! {
                    DmMenuItem {
                        label: t("channel-menu-copy-id"),
                        onclick: move |_| {
                            let c = cid.clone();
                            // lint-allow-unused: Eval is Copy — drop() is no-op, intentionally fire-and-forget
                            #[allow(let_underscore_drop, clippy::let_underscore_must_use)]
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{c}')"));
                            close.call(());
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
