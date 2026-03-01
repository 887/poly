//! Server sidebar — favorited server icons with badges.

use crate::i18n::t;
use crate::state::{AppState, View};
use dioxus::prelude::*;

/// Server sidebar component.
///
/// Shows: DMs icon, Notifications icon, favorited server icons with
/// source badge overlay and account badge overlay.
#[component]
pub fn ServerSidebar(app_state: Signal<AppState>) -> Element {
    let current_view = app_state.read().nav.view;

    rsx! {
        nav { class: "server-sidebar",

            // DMs / Friends button
            div {
                class: if current_view == View::DmsFriends { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    app_state.write().nav.view = View::DmsFriends;
                    app_state.write().nav.selected_server = None;
                },
                title: "{t(\"nav-dms\")}",
                div { class: "icon-dms", "💬" }
            }

            // Notifications button
            div {
                class: if current_view == View::Notifications { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    app_state.write().nav.view = View::Notifications;
                },
                title: "{t(\"nav-notifications\")}",
                div { class: "icon-notifications", "🔔" }
                        // TODO(phase-2.7.3.6): Notification badge count
            }

            // Separator
            div { class: "sidebar-separator" }

            // Favorited servers
            // TODO(phase-2.7.3.3): Load from demo client / real backends
            div {
                class: "server-icon",
                onclick: move |_| {
                    app_state.write().nav.view = View::Server;
                    app_state.write().nav.selected_server = Some("server-poly-dev".to_string());
                },
                title: "Poly Development",
                div { class: "server-icon-letter", "P" }
                // Unread badge
                span { class: "badge", "5" }
            }

            div {
                class: "server-icon",
                onclick: move |_| {
                    app_state.write().nav.view = View::Server;
                    app_state.write().nav.selected_server = Some("server-gaming".to_string());
                },
                title: "Gaming Lounge",
                div { class: "server-icon-letter", "G" }
                span { class: "badge", "12" }
            }

            div {
                class: "server-icon",
                onclick: move |_| {
                    app_state.write().nav.view = View::Server;
                    app_state.write().nav.selected_server = Some("server-music".to_string());
                },
                title: "Music Enthusiasts",
                div { class: "server-icon-letter", "M" }
            }

            // Spacer to push settings to bottom
            div { class: "sidebar-spacer" }

            // Settings button
            div {
                class: if current_view == View::Settings { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    app_state.write().nav.view = View::Settings;
                },
                title: "{t(\"nav-settings\")}",
                div { class: "icon-settings", "⚙" }
            }
        }
    }
}
