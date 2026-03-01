//! Notifications view — aggregated notifications from all backends.
//!
//! Reads from `Signal<ChatData>` and displays notifications with
//! source badges indicating which backend/account they came from.
// TODO(phase-2.5.8): Wire notifications to backend data

use crate::i18n::t;
use crate::state::chat_data::backend_badge;
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
use poly_client::NotificationKind;

/// Notifications view component.
///
/// Shows aggregated notifications from all connected accounts/backends.
#[component]
pub fn NotificationsView(app_state: Signal<AppState>) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let notifications = chat_data.read().notifications.clone();

    rsx! {
        div { class: "notifications-view",
            h2 { class: "notifications-title", "{t(\"notifications-title\")}" }

            div { class: "notification-list",
                if notifications.is_empty() {
                    div { class: "notifications-empty",
                        p { "{t(\"notifications-empty\")}" }
                    }
                }
                for notif in &notifications {
                    {
                        let badge = backend_badge(&notif.backend);
                        let preview = notif.preview.clone();
                        let is_unread = !notif.read;
                        let kind_icon = match &notif.kind {
                            NotificationKind::Mention { .. } => "💬",
                            NotificationKind::FriendRequest { .. } => "👤",
                            NotificationKind::ServerInvite { .. } => "🏠",
                            NotificationKind::Other(_) => "🔔",
                        };
                        let time_ago = format_time_ago(notif.timestamp);
                        let item_class = if is_unread {
                            "notification-item unread"
                        } else {
                            "notification-item"
                        };
                        rsx! {
                            div { class: "{item_class}",
                                div { class: "notification-icon", "{kind_icon}" }
                                span { class: "notification-source", "{badge}" }
                                div { class: "notification-content",
                                    p { class: "notification-text", "{preview}" }
                                    span { class: "notification-time", "{time_ago}" }
                                }
                                if is_unread {
                                    button { class: "notification-action", "{t(\"notifications-mark-read\")}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Format a timestamp as relative time (e.g., "5 minutes ago").
fn format_time_ago(ts: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now - ts;

    if diff.num_minutes() < 1 {
        "just now".to_string()
    } else if diff.num_minutes() < 60 {
        let m = diff.num_minutes();
        format!("{m} minute{} ago", if m == 1 { "" } else { "s" })
    } else if diff.num_hours() < 24 {
        let h = diff.num_hours();
        format!("{h} hour{} ago", if h == 1 { "" } else { "s" })
    } else {
        let d = diff.num_days();
        format!("{d} day{} ago", if d == 1 { "" } else { "s" })
    }
}
