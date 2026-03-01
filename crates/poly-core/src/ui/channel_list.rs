//! Channel list — categories and channels for the selected server.

use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;

/// Channel list component.
///
/// Shows the server name header, categories (collapsible), and channels
/// with type icons (#, 🔊, 📹) and unread indicators.
#[component]
pub fn ChannelList(app_state: Signal<AppState>) -> Element {
    let server_id = app_state.read().nav.selected_server.clone();
    let selected_channel = app_state.read().nav.selected_channel.clone();

    rsx! {
        aside { class: "channel-list",
            // Server header
            div { class: "channel-list-header",
                h3 {
                    if let Some(ref _sid) = server_id {
                        // TODO(phase-2.7.5.1): Get server name from backend
                        "Server"
                    } else {
                        "{t(\"nav-dms\")}"
                    }
                }
            }

            // Channel entries
            div { class: "channel-entries",
                // TODO(phase-2.7.5.2): Render actual categories and channels from backend
                // Placeholder channels
                div {
                    class: if selected_channel.as_deref() == Some("ch-general") { "channel-item active" } else { "channel-item" },
                    onclick: move |_| {
                        app_state.write().nav.selected_channel = Some("ch-general".to_string());
                    },
                    span { class: "channel-icon", "#" }
                    span { class: "channel-name", "general" }
                    span { class: "unread-badge", "3" }
                }
                div {
                    class: if selected_channel.as_deref() == Some("ch-off-topic") { "channel-item active" } else { "channel-item" },
                    onclick: move |_| {
                        app_state.write().nav.selected_channel = Some("ch-off-topic".to_string());
                    },
                    span { class: "channel-icon", "#" }
                    span { class: "channel-name", "off-topic" }
                }
                div {
                    class: "channel-item",
                    onclick: move |_| {
                        app_state.write().nav.selected_channel = Some("ch-voice-dev".to_string());
                    },
                    span { class: "channel-icon", "🔊" }
                    span { class: "channel-name", "Dev Voice" }
                }
            }
        }
    }
}
