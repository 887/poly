//! User sidebar — channel member list (right panel).

use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;

/// User sidebar component.
///
/// Shows channel members grouped by online/offline status.
#[component]
pub fn UserSidebar(app_state: Signal<AppState>) -> Element {
    rsx! {
        aside { class: "user-sidebar",
            h4 { class: "user-sidebar-title", "{t(\"user-members\")}" }

            // Online users section
            div { class: "user-group",
                h5 { class: "user-group-title", "{t(\"user-online\")} — 4" }
                // TODO(phase-2.7.7.1): Load from backend
                div { class: "user-entry",
                    div { class: "user-avatar online", "A" }
                    span { class: "user-name", "Alice" }
                }
                div { class: "user-entry",
                    div { class: "user-avatar online", "B" }
                    span { class: "user-name", "Bob" }
                }
                div { class: "user-entry",
                    div { class: "user-avatar idle", "D" }
                    span { class: "user-name", "Diana" }
                }
                div { class: "user-entry",
                    div { class: "user-avatar dnd", "E" }
                    span { class: "user-name", "Eve" }
                }
            }

            // Offline users section
            div { class: "user-group",
                h5 { class: "user-group-title", "{t(\"user-offline\")} — 2" }
                div { class: "user-entry offline",
                    div { class: "user-avatar offline", "F" }
                    span { class: "user-name", "Frank" }
                }
                div { class: "user-entry offline",
                    div { class: "user-avatar offline", "I" }
                    span { class: "user-name", "Iris" }
                }
            }
        }
    }
}
