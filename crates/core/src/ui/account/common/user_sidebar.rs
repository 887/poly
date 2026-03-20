//! User sidebar — channel member list (right panel).
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific user card decorations (roles, badges, verification)
//! will live in per-backend directories in future phases.
//!
//! ## Member filter
//! A collapsible filter row lives below the header. Click the 🔍 icon to
//! reveal the input; type to filter by display name (case-insensitive
//! substring). The matching substring is highlighted in each visible entry.
//!
//! Reads members from `Signal<ChatData>` and groups them by
//! presence status: Online, Idle, Do Not Disturb, Offline.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.7): Wire user sidebar to backend data

use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::user_color;
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User};

/// Renders a name with the matching substring highlighted via a `<mark>` element.
///
/// If `query` is empty or not found, renders the full name as plain text.
#[rustfmt::skip]
#[component]
fn HighlightedName(name: String, query: String) -> Element {
    if query.is_empty() {
        return rsx! { span { "{name}" } };
    }
    let lower_name = name.to_lowercase();
    let lower_q = query.to_lowercase();
    if let Some(pos) = lower_name.find(lower_q.as_str()) {
        let before = name[..pos].to_string();
        let matched = name[pos..pos + lower_q.len()].to_string();
        let after = name[pos + lower_q.len()..].to_string();
        rsx! {
            span {
                "{before}"
                mark { class: "member-name-highlight", "{matched}" }
                "{after}"
            }
        }
    } else {
        rsx! { span { "{name}" } }
    }
}

/// User sidebar component.
///
/// Shows channel members grouped by presence status.
/// A collapsible filter (🔍 icon → text input) lets users search by name.
#[rustfmt::skip]
#[component]
pub fn UserSidebar() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let members = chat_data.read().members.clone();
    let mut popup_user = use_signal(|| None::<User>);
    let mut filter_open = use_signal(|| false);
    let mut filter_text = use_signal(String::new);

    let query = filter_text.read().clone();
    let lower_q = query.to_lowercase();

    // Apply filter across all members (empty query = show all)
    let visible: Vec<_> = if lower_q.is_empty() {
        members.to_vec()
    } else {
        members
            .iter()
            .filter(|u| u.display_name.to_lowercase().contains(&lower_q))
            .cloned()
            .collect()
    };

    // Group by presence status
    let online: Vec<_> = visible.iter().filter(|u| u.presence == PresenceStatus::Online).cloned().collect();
    let idle: Vec<_> = visible.iter().filter(|u| u.presence == PresenceStatus::Idle).cloned().collect();
    let dnd: Vec<_> = visible.iter().filter(|u| u.presence == PresenceStatus::DoNotDisturb).cloned().collect();
    let offline: Vec<_> = visible.iter().filter(|u| {
        u.presence == PresenceStatus::Offline || u.presence == PresenceStatus::Invisible
    }).cloned().collect();

    rsx! {
        aside { class: "user-sidebar",
            div { class: "chat-utility-header user-sidebar-header",
                h3 { class: "chat-utility-title user-sidebar-title", "{t(\"user-members\")}" }
                button {
                    class: if *filter_open.read() {
                        "header-btn active chat-utility-filter-btn user-filter-btn user-filter-btn-active"
                    } else {
                        "header-btn chat-utility-filter-btn user-filter-btn"
                    },
                    title: "{t(\"member-filter-tooltip\")}",
                    onclick: move |_| {
                        let was_open = *filter_open.read();
                        filter_open.set(!was_open);
                        if was_open {
                            filter_text.set(String::new());
                        }
                    },
                    "🔍"
                }
            }

            div { class: "chat-utility-body user-sidebar-body",
                if *filter_open.read() {
                    div { class: "chat-utility-filter-row user-sidebar-filter-row",
                        input {
                            r#type: "text",
                            class: "chat-utility-filter-input member-filter-input",
                            placeholder: "{t(\"member-filter-placeholder\")}",
                            value: "{query}",
                            oninput: move |e| filter_text.set(e.value()),
                            autofocus: true,
                        }
                    }
                }

                if members.is_empty() {
                    div { class: "user-sidebar-empty", "{t(\"user-no-members\")}" }
                } else if visible.is_empty() {
                    div { class: "user-sidebar-empty", "{t(\"member-filter-no-results\")}" }
                }

                // Online
                if !online.is_empty() {
                    UserGroup {
                        label: format!("{} — {}", t("user-online"), online.len()),
                        users: online,
                        presence_class: "online",
                        query: query.clone(),
                        on_click: move |user: User| popup_user.set(Some(user)),
                    }
                }
                // Idle
                if !idle.is_empty() {
                    UserGroup {
                        label: format!("{} — {}", t("user-idle"), idle.len()),
                        users: idle,
                        presence_class: "idle",
                        query: query.clone(),
                        on_click: move |user: User| popup_user.set(Some(user)),
                    }
                }
                // Do Not Disturb
                if !dnd.is_empty() {
                    UserGroup {
                        label: format!("{} — {}", t("user-dnd"), dnd.len()),
                        users: dnd,
                        presence_class: "dnd",
                        query: query.clone(),
                        on_click: move |user: User| popup_user.set(Some(user)),
                    }
                }
                // Offline
                if !offline.is_empty() {
                    UserGroup {
                        label: format!("{} — {}", t("user-offline"), offline.len()),
                        users: offline,
                        presence_class: "offline",
                        query: query.clone(),
                        on_click: move |user: User| popup_user.set(Some(user)),
                    }
                }
            }

            // Profile popup
            if let Some(user) = popup_user.read().clone() {
                UserProfilePopup { user, on_close: move |_| popup_user.set(None) }
            }
        }
    }
}

/// A group of users under a presence header.
#[rustfmt::skip]
#[component]
fn UserGroup(
    label: String,
    users: Vec<User>,
    presence_class: &'static str,
    /// Current filter query for substring highlighting (empty = no highlight).
    query: String,
    on_click: EventHandler<User>,
) -> Element {
    rsx! {
        div { class: "user-group",
            h5 { class: "user-group-title", "{label}" }
            for user in &users {
                {
                    let color = user_color(&user.id);
                    let first_char: String = user
                        .display_name
                        .chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    let name = user.display_name.clone();
                    let u = user.clone();
                    let avatar_url = user.avatar_url.clone();
                    let q = query.clone();
                    let entry_class = if presence_class == "offline" {
                        "user-entry offline"
                    } else {
                        "user-entry"
                    };
                    rsx! {
                        div { class: "{entry_class}", onclick: move |_| on_click.call(u.clone()),
                            div { class: "user-avatar {presence_class}",
                                if let Some(ref url) = avatar_url {
                                    img { class: "user-avatar-image", src: "{url}", alt: "{name}" }
                                } else {
                                    div {
                                        class: "user-avatar-fallback",
                                        style: "background-color: {color};",
                                        "{first_char}"
                                    }
                                }
                            }
                            span { class: "user-name",
                                HighlightedName { name: name.clone(), query: q.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Profile popup shown when clicking a user in the sidebar.
#[rustfmt::skip]
#[component]
fn UserProfilePopup(user: User, on_close: EventHandler<()>) -> Element {
    let color = user_color(&user.id);
    let first_char: String = user
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let presence_label = match user.presence {
        PresenceStatus::Online => t("user-online"),
        PresenceStatus::Idle => t("user-idle"),
        PresenceStatus::DoNotDisturb => t("user-dnd"),
        PresenceStatus::Invisible => t("user-invisible"),
        PresenceStatus::Offline => t("user-offline"),
    };

    rsx! {
        div { class: "user-popup-overlay", onclick: move |_| on_close.call(()),
            div {
                class: "user-popup",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "user-popup-banner" }
                div { class: "user-popup-avatar",
                    if let Some(ref url) = user.avatar_url {
                        img {
                            class: "user-popup-avatar-image",
                            src: "{url}",
                            alt: "{user.display_name}",
                        }
                    } else {
                        div {
                            class: "user-popup-avatar-fallback",
                            style: "background-color: {color};",
                            "{first_char}"
                        }
                    }
                }
                div { class: "user-popup-info",
                    h3 { class: "user-popup-name", "{user.display_name}" }
                    div { class: "user-popup-status",
                        span { class: "status-dot", "{presence_label}" }
                    }
                    div { class: "user-popup-backend", "Source: {user.backend.display_name()}" }
                                // TODO(phase-3): mutual servers (2.6.3.1)
                // TODO(phase-3): roles list (2.6.3.2)
                }
                button {
                    class: "btn btn-secondary user-popup-close",
                    onclick: move |_| on_close.call(()),
                    "{t(\"action-close\")}"
                }
            }
        }
    }
}
