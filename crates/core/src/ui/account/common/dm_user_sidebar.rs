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

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, UiOverlays};
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
        // Unknown = no presence info (HN, anon backends). Suppress the dot
        // entirely rather than rendering grey "offline".
        PresenceStatus::Unknown => "",
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
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let ui_layout: crate::state::BatchedSignal<crate::state::UiLayout> = use_context();
    let chat_view_state: BatchedSignal<crate::state::ChatViewState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let members = if chat_view_state.read().members.is_empty() { // poly-lint: allow render-time-read — render snapshot; subscription intentional
        chat_view_state.read().active_group_members.clone() // poly-lint: allow render-time-read — render snapshot; subscription intentional
    } else {
        chat_view_state.read().members.clone() // poly-lint: allow render-time-read — render snapshot; subscription intentional
    };
    let group_id = nav.read().selected_channel.cloned();
    let active_account_id = nav.read().active_account_id.cloned();
    let member_count = members.len();

    rsx! {
        aside { class: "user-sidebar dm-user-sidebar",
            div { class: "dm-sidebar-header",
                h4 { class: "user-sidebar-title", "{t(\"group-members-title\")} ({member_count})" }
                button {
                    class: "dm-sidebar-close",
                    title: "Close member list",
                    onclick: move |_| {
                        ui_layout.batch(|l| l.dm_right_sidebar_visible = false);
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
                            client_manager,
                            ui_overlays,
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
#[context_menu(UserRowContextMenu)]
#[component]
fn DmMemberRow(
    member: User,
    group_id: String,
    account_id: String,
    client_manager: BatchedSignal<ClientManager>,
    ui_overlays: BatchedSignal<UiOverlays>,
) -> Element {
    let chat_view_state: BatchedSignal<crate::state::ChatViewState> = use_context();
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
            onclick: move |_| open_user_profile(ui_overlays, member.clone()),
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
                        remove_member(gid, mid, aid, client_manager, chat_view_state).await;
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
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<crate::state::ChatViewState>,
) {
    let result = client_manager.peek().with_backend(&account_id, async |b| {
        b.remove_group_member(&group_id, &user_id).await
    }).await;
    if result.is_ok() {
        // Remove the member from local state immediately for instant feedback.
        chat_view_state.batch(|cv| {
            cv.active_group_members.retain(|m| m.id != user_id);
            cv.members.retain(|m| m.id != user_id);
        });
    }
}
