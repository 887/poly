//! DM-bar sub-concern for the account server bar.
//!
//! Owns: [`AccountBarDmsButton`], [`AccountBarFriendsButton`].
//! These are social-navigation buttons scoped to the DM/friends domain.
//! Capability-gated by the backend: HN/Lemmy/GitHub omit them entirely.

use crate::state::BatchedSignal;
use super::super::super::super::routes::Route;
use crate::state::{AppState, ChatAction, ChatLists, ChatViewState, View};
use crate::i18n::t;
use crate::ui::favorites_sidebar::SidebarTooltip;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// DMs / conversations navigation button — account-scoped.
///
/// Navigates to the DMs home route for the active account.
/// Only rendered when `caps.should_show_dms()` is true.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountBarDmsButton(
    current_view: View,
    backend_slug: String,
    /// Instance ID for federated routing (e.g. `"demo"`, `"matrix.org"`).
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    rsx! {
        div {
            class: if current_view == View::DmsFriends { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::DmsFriends {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                navigator()
                    .push(Route::DmsHome {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                    });
            },
            div { class: "icon-dms", "💬" }
            SidebarTooltip {
                line1: t("nav-dms"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// Friends / ignore / blocked management button — account-scoped.
///
/// Navigates to the Friends route for the active account.
/// Only rendered when `caps.should_show_friends()` is true.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountBarFriendsButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let _app_state: BatchedSignal<AppState> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    rsx! {
        div {
            class: if current_view == View::Friends { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Friends {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                crate::nav!(Route::FriendsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            div { class: "icon-dms", "👥" }
            SidebarTooltip {
                line1: t("nav-friends"),
                line2: None,
                line3: None,
            }
        }
    }
}
