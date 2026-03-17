//! New conversation composer for the active account.
//!
//! This is intentionally separate from the friends-management page: the user
//! lands here to start a new DM (and later a group DM) with friends from the
//! active account context.

use super::channel_list::open_direct_message_from_active_account;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;

#[rustfmt::skip]
#[component]
pub fn NewConversationView() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();

    let mut search_filter = use_signal(String::new);
    let mut selected_user_ids = use_signal(Vec::<String>::new);
    let search_lower = search_filter.read().to_lowercase();
    let active_backend = app_state.read().nav.active_backend;

    let friends: Vec<_> = chat_data
        .read()
        .friends
        .iter()
        .filter(|friend| active_backend.is_none_or(|backend| backend == friend.backend))
        .filter(|friend| {
            search_lower.is_empty() || friend.display_name.to_lowercase().contains(&search_lower)
        })
        .cloned()
        .collect();

    let selected_count = selected_user_ids.read().len();
    let can_start_dm = selected_count == 1;
    let can_start_group = selected_count > 1;
    let title = t("dm-new-conversation");
    let description = t("new-conversation-description");
    let search_placeholder = t("friends-search-placeholder");
    let empty_text = t("friends-none");
    let start_dm_label = t("new-conversation-start-dm");
    let multi_select_note = if can_start_group {
        Some(t("new-conversation-group-pending"))
    } else {
        None
    };

    rsx! {
        main { class: "chat-view",
            div { class: "chat-header",
                span { class: "chat-channel-name", "{title}" }
            }
            div { class: "friends-panel",
                div { class: "friends-header",
                    h2 { "{title}" }
                    p { class: "settings-description", "{description}" }
                }

                div { class: "friends-filters",
                    input {
                        class: "friends-search",
                        placeholder: "{search_placeholder}",
                        value: "{search_filter.read()}",
                        oninput: move |evt| search_filter.set(evt.value()),
                    }
                }

                div { class: "notification-list",
                    if friends.is_empty() {
                        div { class: "notifications-empty",
                            p { "{empty_text}" }
                        }
                    } else {
                        for friend in &friends {
                            {
                                let friend_id = friend.id.clone();
                                let selected = selected_user_ids.read().contains(&friend_id);
                                let display_name = friend.display_name.clone();
                                let backend_name = friend.backend.display_name().to_string();
                                let avatar_url = friend.avatar_url.clone();
                                rsx! {
                                    label {
                                        class: if selected { "search-node-row active" } else { "search-node-row" },
                                        input {
                                            r#type: "checkbox",
                                            checked: selected,
                                            onchange: move |_| {
                                                let mut selected_ids = selected_user_ids.write();
                                                if selected_ids.contains(&friend_id) {
                                                    selected_ids.retain(|id| id != &friend_id);
                                                } else {
                                                    selected_ids.push(friend_id.clone());
                                                }
                                            },
                                        }
                                        div { class: "search-node-info",
                                            span { class: "search-node-label", "{display_name}" }
                                            span { class: "search-node-sublabel", "{backend_name}" }
                                        }
                                        if let Some(url) = avatar_url {
                                            img { class: "search-avatar-icon", src: "{url}", alt: "{display_name}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "message-input-area",
                    button {
                        class: if can_start_dm { "btn btn-primary" } else { "btn btn-primary disabled" },
                        disabled: !can_start_dm,
                        onclick: move |_| {
                            let Some(friend_id) = selected_user_ids.read().first().cloned() else {
                                return;
                            };
                            open_direct_message_from_active_account(
                                friend_id,
                                app_state,
                                chat_data,
                                client_manager,
                                nav,
                            );
                        },
                        "{start_dm_label}" 
                    }
                    if let Some(note) = multi_select_note {
                        span { class: "search-node-sublabel", "{note}" }
                    }
                }
            }
        }
    }
}
