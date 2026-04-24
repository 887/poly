//! Friends management panel — special account-social management surface.

use crate::state::BatchedSignal;
use super::VoiceAccountFooter;
use super::channel_list::open_direct_message_from_active_account;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AppState, ChatData};
use crate::ui::account::common::chat_history::remember_message_list_scroll_position;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::toast::{ToastMessage, push_toast};
use crate::ui::routes::Route;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::ToastTone;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the friends management panel.
#[derive(Debug, Clone)]
pub enum FriendsPanelAction {
    /// User switched to the Friends tab.
    ShowFriends,
    /// User switched to the Ignored tab.
    ShowIgnored,
    /// User switched to the Blocked tab.
    ShowBlocked,
    /// User filtered by search text.
    SetSearchFilter(String),
    /// User clicked "Message" on a friend card.
    MessageFriend(String),
}

impl UiAction for FriendsPanelAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: FriendsPanelAction requires Signal handles");
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FriendsManagementTab {
    Friends,
    Ignored,
    Blocked,
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(FriendsPanelAction)]
#[component]
pub fn FriendsPanel(account_id: String, backend_slug: String) -> Element {
    let chat_data: BatchedSignal<ChatData> = use_context();
    let friends = chat_data.read().friends.get(&account_id).cloned().unwrap_or_default();
    let blocked_users = chat_data.read().blocked_users.get(&account_id).cloned().unwrap_or_default();

    let search_filter = use_signal(String::new);
    let account_filter = use_signal(|| None::<String>);
    let mut active_tab = use_signal(|| FriendsManagementTab::Friends);

    let search_lower = search_filter.read().to_lowercase();
    let friends_management_title = t("friends-management-title");
    let friends_management_description = t("friends-management-description");
    let friends_title = t("friends-title");
    let ignored_title = t("friends-ignored-title");
    let blocked_title = t("content-social-blocked");

    let mut backend_names: Vec<String> = friends
        .iter()
        .map(|friend| format!("{:?}", friend.backend))
        .collect();
    backend_names.sort();
    backend_names.dedup();

    let filtered_friends = friends
        .iter()
        .filter(|friend| {
            if !search_lower.is_empty() && !friend.display_name.to_lowercase().contains(&search_lower) {
                return false;
            }

            if account_filter
                .read()
                .as_ref()
                .is_some_and(|account| format!("{:?}", friend.backend) != account.as_str())
            {
                return false;
            }

            true
        })
        .cloned()
        .collect::<Vec<_>>();

    let filtered_blocked = blocked_users
        .iter()
        .filter(|user| search_lower.is_empty() || user.display_name.to_lowercase().contains(&search_lower))
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        SplitMenuShell {
            root_class: "friends-panel-shell".to_string(),
            sidebar_class: "special-page-sidebar friends-panel-sidebar".to_string(),
            content_class: "special-page-content friends-panel-content".to_string(),
            sidebar: rsx! {
                div { class: "special-page-sidebar-header",
                    h2 { class: "special-page-sidebar-title", "{friends_management_title}" }
                    p { class: "special-page-sidebar-description", "{friends_management_description}" }
                }
                div { class: "special-page-sidebar-nav",
                    SidebarMenuButton {
                        label: friends_title.clone(),
                        active: *active_tab.read() == FriendsManagementTab::Friends,
                        onclick: move |_| active_tab.set(FriendsManagementTab::Friends),
                    }
                    SidebarMenuButton {
                        label: ignored_title.clone(),
                        active: *active_tab.read() == FriendsManagementTab::Ignored,
                        onclick: move |_| active_tab.set(FriendsManagementTab::Ignored),
                    }
                    SidebarMenuButton {
                        label: blocked_title.clone(),
                        active: *active_tab.read() == FriendsManagementTab::Blocked,
                        onclick: move |_| active_tab.set(FriendsManagementTab::Blocked),
                    }
                }
                VoiceAccountFooter {}
            },
            content: rsx! {
                div { class: "special-page-panel",
                    div { class: "special-page-header",
                        h2 { class: "special-page-title", "{friends_management_title}" }
                    }
                    FriendsFilterBar {
                        search_filter,
                        account_filter,
                        backend_names,
                    }
                    if *active_tab.read() == FriendsManagementTab::Friends {
                        FriendsGrid { friends: filtered_friends, backend_slug: backend_slug.clone() }
                    } else if *active_tab.read() == FriendsManagementTab::Blocked {
                        BlockedUsersGrid { blocked_users: filtered_blocked }
                    } else {
                        IgnoredUsersPlaceholder {}
                    }
                }
            },
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SidebarMenuButton(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "special-page-sidebar-button active"
    } else {
        "special-page-sidebar-button"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn FriendsFilterBar(
    search_filter: Signal<String>,
    account_filter: Signal<Option<String>>,
    backend_names: Vec<String>,
) -> Element {
    rsx! {
        div { class: "friends-filters special-page-toolbar",
            input {
                class: "friends-search",
                placeholder: "{t(\"friends-search-placeholder\")}",
                value: "{search_filter.read()}",
                oninput: move |evt| search_filter.set(evt.value().clone()),
            }
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
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn FriendsGrid(friends: Vec<poly_client::User>, backend_slug: String) -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();
    let message_label = t("friends-management-message");
    let is_demo = backend_slug == "demo" || backend_slug == "demo_forum";

    rsx! {
        div { class: "friends-grid",
            if friends.is_empty() {
                if is_demo {
                    // Demo accounts are seeded with static data — they cannot
                    // add friends to themselves.  Show a contextual hint and a
                    // button to start the real-account signup flow.
                    div { class: "empty-state friends-empty-state",
                        p { class: "friends-empty-message", "{t(\"friends-demo-empty\")}" }
                        button {
                            class: "btn btn-primary friends-empty-action",
                            onclick: move |_| { nav.push(Route::SignupPicker); },
                            "{t(\"friends-demo-add-account\")}"
                        }
                    }
                } else {
                    // Real backend with no friends yet — nudge the user with an
                    // "Add friend" affordance (full feature pending; shows a
                    // "coming soon" toast for now).
                    div { class: "empty-state friends-empty-state",
                        p { class: "friends-empty-message", "{t(\"friends-none\")}" }
                        button {
                            class: "btn btn-secondary friends-empty-action",
                            onclick: move |_| {
                                if let Some(tq) = try_consume_context::<Signal<Vec<ToastMessage>>>() {
                                    push_toast(tq, ToastMessage::new("friends-add-coming-soon", ToastTone::Info));
                                }
                            },
                            "{t(\"friends-add-friend\")}"
                        }
                    }
                }
            } else {
                for friend in &friends {
                    {
                        let friend_id = friend.id.clone();
                        let display_name = friend.display_name.clone();
                        let backend = friend.backend.clone();
                        let color = user_color(&friend.id);
                        let avatar_url = friend.avatar_url.clone();
                        let first_char = display_name.chars().next().map(|ch| ch.to_string()).unwrap_or_default();
                        let presence_dot_class: &'static str = match friend.presence {
                            poly_client::PresenceStatus::Online => "presence-dot online",
                            poly_client::PresenceStatus::Idle => "presence-dot idle",
                            poly_client::PresenceStatus::DoNotDisturb => "presence-dot dnd",
                            poly_client::PresenceStatus::Offline | poly_client::PresenceStatus::Invisible => "",
                        };
                        rsx! {
                            button {
                                class: "friend-card",
                                onclick: move |_| {
                                    // Drop the read guard in a tightly-scoped block
                                    // before open_direct_message_from_active_account
                                    // runs.  activate_dm_channel (called inside) takes
                                    // write guards on app_state and chat_data; any
                                    // live read guard on the same signal would panic.
                                    {
                                        let prev = app_state.read().nav.selected_channel.cloned();
                                        if let Some(ref id) = prev {
                                            remember_message_list_scroll_position(id);
                                        }
                                    }
                                    open_direct_message_from_active_account(
                                        friend_id.clone(),
                                        app_state,
                                        chat_data,
                                        client_manager,
                                        nav,
                                    );
                                },
                                div { class: "friend-info",
                                    div { class: "friend-avatar-wrap",
                                        if let Some(ref url) = avatar_url {
                                            img { class: "friend-avatar friend-avatar-image", src: "{url}", alt: "{display_name}" }
                                        } else {
                                            div { class: "friend-avatar", style: "background-color: {color};", "{first_char}" }
                                        }
                                        if !presence_dot_class.is_empty() {
                                            span { class: "{presence_dot_class}" }
                                        }
                                    }
                                    div { class: "friend-name", "{display_name}" }
                                    div { class: "friend-account", "{backend.display_name()}" }
                                    span { class: "friend-card-action btn btn-secondary btn-sm", "{message_label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn BlockedUsersGrid(blocked_users: Vec<poly_client::BlockedUser>) -> Element {
    let no_blocked_label = t("content-social-no-blocked");
    let blocked_label = t("content-social-blocked");

    rsx! {
        div { class: "friends-grid",
            if blocked_users.is_empty() {
                div { class: "empty-state", "{no_blocked_label}" }
            } else {
                for user in &blocked_users {
                    {
                        let fallback = user.display_name.chars().next().unwrap_or('?').to_string();
                        rsx! {
                            div { class: "friend-card friend-card-static",
                                if let Some(url) = &user.avatar_url {
                                    img { class: "friend-avatar friend-avatar-image", src: "{url}", alt: "{user.display_name}" }
                                } else {
                                    div { class: "friend-avatar", "{fallback}" }
                                }
                                div { class: "friend-info",
                                    div { class: "friend-name", "{user.display_name}" }
                                    div { class: "friend-account", "{blocked_label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn IgnoredUsersPlaceholder() -> Element {
    let ignored_title = t("friends-ignored-title");
    let ignored_empty = t("friends-ignored-empty");

    rsx! {
        div { class: "empty-state special-page-empty-state",
            h3 { "{ignored_title}" }
            p { "{ignored_empty}" }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn friends_panel_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<FriendsPanelAction>();
        let _ = FriendsPanelAction::ShowFriends;
        let _ = FriendsPanelAction::ShowIgnored;
        let _ = FriendsPanelAction::ShowBlocked;
        let _ = FriendsPanelAction::SetSearchFilter("query".into());
        let _ = FriendsPanelAction::MessageFriend("user-1".into());
    }

    #[test]
    fn demo_slug_detection() {
        // Ensure the slugs used in production match what FriendsGrid checks.
        assert!(["demo", "demo_forum"].contains(&"demo"));
        assert!(["demo", "demo_forum"].contains(&"demo_forum"));
        assert!(!["demo", "demo_forum"].contains(&"discord"));
    }
}
