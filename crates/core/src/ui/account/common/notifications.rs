//! Notifications view — aggregated notifications from all backends.
//!
//! Common implementation shared across all messenger backends.
//!
//! Reads from `Signal<ChatData>` and displays notifications with
//! source badges indicating which backend/account they came from.
//! Features: mark-as-read, mark-all-as-read, filter by backend.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.8): Wire notifications to backend data

use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::backend_badge;
use dioxus::prelude::*;
use poly_client::{BackendType, NotificationKind};

/// Notifications view component.
///
/// Shows aggregated notifications from all connected accounts/backends,
/// with filtering by backend and mark-read actions.
#[rustfmt::skip]
#[component]
pub fn NotificationsView() -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let mut filter_backend = use_signal(|| None::<BackendType>);
    let notifications = chat_data.read().notifications.clone();

    // Collect distinct backends present in notifications for filter
    let mut backends: Vec<BackendType> = notifications.iter().map(|n| n.backend).collect();
    backends.sort_by_key(|b| format!("{b:?}"));
    backends.dedup();

    // Apply filter
    let filtered: Vec<_> = notifications
        .iter()
        .filter(|n| filter_backend.read().is_none_or(|f| n.backend == f))
        .cloned()
        .collect();

    let has_unread = filtered.iter().any(|n| !n.read);

    rsx! {
        div { class: "notifications-view",
            div { class: "notifications-header",
                h2 { class: "notifications-title", "{t(\"notifications-title\")}" }
                // Backend filter
                if backends.len() > 1 {
                    NotificationFilter {
                        backends: backends.clone(),
                        selected: *filter_backend.read(),
                        on_change: move |b| filter_backend.set(b),
                    }
                }
                // Mark all as read
                if has_unread {
                    button {
                        class: "btn btn-secondary btn-sm notif-mark-all",
                        onclick: move |_| {
                            let filter = *filter_backend.read();
                            let mut cd = chat_data.write();
                            for notif in &mut cd.notifications {
                                if filter.is_none_or(|f| notif.backend == f) {
                                    notif.read = true;
                                }
                            }
                        },
                        "{t(\"notifications-mark-read\")}"
                    }
                }
            }

            NotificationList { notifications: filtered }
        }
    }
}

/// Backend filter dropdown for notifications.
#[rustfmt::skip]
#[component]
fn NotificationFilter(
    backends: Vec<BackendType>,
    selected: Option<BackendType>,
    on_change: EventHandler<Option<BackendType>>,
) -> Element {
    rsx! {
        select {
            class: "poly-select-native notif-filter",
            value: selected.map_or_else(String::new, |b| format!("{b:?}")),
            onchange: move |evt| {
                let val = evt.value();
                if val.is_empty() {
                    on_change.call(None);
                } else {
                    // Match backend type from debug name
                    let matched = backends.iter().find(|b| format!("{b:?}") == val).copied();
                    on_change.call(matched);
                }
            },
            option { value: "", "{t(\"filter-all\")}" }
            for b in &backends {
                {
                    let label = format!("{} {}", backend_badge(b), b.display_name());
                    let val = format!("{b:?}");
                    rsx! {
                        option { value: "{val}", "{label}" }
                    }
                }
            }
        }
    }
}

/// Rendered list of notification items.
#[rustfmt::skip]
#[component]
fn NotificationList(notifications: Vec<poly_client::Notification>) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();

    rsx! {
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
                    let notif_id = notif.id.clone();
                    rsx! {
                        div { class: "{item_class}",
                            div { class: "notification-icon", "{kind_icon}" }
                            span { class: "notification-source", "{badge}" }
                            div { class: "notification-content",
                                p { class: "notification-text", "{preview}" }
                                span { class: "notification-time", "{time_ago}" }
                            }
                            if is_unread {
                                button {
                                    class: "notification-action",
                                    onclick: {
                                        let nid = notif_id.clone();
                                        move |_| {
                                            let mut cd = chat_data.write();
                                            if let Some(n) = cd.notifications.iter_mut().find(|n| n.id == nid) {
                                                n.read = true;
                                            }
                                        }
                                    },
                                    "{t(\"notifications-mark-read\")}"
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
    use crate::i18n::{t, t_args};

    let now = chrono::Utc::now();
    let diff = now - ts;

    if diff.num_minutes() < 1 {
        t("time-just-now")
    } else if diff.num_minutes() < 60 {
        let m = diff.num_minutes();
        if m == 1 {
            t("time-one-minute-ago")
        } else {
            t_args("time-minutes-ago", &[("count", &m.to_string())])
        }
    } else if diff.num_hours() < 24 {
        let h = diff.num_hours();
        if h == 1 {
            t("time-one-hour-ago")
        } else {
            t_args("time-hours-ago", &[("count", &h.to_string())])
        }
    } else {
        let d = diff.num_days();
        if d == 1 {
            t("time-one-day-ago")
        } else {
            t_args("time-days-ago", &[("count", &d.to_string())])
        }
    }
}
