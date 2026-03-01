//! Friends browser panel — tiled grid view of friends with filtering.
//!
//! Supports filtering by:
//! - Account/backend (Discord, Matrix, etc.)
//! - Server/community they're known from
//! - Search by username
//! - Favorite servers

use crate::i18n::t;
use crate::state::chat_data::backend_badge;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;

/// Friends browser panel with tiled grid display and filtering options.
#[component]
pub fn FriendsPanel() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let friends = chat_data.read().friends.clone();

    // Filter state
    let mut search_filter = use_signal(String::new);
    let mut account_filter = use_signal(|| None::<String>);
    let mut server_filter = use_signal(|| None::<String>);

    let search_lower = search_filter.read().to_lowercase();

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
            // Filter by server if selected
            if server_filter.read().is_some() {
                // TODO: Check if friend is in selected server
            }
            true
        })
        .collect::<Vec<_>>();

    let back_onclick = move |_| {
        app_state.write().nav.view = View::DmsFriends;
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

                // Account filter dropdown
                select {
                    class: "friends-filter-select",
                    value: "{account_filter.read().as_deref().unwrap_or(\"all\")}",
                    onchange: move |evt| {
                        let val = evt.value();
                        account_filter.set(if val == "all" { None } else { Some(val) });
                    },
                    option { value: "all", "{t(\"filter-all\")}" }
                                // TODO: Populate with actual accounts
                }

                // Server filter dropdown
                select {
                    class: "friends-filter-select",
                    value: "{server_filter.read().as_deref().unwrap_or(\"all\")}",
                    onchange: move |evt| {
                        let val = evt.value();
                        server_filter.set(if val == "all" { None } else { Some(val) });
                    },
                    option { value: "all", "{t(\"filter-all-servers\")}" }
                                // TODO: Populate with actual servers
                }
            }

            // Friends grid
            div { class: "friends-grid",
                if filtered_friends.is_empty() {
                    div { class: "empty-state", "{t(\"friends-none\")}" }
                } else {
                    for friend in filtered_friends.iter() {
                        div {
                            class: "friend-card",
                            onclick: {
                                let friend_id = friend.id.clone();
                                move |_| {
                                    app_state.write().push_nav_history();
                                    app_state.write().nav.selected_channel = Some(friend_id.clone());
                                    app_state.write().nav.view = View::DmsFriends;
                                }
                            },
                            // Avatar
                            div {
                                class: "friend-avatar",
                                style: "background-color: {user_color(&friend.id)};",
                                "{friend.display_name.chars().next().unwrap_or('?')}"
                            }
                            // Friend info
                            div { class: "friend-info",
                                div { class: "friend-name", "{friend.display_name}" }
                                div { class: "friend-account", "{backend_badge(&friend.backend)}" }
                                                        // TODO: Show mutual servers
                            }
                        }
                    }
                }
            }
        }
    }
}
