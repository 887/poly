//! User sidebar — channel member list (right panel).
//!
//! Reads members from `Signal<ChatData>` and groups them by
//! presence status: Online, Idle, Do Not Disturb, Offline.
// TODO(phase-2.5.7): Wire user sidebar to backend data

use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::user_color;
use dioxus::prelude::*;
use poly_client::PresenceStatus;

/// User sidebar component.
///
/// Shows channel members grouped by presence status.
#[component]
pub fn UserSidebar() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let members = chat_data.read().members.clone();

    // Group members by presence status
    let online: Vec<_> = members
        .iter()
        .filter(|u| u.presence == PresenceStatus::Online)
        .collect();
    let idle: Vec<_> = members
        .iter()
        .filter(|u| u.presence == PresenceStatus::Idle)
        .collect();
    let dnd: Vec<_> = members
        .iter()
        .filter(|u| u.presence == PresenceStatus::DoNotDisturb)
        .collect();
    let offline: Vec<_> = members
        .iter()
        .filter(|u| {
            u.presence == PresenceStatus::Offline || u.presence == PresenceStatus::Invisible
        })
        .collect();

    rsx! {
        aside { class: "user-sidebar",
            h4 { class: "user-sidebar-title", "{t(\"user-members\")}" }

            if members.is_empty() {
                div { class: "user-sidebar-empty", "No members to show" }
            }

            // Online
            if !online.is_empty() {
                div { class: "user-group",
                    h5 { class: "user-group-title", "{t(\"user-online\")} — {online.len()}" }
                    for user in &online {
                        {
                            let color = user_color(&user.id);
                            let first_char: String = user
                                .display_name
                                .chars()
                                .next()
                                .map(|c| c.to_string())
                                .unwrap_or_default();
                            let name = user.display_name.clone();
                            rsx! {
                                div { class: "user-entry",
                                    div { class: "user-avatar online", style: "background-color: {color};", "{first_char}" }
                                    span { class: "user-name", "{name}" }
                                }
                            }
                        }
                    }
                }
            }

            // Idle
            if !idle.is_empty() {
                div { class: "user-group",
                    h5 { class: "user-group-title", "{t(\"user-idle\")} — {idle.len()}" }
                    for user in &idle {
                        {
                            let color = user_color(&user.id);
                            let first_char: String = user
                                .display_name
                                .chars()
                                .next()
                                .map(|c| c.to_string())
                                .unwrap_or_default();
                            let name = user.display_name.clone();
                            rsx! {
                                div { class: "user-entry",
                                    div { class: "user-avatar idle", style: "background-color: {color};", "{first_char}" }
                                    span { class: "user-name", "{name}" }
                                }
                            }
                        }
                    }
                }
            }

            // Do Not Disturb
            if !dnd.is_empty() {
                div { class: "user-group",
                    h5 { class: "user-group-title", "{t(\"user-dnd\")} — {dnd.len()}" }
                    for user in &dnd {
                        {
                            let color = user_color(&user.id);
                            let first_char: String = user
                                .display_name
                                .chars()
                                .next()
                                .map(|c| c.to_string())
                                .unwrap_or_default();
                            let name = user.display_name.clone();
                            rsx! {
                                div { class: "user-entry",
                                    div { class: "user-avatar dnd", style: "background-color: {color};", "{first_char}" }
                                    span { class: "user-name", "{name}" }
                                }
                            }
                        }
                    }
                }
            }

            // Offline
            if !offline.is_empty() {
                div { class: "user-group",
                    h5 { class: "user-group-title", "{t(\"user-offline\")} — {offline.len()}" }
                    for user in &offline {
                        {
                            let color = user_color(&user.id);
                            let first_char: String = user
                                .display_name
                                .chars()
                                .next()
                                .map(|c| c.to_string())
                                .unwrap_or_default();
                            let name = user.display_name.clone();
                            rsx! {
                                div { class: "user-entry offline",
                                    div { class: "user-avatar offline", style: "background-color: {color};", "{first_char}" }
                                    span { class: "user-name", "{name}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
