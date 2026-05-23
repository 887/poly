//! Account server bar — per-account navigation (DMs, Notifications, Servers).
//!
//! This is the **second sidebar column** (Bar 2), shown whenever an account
//! is active (`active_account_id` is set in `NavigationState`).
//!
//! Shows:
//! 1. Overview button (account-scoped)
//! 2. Conversations button (account-scoped, if `show_dms`)
//! 3. Friends/Ignore/Blocks management button (account-scoped, if `show_friends`)
//! 4. Notifications button (account-scoped, with unread badge, if `show_notifs`)
//! 5. Discover Communities button (if `show_discover`)
//! 6. Separator
//! 7. All servers for the active account (drag-and-drop reorderable)
//! 8. Spacer
//!
//! ## ISP split (B.8)
//! - [`account_list`] — account-wide nav buttons (overview, notifs, discover, create)
//! - [`dm_bar`] — social-nav buttons (DMs, friends) — different capability gate
//! - [`server_list`] — server icons + DnD reordering helpers
//! - This `mod.rs` — thin shell; orchestrates the three sub-concerns via [`AccountServerBar`]
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

pub mod account_list;
pub mod dm_bar;
pub mod server_list;

pub use account_list::{
    AccountBarDiscoverButton, AccountBarNotifsButton, AccountBarOverviewButton, CreateServerButton,
};
pub use dm_bar::{AccountBarDmsButton, AccountBarFriendsButton};
pub use server_list::AccountServerIcon;

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::{AccountSessions, ChatLists, NavState, View};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};
use server_list::get_ordered_servers;

/// Account server bar — second sidebar column, per-account.
///
/// Only rendered when `active_account_id` is `Some(...)`.
/// Shows DMs, notifications, and all servers for this account.
/// Delegates rendering to the three sub-concern modules:
///   `account_list` (overview/notifs/discover), `dm_bar` (DMs/friends), `server_list` (servers).
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn AccountServerBar() -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let (active_account_id, active_backend, active_instance_id, current_view, selected_server) = {
        let nav = nav_state.read(); // poly-lint: allow render-time-read — intentional: AccountServerBar must re-render on any nav change
        (
            nav.active_account_id.cloned(),
            nav.active_backend.cloned(),
            nav.active_instance_id.cloned(),
            *nav.view,
            nav.selected_server.cloned(),
        )
    };

    // If no account is active, don't render
    let Some(account_id) = active_account_id else {
        return rsx! {};
    };

    let backend_slug = active_backend.map_or_else(|| "demo".to_string(), |b| b.slug().to_string());
    let instance_id = active_instance_id.unwrap_or_else(|| "demo".to_string());

    // Get all servers for this account (not just favorites)
    let all_servers = chat_lists.read() // poly-lint: allow render-time-read — intentional: re-render when server list changes
        .servers.clone();
    let account_servers: Vec<_> = all_servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .cloned()
        .collect();

    // Apply per-account ordering from drag-and-drop reordering.
    let ordered_account_servers = {
        let as_ = account_sessions.read(); // poly-lint: allow render-time-read — intentional: re-render when account_server_order changes after DnD
        get_ordered_servers(&as_, &account_id, &account_servers)
    };

    // Count unread notifications for this account
    let notif_count = chat_lists
        .read() // poly-lint: allow render-time-read — intentional: re-render when notifications arrive to update badge count
        .notifications
        .iter()
        .filter(|n| !n.read && n.account_id == account_id)
        .count();

    // Pack F (P57) — capability-gate the per-account nav buttons.
    let caps = client_manager.peek().capabilities_for_slug(&backend_slug);
    let show_dms = caps.should_show_dms();
    let show_friends = caps.should_show_friends();
    let show_notifs = caps.should_show_notifications();
    let show_discover = caps.should_show_discover();

    // Forum-layout backends (Lemmy) store subscribed communities as servers in
    // `chat_data.servers`, populated at login/restore time. If the list is empty
    // for this account (e.g. first load before restore completes, or after
    // subscribing to a new community), trigger a background refresh so Bar-2
    // icons appear without requiring an app restart.
    //
    // Uses `use_resource` keyed on account_id so it re-fires when the user
    // switches accounts. Runs for ALL backends so chat-style accounts also
    // refresh on first mount (e.g. if restore was slow).
    // lint-allow-unused: use_resource returns a Resource handle that owns the
    // spawned future; we deliberately discard the handle so the resource lives
    // for the component's lifetime via Dioxus' runtime.
    #[allow(clippy::let_underscore_future, clippy::let_underscore_must_use)]
    let _ = {
        let account_id_res = account_id.clone();
        let backend_slug_res = backend_slug.clone();
        use_resource(move || {
            let account_id = account_id_res.clone();
            let backend_slug = backend_slug_res.clone();
            async move {
                let client_manager: BatchedSignal<ClientManager> =
                    match try_consume_context() {
                        Some(cm) => cm,
                        None => return,
                    };
                let servers = match client_manager.peek().with_backend_timeout(
                    &account_id,
                    std::time::Duration::from_secs(10),
                    async |b| b.get_servers().await,
                ).await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                if servers.is_empty() {
                    return;
                }
                // Only add servers not already present (avoid duplicates on
                // repeated renders).
                let chat_lists: BatchedSignal<ChatLists> = match try_consume_context() {
                    Some(cl) => cl,
                    None => return,
                };
                let account_sessions: BatchedSignal<AccountSessions> = match try_consume_context() {
                    Some(as_) => as_,
                    None => return,
                };
                let is_forum = client_manager
                    .peek()
                    .capabilities_for_slug(&backend_slug)
                    .is_forum_layout();
                chat_lists.batch(move |cl| {
                    for srv in servers {
                        if !cl.servers.iter().any(|s| s.id == srv.id) {
                            cl.push_server(srv);
                        }
                    }
                });
                // Forum backends: ensure all servers are in favorited_server_ids
                // so they also appear in Bar-1 (favorites sidebar).
                if is_forum {
                    let srv_ids: Vec<String> = chat_lists.read() // poly-lint: allow render-time-read — inside async block in use_resource, not render body
                        .servers
                        .iter()
                        .filter(|s| s.account_id == account_id)
                        .map(|s| s.id.clone())
                        .collect();
                    account_sessions.batch(move |as_| {
                        for srv in srv_ids {
                            if !as_.favorited_server_ids.contains(&srv) {
                                as_.favorited_server_ids.push(srv);
                            }
                        }
                    });
                }
            }
        })
    };

    rsx! {
        nav { class: "account-server-bar",
            // Per-account Overview — first item in Bar 2 for every backend.
            AccountBarOverviewButton {
                current_view,
                backend_slug: backend_slug.clone(),
                instance_id: instance_id.clone(),
                account_id: account_id.clone(),
            }

            // DMs button — social-nav, capability-gated
            if show_dms {
                AccountBarDmsButton {
                    current_view,
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                }
            }

            // Friends button — social-nav, capability-gated
            if show_friends {
                AccountBarFriendsButton {
                    current_view,
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                }
            }

            // Notifications button — account-wide, capability-gated
            if show_notifs {
                AccountBarNotifsButton { current_view, notif_count }
            }

            // Discover Communities — only for backends with community_search support
            if show_discover {
                AccountBarDiscoverButton {
                    current_view,
                    backend_slug: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                }
            }

            // Separator
            div { class: "sidebar-separator" }

            // All servers for this account (ordered by drag-and-drop if reordered).
            for server in ordered_account_servers {
                AccountServerIcon {
                    key: "{server.id}",
                    server_id: server.id.clone(),
                    server_name: server.name.clone(),
                    backend_slug: server.backend.slug().to_string(),
                    instance_id: instance_id.clone(),
                    account_id: server.account_id.clone(),
                    unread: server.unread_count,
                    mention: server.mention_count,
                    is_selected: selected_server.as_deref() == Some(server.id.as_str()),
                    icon_url: server.icon_url.clone(),
                }
            }

            // Separator + "+" button to join/create a new server/guild.
            div { class: "sidebar-separator" }
            CreateServerButton { account_id: account_id.clone() }

            // Spacer keeps the icon rail aligned above the shared bottom account bar.
            div { class: "sidebar-spacer" }
        }
    }
}
