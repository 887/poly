//! DM + Groups unified view — list of DM channels, group chats, and action
//! shortcuts (New Conversation, Saved Messages).
//!
//! Also contains the async helper `load_dm_messages` (hang-class #4 safe).
//!
//! The timing helpers `dm_last_incoming_timestamp` and
//! `group_last_incoming_timestamp` are pure functions (no signals); they live
//! here because they exist solely to sort the DM / group lists in this module.

use super::items::{DMChannelItem, GroupChannelItem};
use super::ChannelListAction;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::state::{AccountSessions, ChatLists, ChatViewState, NavState};
use crate::ui::account::common::chat_history::{
    initial_message_query, remember_message_list_scroll_position,
    request_restore_scroll_position_or_bottom,
};
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::routes::Route;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use poly_client::DmChannel;
use poly_ui_macros::{context_menu, ui_action};

// ── Pure sorting helpers ──────────────────────────────────────────────────────

pub(super) fn dm_last_incoming_timestamp(dm: &DmChannel) -> Option<DateTime<Utc>> {
    dm.last_message
        .as_ref()
        .filter(|message| message.author.id == dm.user.id)
        .map(|message| message.timestamp)
}

pub(super) fn group_last_incoming_timestamp(
    group: &poly_client::Group,
    active_user_id: Option<&str>,
) -> Option<DateTime<Utc>> {
    group
        .last_message
        .as_ref()
        .filter(|message| active_user_id.is_none_or(|user_id| message.author.id != user_id))
        .map(|message| message.timestamp)
}

// ── Async loader (hang-class #4 safe) ────────────────────────────────────────

/// Load messages for a DM or group channel using the account backend directly
/// (does not require a selected server).
///
/// Uses `ClientManager::with_backend` which internally wraps the backend lock
/// acquisition with a timeout — safe on `wasm32-unknown-unknown`.
pub(super) async fn load_dm_messages(
    channel_id: String,
    account_id: String,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
) {
    tracing::info!(
        channel_id = %channel_id,
        account_id = %account_id,
        "load_dm_messages: start"
    );
    // One cascade for the initial reset so subscribers paint the empty
    // loading state before we start awaiting.
    chat_view_state.batch(|cv| {
        cv.loading = true;
        cv.set_messages(Vec::new());
        cv.members = Vec::new();
    });

    let unread_count = chat_view_state
        .peek()
        .current_channel
        .as_ref()
        .filter(|channel| channel.id == channel_id)
        .map_or(0, |channel| channel.unread_count);

    tracing::info!(channel_id = %channel_id, "load_dm_messages: fetching messages");
    let Ok((messages, members)) = client_manager.peek().with_backend(&account_id, async |b| {
        let messages = b.get_messages(&channel_id, initial_message_query(unread_count)).await.ok();
        let members = b.get_channel_members(&channel_id).await.ok();
        Ok((messages, members))
    }).await else {
        tracing::warn!(account_id = %account_id, "load_dm_messages: no backend or timed out");
        chat_view_state.batch(|cv| cv.loading = false);
        return;
    };

    tracing::info!(
        channel_id = %channel_id,
        messages = messages.as_ref().map(|m: &Vec<_>| m.len()),
        members = members.as_ref().map(|m: &Vec<_>| m.len()),
        "load_dm_messages: done, writing results"
    );

    // ONE terminal cascade for the whole async fetch.
    let mut pending = chat_view_state.pending_update();
    if let Some(msgs) = messages {
        pending.set(move |cv| cv.set_messages(msgs));
        request_restore_scroll_position_or_bottom(&channel_id);
    }
    if let Some(mbrs) = members {
        pending.set(move |cv| cv.members = mbrs);
    }
    pending.set(|cv| cv.loading = false);
    pending.apply();
}

// ── Nav helpers ───────────────────────────────────────────────────────────────

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn activate_dm_channel(
    dm: DmChannel,
    instance_id: String,
    nav_state: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
    nav: crate::ui::dioxus_router::Navigator,
) {
    tracing::info!(
        dm_id = %dm.id,
        account_id = %dm.account_id,
        "activate_dm_channel: start"
    );

    // Snapshot the previous channel before taking any write lock.
    let previous_channel_id = nav_state.read().selected_channel.cloned();
    if let Some(ref prev_id) = previous_channel_id {
        remember_message_list_scroll_position(prev_id);
    }

    // Pre-mutating app_state.nav and chat_data here was triggering a render
    // storm when combined with on_update's write of the SAME nav fields after
    // nav.push. Each pre-mutation re-fired ChatView's many use_effect
    // subscribers (use_history_state_effect, use_member_list_effect, …) on the
    // single-threaded WASM scheduler, hanging the page. Just navigate — F5 on
    // the same URL works because it skips the pre-mutation, and DmChat's own
    // use_effect (restore_dm_chat) loads the channel + messages from the route
    // params. Friend-click now walks the same path.
    let _ = (dm.unread_count, &dm.last_message);

    nav.push(Route::DmChat {
        backend: dm.backend.slug().to_string(),
        instance_id,
        account_id: dm.account_id.clone(),
        dm_id: dm.id.clone(),
    });
    close_mobile_drawer();
    let _ = client_manager;
}

fn active_account_context(
    nav_state: BatchedSignal<NavState>,
    account_sessions: BatchedSignal<AccountSessions>,
) -> Option<(String, String)> {
    let account_id = nav_state.read().active_account_id.cloned()?;
    let instance_id = account_sessions
        .read()
        .account_sessions
        .get(&account_id)
        .map(|session| session.instance_id.clone())
        .or_else(|| nav_state.read().active_instance_id.cloned())
        .unwrap_or_default();
    Some((account_id, instance_id))
}

/// Open or create a direct message for the current active account, then
/// navigate using the real DM channel ID returned by the backend.
pub fn open_direct_message_from_active_account(
    user_id: String,
    nav_state: BatchedSignal<NavState>,
    account_sessions: BatchedSignal<AccountSessions>,
    client_manager: BatchedSignal<ClientManager>,
    nav: crate::ui::dioxus_router::Navigator,
    chat_lists: BatchedSignal<ChatLists>,
) {
    tracing::info!(user_id = %user_id, "open_direct_message_from_active_account: start");

    let Some((account_id, instance_id)) = active_account_context(nav_state, account_sessions) else {
        tracing::warn!("open_direct_message_from_active_account: no active account");
        return;
    };

    tracing::info!(
        user_id = %user_id,
        account_id = %account_id,
        "open_direct_message_from_active_account: active account resolved"
    );

    // Read `dm_channels` under a scoped borrow that is dropped before any write.
    let existing_dm = {
        let cl = chat_lists.peek();
        cl.dm_channels
            .iter()
            .find(|dm| dm.account_id == account_id && dm.user.id == user_id)
            .cloned()
    };

    if let Some(existing_dm) = existing_dm {
        tracing::info!(
            dm_id = %existing_dm.id,
            "open_direct_message_from_active_account: existing DM found, activating"
        );
        activate_dm_channel(
            existing_dm,
            instance_id,
            nav_state,
            client_manager,
            nav,
        );
        return;
    }

    tracing::info!(
        user_id = %user_id,
        account_id = %account_id,
        "open_direct_message_from_active_account: no existing DM, requesting backend"
    );

    spawn(async move {
        tracing::info!(
            user_id = %user_id,
            account_id = %account_id,
            "open_direct_message_from_active_account: spawned, awaiting open_direct_message_channel"
        );
        let opened_dm = {
            match client_manager.peek().with_backend(&account_id, async |b| {
                match b.as_dms_and_groups() {
                    Some(dg) => dg.open_direct_message_channel(&user_id).await,
                    None => Err(poly_client::ClientError::NotSupported(
                        "open_direct_message_channel: backend has no DMs capability".to_string(),
                    )),
                }
            }).await {
                Ok(dm) => dm,
                Err(err) => {
                    tracing::warn!(
                        "open_direct_message_from_active_account: failed to open DM for user {} on account {}: {}",
                        user_id,
                        account_id,
                        err
                    );
                    return;
                }
            }
        };

        tracing::info!(
            dm_id = %opened_dm.id,
            "open_direct_message_from_active_account: DM channel opened, updating dm_channels list"
        );

        // Single write guard: dedup + push under one lock so one re-render fires.
        {
            let opened_clone = opened_dm.clone();
            chat_lists.batch(|cl| {
                cl.dm_channels.retain(|dm| {
                    !(dm.account_id == account_id
                        && (dm.id == opened_clone.id || dm.user.id == user_id))
                });
                cl.dm_channels.push(opened_clone);
            });
        }

        tracing::info!(
            dm_id = %opened_dm.id,
            "open_direct_message_from_active_account: activating new DM channel"
        );
        activate_dm_channel(
            opened_dm,
            instance_id,
            nav_state,
            client_manager,
            nav,
        );
    });
}

// ── Component ─────────────────────────────────────────────────────────────────

/// DMs and Friends view — action shortcuts plus unified list of DMs + groups.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(ChannelListAction)]
#[component]
pub(super) fn DMFriendsView() -> Element {
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();

    // Only show DMs and groups belonging to the currently active account.
    let active_account_id = nav.read().active_account_id.cloned();
    let active_user_id = active_account_id.as_ref().and_then(|account_id| {
        account_sessions
            .read()
            .account_sessions
            .get(account_id)
            .map(|session| session.user.id.clone())
    });
    let new_conversation_label = t("dm-new-conversation");
    let saved_messages_label = t("dm-saved-messages");
    let dm_channels: Vec<_> = chat_lists
        .read()
        .dm_channels
        .iter()
        .filter(|dm| {
            active_account_id.as_deref() == Some(&dm.account_id)
                && active_user_id.as_deref() != Some(dm.user.id.as_str())
        })
        .cloned()
        .collect();
    let groups: Vec<_> = chat_lists
        .read()
        .groups
        .iter()
        .filter(|g| active_account_id.as_deref() == Some(&g.account_id))
        .cloned()
        .collect();
    let selected_channel = nav.read().selected_channel.clone();

    // Sort DMs by the latest incoming message from the other participant.
    let mut sorted_dms = dm_channels.clone();
    sorted_dms.sort_by(|a, b| {
        dm_last_incoming_timestamp(b)
            .cmp(&dm_last_incoming_timestamp(a))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.user.display_name.cmp(&b.user.display_name))
    });

    // Sort groups by the latest incoming message from another member.
    let mut sorted_groups = groups.clone();
    sorted_groups.sort_by(|a, b| {
        group_last_incoming_timestamp(b, active_user_id.as_deref())
            .cmp(&group_last_incoming_timestamp(a, active_user_id.as_deref()))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.name.cmp(&b.name))
    });

    // Pre-compute instance_ids for DMs and groups (cannot use let inside RSX for-loops)
    let dm_instance_ids: Vec<String> = sorted_dms
        .iter()
        .map(|dm| {
            account_sessions
                .read()
                .account_sessions
                .get(&dm.account_id)
                .map(|s| s.instance_id.clone())
                .unwrap_or_default()
        })
        .collect();
    let group_instance_ids: Vec<String> = sorted_groups
        .iter()
        .map(|g| {
            account_sessions
                .read()
                .account_sessions
                .get(&g.account_id)
                .map(|s| s.instance_id.clone())
                .unwrap_or_default()
        })
        .collect();

    rsx! {
        // New conversation button
        button {
            class: "dm-friends-row-btn",
            onclick: move |_| {
                let (backend_slug, instance_id, account_id) = {
                    let nav_snap = nav.read();
                    match (
                        nav_snap.active_backend.cloned(),
                        nav_snap.active_instance_id.cloned(),
                        nav_snap.active_account_id.cloned(),
                    ) {
                        (Some(b), Some(iid), Some(id)) => (b.slug().to_string(), iid, id),
                        _ => ("demo".to_string(), "demo".to_string(), "demo-cat".to_string()),
                    }
                };
                navigator()
                    .push(Route::NewConversationRoute {
                        backend: backend_slug,
                        instance_id,
                        account_id,
                    });
                close_mobile_drawer();
            },
            span { class: "dm-friends-row-icon", "✚" }
            span { class: "dm-friends-row-label", "{new_conversation_label}" }
        }

        button {
            class: "dm-friends-row-btn",
            onclick: move |_| {
                let (backend_slug, instance_id, account_id) = {
                    let nav_snap = nav.read();
                    match (
                        nav_snap.active_backend.cloned(),
                        nav_snap.active_instance_id.cloned(),
                        nav_snap.active_account_id.cloned(),
                    ) {
                        (Some(b), Some(iid), Some(id)) => (b.slug().to_string(), iid, id),
                        _ => ("demo".to_string(), "demo".to_string(), "demo-cat".to_string()),
                    }
                };
                crate::nav!(Route::SavedItemsRoute {
                    backend: backend_slug,
                    instance_id,
                    account_id,
                });
                close_mobile_drawer();
            },
            span { class: "dm-friends-row-icon", "🔖" }
            span { class: "dm-friends-row-label", "{saved_messages_label}" }
        }

        // Unified DM + Group list
        div { class: "dm-unified-list",
            for (dm , dm_iid) in sorted_dms.iter().zip(dm_instance_ids.iter()) {
                DMChannelItem {
                    channel_id: dm.id.clone(),
                    display_name: dm.user.display_name.clone(),
                    user_id: dm.user.id.clone(),
                    unread: dm.unread_count,
                    is_active: selected_channel.as_deref() == Some(&dm.id),
                    account_id: dm.account_id.clone(),
                    backend_slug: dm.backend.slug().to_string(),
                    instance_id: dm_iid.clone(),
                    avatar_url: dm.user.avatar_url.clone(),
                    presence: dm.user.presence,
                }
            }

            for (group , group_iid) in sorted_groups.iter().zip(group_instance_ids.iter()) {
                GroupChannelItem {
                    group_id: group.id.clone(),
                    group_name: group.name.clone(),
                    members: group.members.clone(),
                    is_active: selected_channel.as_deref() == Some(&group.id),
                    account_id: group.account_id.clone(),
                    backend_slug: group.backend.slug().to_string(),
                    instance_id: group_iid.clone(),
                }
            }
        }
    }
}
