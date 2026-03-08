//! User sidebar — channel member list (right panel).
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific user card decorations (roles, badges, verification)
//! will live in per-backend directories in future phases.
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

/// User sidebar component.
///
/// Shows channel members grouped by presence status.
#[rustfmt::skip]
#[component]
pub fn UserSidebar() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let members = chat_data.read().members.clone();
    let mut popup_user = use_signal(|| None::<User>);

    // Group members by presence status
    let online: Vec<_> = members
        .iter()
        .filter(|u| u.presence == PresenceStatus::Online)
        .cloned()
        .collect();
    let idle: Vec<_> = members
        .iter()
        .filter(|u| u.presence == PresenceStatus::Idle)
        .cloned()
        .collect();
    let dnd: Vec<_> = members
        .iter()
        .filter(|u| u.presence == PresenceStatus::DoNotDisturb)
        .cloned()
        .collect();
    let offline: Vec<_> = members
        .iter()
        .filter(|u| {
            u.presence == PresenceStatus::Offline || u.presence == PresenceStatus::Invisible
        })
        .cloned()
        .collect();

    rsx! {
        aside { class: "user-sidebar",
            h4 { class: "user-sidebar-title", "{t(\"user-members\")}" }

            if members.is_empty() {
                div { class: "user-sidebar-empty", "{t(\"user-no-members\")}" }
            }

            // Online
            if !online.is_empty() {
                UserGroup {
                    label: format!("{} — {}", t("user-online"), online.len()),
                    users: online.clone(),
                    presence_class: "online",
                    on_click: move |user: User| popup_user.set(Some(user)),
                }
            }
            // Idle
            if !idle.is_empty() {
                UserGroup {
                    label: format!("{} — {}", t("user-idle"), idle.len()),
                    users: idle.clone(),
                    presence_class: "idle",
                    on_click: move |user: User| popup_user.set(Some(user)),
                }
            }
            // Do Not Disturb
            if !dnd.is_empty() {
                UserGroup {
                    label: format!("{} — {}", t("user-dnd"), dnd.len()),
                    users: dnd.clone(),
                    presence_class: "dnd",
                    on_click: move |user: User| popup_user.set(Some(user)),
                }
            }
            // Offline
            if !offline.is_empty() {
                UserGroup {
                    label: format!("{} — {}", t("user-offline"), offline.len()),
                    users: offline.clone(),
                    presence_class: "offline",
                    on_click: move |user: User| popup_user.set(Some(user)),
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
                            span { class: "user-name", "{name}" }
                        // TODO(phase-3): display roles when backend provides them (2.6.3.2)
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
