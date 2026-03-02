//! Friends browser panel — tiled grid view of friends with filtering.
//!
//! Supports filtering by:
//! - Account/backend (Discord, Matrix, etc.)
//! - Server/community they're known from
//! - Search by username
//! - Favorite servers
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use super::routes::Route;
use crate::i18n::t;
use crate::state::chat_data::backend_badge;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;

/// Friends browser panel with tiled grid display and filtering options.
#[component]
pub fn FriendsPanel() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let friends = chat_data.read().friends.clone();
    let servers = chat_data.read().servers.clone();

    // Filter state
    let mut search_filter = use_signal(String::new);
    let mut account_filter = use_signal(|| None::<String>);
    let mut server_filter = use_signal(|| None::<String>);

    let search_lower = search_filter.read().to_lowercase();

    // Collect distinct backends from friends for the account filter
    let mut backend_names: Vec<String> =
        friends.iter().map(|f| format!("{:?}", f.backend)).collect();
    backend_names.sort();
    backend_names.dedup();

    // Filter friends based on current filters
    let filtered_friends = friends
        .iter()
        .filter(|friend| {
            // Search by name
            if !search_lower.is_empty()
                && !friend.display_name.to_lowercase().contains(&search_lower)
            {
                return false;
            }
            // Filter by account (backend) if selected
            if account_filter
                .read()
                .as_ref()
                .is_some_and(|account| format!("{:?}", friend.backend) != account.as_str())
            {
                return false;
            }
            // Server filter is informational — requires mutual server data
            // which is a phase-3 feature
            true
        })
        .collect::<Vec<_>>();

    let back_onclick = move |_| {
        navigator().go_back();
    };

    rsx! {
        div { class: "friends-panel",
            // Header with back button and title
            div { class: "friends-header",
                button { class: "friends-back-btn", onclick: back_onclick, "← {t(\"nav-back\")}" }
                h2 { "{t(\"friends-title\")}" }
            }

            // Filter bar
            div { class: "friends-filters",
                // Search box
                input {
                    class: "friends-search",
                    placeholder: "{t(\"friends-search-placeholder\")}",
                    value: "{search_filter.read()}",
                    oninput: move |evt| {
                        search_filter.set(evt.value().clone());
                    },
                }

                // Account filter dropdown — populated from real backend data
                select {
                    class: "friends-filter-select",
                    value: "{account_filter.read().as_deref().unwrap_or(\"all\")}",
                    onchange: move |evt| {
                        let val = evt.value();
                        account_filter.set(if val == "all" { None } else { Some(val) });
                    },
                    option { value: "all", "{t(\"filter-all\")}" }
                    for name in &backend_names {
                        option { value: "{name}", "{name}" }
                    }
                }

                // Server filter dropdown — populated from real server data
                select {
                    class: "friends-filter-select",
                    value: "{server_filter.read().as_deref().unwrap_or(\"all\")}",
                    onchange: move |evt| {
                        let val = evt.value();
                        server_filter.set(if val == "all" { None } else { Some(val) });
                    },
                    option { value: "all", "{t(\"filter-all-servers\")}" }
                    for srv in &servers {
                        {
                            let badge = backend_badge(&srv.backend);
                            let label = format!("{badge} {}", srv.name);
                            let sid = srv.id.clone();
                            rsx! {
                                option { value: "{sid}", "{label}" }
                            }
                        }
                    }
                }
            }

            // Friends grid
            FriendsGrid { friends: filtered_friends.into_iter().cloned().collect() }
        }
    }
}

/// Grid of friend cards.
#[component]
fn FriendsGrid(friends: Vec<poly_client::User>) -> Element {
    let mut app_state: Signal<AppState> = use_context();

    rsx! {
        div { class: "friends-grid",
            if friends.is_empty() {
                div { class: "empty-state", "{t(\"friends-none\")}" }
            } else {
                for friend in &friends {
                    {
                        let friend_id = friend.id.clone();
                        let display_name = friend.display_name.clone();
                        let backend = friend.backend;
                        let color = user_color(&friend.id);
                        let first_char: String = display_name
                            .chars()
                            .next()
                            .map(|c| c.to_string())
                            .unwrap_or_default();
                        rsx! {
                            div {
                                class: "friend-card",
                                onclick: {
                                    let fid = friend_id.clone();
                                    move |_| {
                                        app_state.write().nav.selected_channel = Some(fid.clone());
                                        // Use the friend's backend slug for the route; read
                                        // the active account_id from nav state (the account
                                        // that owns this friend relationship).
                                        let account_id = app_state
                                            .read()
                                            .nav
                                            .active_account_id
                                            .clone()
                                            .unwrap_or_else(|| backend.slug().to_string());
                                        navigator()
                                            .push(Route::DmChat {
                                                backend: backend.slug().to_string(),
                                                account_id,
                                                channel_id: fid.clone(),
                                            });
                                    }
                                },
                                div { class: "friend-avatar", style: "background-color: {color};", "{first_char}" }
                                div { class: "friend-info",
                                    div { class: "friend-name", "{display_name}" }
                                    div { class: "friend-account", "{backend_badge(&backend)} {backend.display_name()}" }
                                    // TODO(phase-3): mutual servers list (2.6.6.3)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
