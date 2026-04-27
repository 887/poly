//! Right-click context menu for account icons in the leftmost favorites bar.
//!
//! Items: Mark Account as Read / Account Settings / Sign Out / Copy Account ID.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::nav;
use crate::state::{AppState, BatchedSignal, ChatData};
use crate::ui::account::common::chat_view::mark_channel_as_read;
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountContextMenu() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let mut client_manager: BatchedSignal<ClientManager> = use_context();

    let Some(menu) = app_state.read().account_context_menu.clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let account_id = menu.account_id.clone();
    let display_name = menu.display_name.clone();
    let backend_slug = menu.backend_slug.clone();
    let instance_id = menu.instance_id.clone();

    // Total unread for this account = sum across all channels + DMs.
    let account_unread: u32 = {
        let cd = chat_data.read();
        let from_channels: u32 = cd
            .channels
            .iter()
            .filter(|c| !c.id.is_empty())
            .map(|c| c.unread_count)
            .sum();
        let from_dms: u32 = cd
            .dm_channels
            .iter()
            .filter(|d| d.account_id == account_id)
            .map(|d| d.unread_count)
            .sum();
        from_channels.saturating_add(from_dms)
    };
    let mark_read_disabled = account_unread == 0;

    let close = move || {
        app_state.batch(|st| st.account_context_menu = None);
    };

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| {
                app_state.batch(|st| st.account_context_menu = None);
            },
            oncontextmenu: move |evt| evt.prevent_default(),
        }
        div {
            class: "context-menu",
            style: "left: min({x}px, calc(100vw - 220px)); top: min({y}px, calc(100vh - 220px));",
            onclick: move |evt| evt.stop_propagation(),

            div { class: "context-menu-label", "{display_name}" }
            div { class: "context-menu-separator" }

            // Mark Account as Read — sweep every DM + channel for this account.
            {
                let aid = account_id.clone();
                let mut close = close;
                rsx! {
                    AccountMenuItem {
                        label: t("account-menu-mark-read"),
                        disabled: mark_read_disabled,
                        onclick: move |_| {
                            if mark_read_disabled { return; }
                            let dm_ids: Vec<String> = chat_data
                                .read()
                                .dm_channels
                                .iter()
                                .filter(|d| d.account_id == aid && d.unread_count > 0)
                                .map(|d| d.id.clone())
                                .collect();
                            let chan_ids: Vec<String> = chat_data
                                .read()
                                .channels
                                .iter()
                                .filter(|c| c.unread_count > 0)
                                .map(|c| c.id.clone())
                                .collect();
                            for id in dm_ids.iter().chain(chan_ids.iter()) {
                                mark_channel_as_read(chat_data, id);
                            }
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Account Settings
            {
                let backend = backend_slug.clone();
                let inst = instance_id.clone();
                let aid = account_id.clone();
                let mut close = close;
                rsx! {
                    AccountMenuItem {
                        label: t("account-menu-settings"),
                        onclick: move |_| {
                            nav!(Route::AccountSettingsRoute {
                                backend: backend.clone(),
                                instance_id: inst.clone(),
                                account_id: aid.clone(),
                            });
                            close();
                        },
                    }
                }
            }

            // Sign Out — disconnect the backend (keeps stored token; user
            // can re-enable from Settings → Plugins).
            {
                let aid = account_id.clone();
                let mut close = close;
                rsx! {
                    AccountMenuItem {
                        label: t("account-menu-sign-out"),
                        danger: true,
                        onclick: move |_| {
                            let aid = aid.clone();
                            client_manager.batch(|cm| { cm.take_account(&aid); });
                            close();
                        },
                    }
                }
            }

            div { class: "context-menu-separator" }

            // Copy Account ID
            {
                let aid = account_id.clone();
                let mut close = close;
                rsx! {
                    AccountMenuItem {
                        label: t("account-menu-copy-id"),
                        onclick: move |_| {
                            let a = aid.clone();
                            let _ = document::eval(&format!("navigator.clipboard.writeText('{a}')"));
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
fn AccountMenuItem(
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
