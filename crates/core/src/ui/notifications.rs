//! Notifications view — aggregated notifications from all backends.

use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;

/// Notifications view component.
///
/// Shows aggregated notifications from all connected accounts/backends.
#[component]
pub fn NotificationsView(app_state: Signal<AppState>) -> Element {
    rsx! {
        div { class: "notifications-view",
            h2 { class: "notifications-title", "{t(\"notifications-title\")}" }

            // TODO(phase-2.7.8): Load notifications from backends
            div { class: "notification-list",
                div { class: "notification-item unread",
                    div { class: "notification-icon", "💬" }
                    div { class: "notification-content",
                        p { class: "notification-text", "Alice mentioned you in #general" }
                        span { class: "notification-time", "10 minutes ago" }
                    }
                    button { class: "notification-action", "{t(\"notifications-mark-read\")}" }
                }
                div { class: "notification-item unread",
                    div { class: "notification-icon", "👤" }
                    div { class: "notification-content",
                        p { class: "notification-text", "Iris sent you a friend request" }
                        span { class: "notification-time", "1 hour ago" }
                    }
                    button { class: "notification-action", "{t(\"notifications-dismiss\")}" }
                }
                div { class: "notification-item",
                    div { class: "notification-icon", "🏠" }
                    div { class: "notification-content",
                        p { class: "notification-text", "You've been invited to Rust Community" }
                        span { class: "notification-time", "5 hours ago" }
                    }
                }
            }
        }
    }
}
