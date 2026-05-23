//! Friend contact item — shown in search results or the friends panel.
//!
//! Clicking a `FriendItem` opens (or creates) a direct message channel with
//! the target user via the currently active account.

use super::dm_view::open_direct_message_from_active_account;
use super::ChannelListAction;
use crate::client_manager::ClientManager;
use crate::state::BatchedSignal;
use crate::state::{AccountSessions, ChatLists};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Friend contact in search results.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(ChannelListAction)]
#[component]
pub(super) fn FriendItem(display_name: String, user_id: String) -> Element {
    use crate::state::chat_data::user_color;

    let nav_state: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let nav = navigator();

    let color = user_color(&user_id);
    let first_char: String = display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();

    rsx! {
        div {
            class: "channel-item",
            onclick: {
                let target_user_id = user_id.clone();
                move |_| {
                    open_direct_message_from_active_account(
                        target_user_id.clone(),
                        nav_state,
                        account_sessions,
                        client_manager,
                        nav,
                        chat_lists,
                    );
                }
            },
            div { class: "dm-avatar-small", style: "background-color: {color};", "{first_char}" }
            span { class: "channel-name", "{display_name}" }
        }
    }
}
