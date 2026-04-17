//! DM user sidebar — member list for group DMs with remove action.
//!
//! Shown on the right side of the chat area when a group DM is open and
//! the "Members" toggle button in the chat header is active.
//!
//! Features:
//! - Lists all members of the current group DM with avatar and presence dot.
//! - "Remove" button appears on hover for each member (excluding the local user).
//! - Remove calls `remove_group_member` on the active backend and removes the
//!   member from `ChatData::active_group_members` locally for immediate feedback.
//! - "×" close button toggles `NavState::dm_right_sidebar_visible` off.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData};
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User};
use poly_ui_macros::{context_menu, ui_action};

/// Presence dot CSS class for a given status.
fn presence_dot_class(status: &PresenceStatus) -> &'static str {
    match status {
        PresenceStatus::Online => "presence-dot online",
        PresenceStatus::Idle => "presence-dot idle",
        PresenceStatus::DoNotDisturb => "presence-dot dnd",
        PresenceStatus::Offline | PresenceStatus::Invisible => "presence-dot offline",
    }
}

/// DM user sidebar — group member list with remove action.
///
/// Reads `ChatData::active_group_members` and `NavState::dm_right_sidebar_visible`.
/// Renders nothing when there are no active group members.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn DmUserSidebar() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let members = if chat_data.read().members.is_empty() {
        chat_data.read().active_group_members.clone()
    } else {
        chat_data.read().members.clone()
    };
    let group_id = app_state.read().nav.selected_channel.clone();
    let active_account_id = app_state.read().nav.active_account_id.clone();
    let member_count = members.len();

    rsx! {
        aside { class: "user-sidebar dm-user-sidebar",
            div { class: "dm-sidebar-header",
                h4 { class: "user-sidebar-title", "{t(\"group-members-title\")} ({member_count})" }
                button {
                    class: "dm-sidebar-close",
                    title: "Close member list",
                    onclick: move |_| {
                        app_state.write().nav.dm_right_sidebar_visible = false;
                    },
                    "×"
                }
            }

            if members.is_empty() {
                div { class: "user-sidebar-empty", "{t(\"user-no-members\")}" }
            } else {
                div { class: "dm-member-list",
                    for member in &members {
                        DmMemberRow {
                            member: member.clone(),
                            group_id: group_id.clone().unwrap_or_default(),
                            account_id: active_account_id.clone().unwrap_or_default(),
                            chat_data,
                            client_manager,
                            app_state,
                        }
                    }
                }
            }
        }
    }
}

/// A single member row in the DM group member sidebar.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn DmMemberRow(
    member: User,
    group_id: String,
    account_id: String,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
    app_state: Signal<AppState>,
) -> Element {
    let color = user_color(&member.id);
    let first_char: String = member
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let dot_class = presence_dot_class(&member.presence);
    let member_id = member.id.clone();
    let member_name = member.display_name.clone();
    let avatar_url = member.avatar_url.clone();
    let remove_tooltip = format!("Remove {} from this group", member.display_name);

    rsx! {
        div {
            class: "dm-member-row",
            onclick: move |_| open_user_profile(app_state, member.clone()),
            // Avatar with presence dot
            div { class: "dm-member-avatar-wrap",
                div {
                    class: "dm-avatar-small",
                    style: "background-color: {color};",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "dm-avatar-img",
                            src: "{url}",
                            alt: "{member_name}",
                        }
                    } else {
                        "{first_char}"
                    }
                }
                span { class: "{dot_class}" }
            }
            span { class: "dm-member-name", "{member_name}" }
            // Remove button
            button {
                class: "dm-member-remove-btn",
                title: "{remove_tooltip}",
                onclick: move |_| {
                    let mid = member_id.clone();
                    let gid = group_id.clone();
                    let aid = account_id.clone();
                    spawn(async move {
                        remove_member(gid, mid, aid, client_manager, chat_data).await;
                    });
                },
                "{t(\"group-member-remove\")}"
            }
        }
    }
}

/// Remove a member from the group in the backend and update local state.
async fn remove_member(
    group_id: String,
    user_id: String,
    account_id: String,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
) {
    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        return;
    };
    let guard = backend.read().await;
    if guard.remove_group_member(&group_id, &user_id).await.is_ok() {
        // Remove the member from local state immediately for instant feedback.
        chat_data
            .write()
            .active_group_members
            .retain(|m| m.id != user_id);
        chat_data.write().members.retain(|m| m.id != user_id);
    }
}
