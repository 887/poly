//! Notifications view — aggregated notifications from all backends.
//!
//! Common implementation shared across all messenger backends.
//!
//! Reads from `Signal<ChatData>` and displays notifications with
//! source badges indicating which backend/account they came from.
//! Features: mark-as-read, mark-all-as-read, filter by backend,
//! accept/decline friend requests, server invites, and voice invites.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.8): Wire notifications to backend data

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::backend_badge;
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::{BackendType, NotificationKind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum NotificationMenuFilter {
    All,
    Mentions,
    FriendRequests,
    ServerInvites,
    VoiceInvites,
    Other,
}

impl NotificationMenuFilter {
    fn matches(self, kind: &NotificationKind) -> bool {
        match self {
            Self::All => true,
            Self::Mentions => matches!(kind, NotificationKind::Mention { .. }),
            Self::FriendRequests => matches!(kind, NotificationKind::FriendRequest { .. }),
            Self::ServerInvites => matches!(kind, NotificationKind::ServerInvite { .. }),
            Self::VoiceInvites => matches!(kind, NotificationKind::VoiceChannelInvite { .. }),
            Self::Other => matches!(kind, NotificationKind::Other(_)),
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::All => "notifications-filter-all-types",
            Self::Mentions => "notifications-filter-mentions",
            Self::FriendRequests => "notifications-filter-friend-requests",
            Self::ServerInvites => "notifications-filter-server-invites",
            Self::VoiceInvites => "notifications-filter-voice-invites",
            Self::Other => "notifications-filter-other",
        }
    }
}

/// Notifications view component.
///
/// Shows notifications scoped to the given account, with filtering by kind
/// and mark-read actions.
#[rustfmt::skip]
#[component]
pub fn NotificationsView(account_id: String) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    let mut show_unread_only = use_signal(|| false);
    let mut kind_filter = use_signal(|| NotificationMenuFilter::All);
    let notifications = chat_data.read().notifications.iter()
        .filter(|n| n.account_id == account_id)
        .cloned()
        .collect::<Vec<_>>();
    let notifications_title = t("notifications-title");
    let notifications_empty = t("notifications-empty");
    let notifications_unread_label = t("notifications-unread-count");
    let notifications_show_all = t("notifications-show-all");
    let notifications_show_unread = t("notifications-show-unread");
    let notifications_mark_read = t("notifications-mark-read");

    // Apply filter
    let filtered: Vec<_> = notifications
        .iter()
        .filter(|n| kind_filter.read().matches(&n.kind))
        .filter(|n| !*show_unread_only.read() || !n.read)
        .cloned()
        .collect();

    let has_unread = notifications.iter().any(|n| !n.read);
    let unread_count = notifications.iter().filter(|n| !n.read).count();
    let sidebar_filters = [
        NotificationMenuFilter::All,
        NotificationMenuFilter::Mentions,
        NotificationMenuFilter::FriendRequests,
        NotificationMenuFilter::ServerInvites,
        NotificationMenuFilter::VoiceInvites,
        NotificationMenuFilter::Other,
    ];

    rsx! {
        SplitMenuShell {
            root_class: "notifications-shell".to_string(),
            sidebar_class: "special-page-sidebar notifications-sidebar".to_string(),
            content_class: "special-page-content notifications-content".to_string(),
            sidebar: rsx! {
                div { class: "special-page-sidebar-header",
                    h2 { class: "special-page-sidebar-title", "{notifications_title}" }
                    if unread_count > 0 {
                        p { class: "special-page-sidebar-description", "{unread_count} {notifications_unread_label}" }
                    } else {
                        p { class: "special-page-sidebar-description", "{notifications_empty}" }
                    }
                }
                div { class: "special-page-sidebar-nav",
                    for filter in sidebar_filters {
                        NotificationSidebarButton {
                            key: "{filter.label_key()}",
                            label: t(filter.label_key()),
                            count: notifications.iter().filter(|n| !n.read && filter.matches(&n.kind)).count(),
                            active: *kind_filter.read() == filter,
                            onclick: move |_| kind_filter.set(filter),
                        }
                    }
                }
                div { class: "special-page-sidebar-section notifications-sidebar-actions",
                    button {
                        class: if *show_unread_only.read() { "special-page-sidebar-button active" } else { "special-page-sidebar-button" },
                        onclick: move |_| {
                            let current = *show_unread_only.read();
                            show_unread_only.set(!current);
                        },
                        if *show_unread_only.read() {
                            "{notifications_show_all}"
                        } else {
                            "{notifications_show_unread}"
                        }
                    }
                    if has_unread {
                        button {
                            class: "special-page-sidebar-button",
                            onclick: move |_| {
                                let active_kind = *kind_filter.read();
                                let mut cd = chat_data.write();
                                for notif in &mut cd.notifications {
                                    if notif.account_id == account_id && active_kind.matches(&notif.kind) {
                                        notif.read = true;
                                    }
                                }
                            },
                            "{notifications_mark_read}"
                        }
                    }
                }
                VoiceAccountFooter {}
            },
            content: rsx! {
                div { class: "notifications-view notifications-view-embedded",
                    div { class: "notifications-header special-page-header",
                        div { class: "notifications-title-row",
                            h2 { class: "notifications-title",
                                "{notifications_title}"
                                if unread_count > 0 {
                                    span { class: "notif-badge", " {unread_count}" }
                                }
                            }
                        }
                    }

                    NotificationList { notifications: filtered }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn NotificationSidebarButton(
    label: String,
    count: usize,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "special-page-sidebar-button special-page-sidebar-button-with-count active"
    } else {
        "special-page-sidebar-button special-page-sidebar-button-with-count"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |evt| onclick.call(evt),
            span { class: "special-page-sidebar-button-label", "{label}" }
            span { class: "special-page-sidebar-count", "{count}" }
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
                    let matched = backends.iter().find(|b| format!("{b:?}") == val).cloned();
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

/// Rendered list of notification items with per-kind action buttons.
#[rustfmt::skip]
#[component]
fn NotificationList(notifications: Vec<poly_client::Notification>) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

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
                    let time_ago = format_time_ago(notif.timestamp);
                    let notif_id = notif.id.clone();
                    let item_class = if is_unread {
                        "notification-item unread"
                    } else {
                        "notification-item"
                    };
                    rsx! {
                        div { class: "{item_class}",
                            NotificationItemContent {
                                notif_id: notif_id.clone(),
                                account_id: notif.account_id.clone(),
                                kind: notif.kind.clone(),
                                badge: badge.to_string(),
                                preview,
                                time_ago,
                                is_unread,
                                chat_data,
                                client_manager,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Inner content for a single notification item, with kind-specific action buttons.
#[rustfmt::skip]
#[component]
fn NotificationItemContent(
    notif_id: String,
    account_id: String,
    kind: NotificationKind,
    badge: String,
    preview: String,
    time_ago: String,
    is_unread: bool,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
) -> Element {
    let (kind_icon, kind_label) = match &kind {
        NotificationKind::Mention { .. } => ("💬", "Mention"),
        NotificationKind::FriendRequest { .. } => ("👤", "Friend Request"),
        NotificationKind::ServerInvite { .. } => ("🏠", "Server Invite"),
        NotificationKind::VoiceChannelInvite { .. } => ("🔊", "Voice Invite"),
        NotificationKind::Other(_) => ("🔔", "Notification"),
    };

    // Helper: dismiss a notification (mark read and remove from list)
    let dismiss_id = notif_id.clone();
    let accept_id = notif_id.clone();
    let mark_id = notif_id.clone();

    rsx! {
        div { class: "notification-icon", "{kind_icon}" }
        div { class: "notification-body",
            div { class: "notification-meta",
                span { class: "notification-source", "{badge}" }
                span { class: "notification-kind-label", "{kind_label}" }
                span { class: "notification-time", "{time_ago}" }
            }
            p { class: "notification-text", "{preview}" }
            div { class: "notification-actions",
                // Per-kind action buttons
                match &kind {
                    NotificationKind::FriendRequest { from_user_id } => {
                        let user_id = from_user_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-success btn-sm notif-action-accept",
                                onclick: {
                                    let uid = user_id.clone();
                                    let nid = accept_id.clone();
                                    let aid = account_id.clone();
                                    let cm = client_manager;
                                    move |_| {
                                        chat_data.write().notifications.retain(|n| n.id != nid);
                                        let uid = uid.clone();
                                        let nid = nid.clone();
                                        let aid = aid.clone();
                                        let chat_data = chat_data;
                                        spawn(async move {
                                            handle_friend_request_action(
                                                cm,
                                                chat_data,
                                                aid,
                                                uid,
                                                nid,
                                                true,
                                            )
                                            .await;
                                        });
                                    }
                                },
                                "{t(\"notifications-accept\")}"
                            }
                            button {
                                class: "btn btn-ghost btn-sm notif-action-deny",
                                onclick: {
                                    let nid = dismiss_id.clone();
                                    let uid = user_id.clone();
                                    let aid = account_id.clone();
                                    let cm = client_manager;
                                    move |_| {
                                        chat_data.write().notifications.retain(|n| n.id != nid);
                                        let uid = uid.clone();
                                        let nid = nid.clone();
                                        let aid = aid.clone();
                                        let chat_data = chat_data;
                                        spawn(async move {
                                            handle_friend_request_action(
                                                cm,
                                                chat_data,
                                                aid,
                                                uid,
                                                nid,
                                                false,
                                            )
                                            .await;
                                        });
                                    }
                                },
                                "{t(\"notifications-deny\")}"
                            }
                        }
                    }
                    NotificationKind::ServerInvite { .. } => {
                        rsx! {
                            button {
                                class: "btn btn-success btn-sm notif-action-accept",
                                onclick: {
                                    let nid = accept_id.clone();
                                    move |_| {
                                        chat_data.write().notifications.retain(|n| n.id != nid);
                                    }
                                },
                                "{t(\"notifications-accept\")}"
                            }
                            button {
                                class: "btn btn-ghost btn-sm notif-action-deny",
                                onclick: {
                                    let nid = dismiss_id.clone();
                                    move |_| {
                                        chat_data.write().notifications.retain(|n| n.id != nid);
                                    }
                                },
                                "{t(\"notifications-decline\")}"
                            }
                        }
                    }
                    NotificationKind::VoiceChannelInvite { channel_name, .. } => {
                        let _ch_name = channel_name.clone();
                        rsx! {
                            button {
                                class: "btn btn-success btn-sm notif-action-join",
                                onclick: {
                                    let nid = accept_id.clone();
                                    move |_| {
                                        // Dismiss the invite; navigation to voice channel
                                        // is handled by the calling context (TODO: deep link).
                                        chat_data.write().notifications.retain(|n| n.id != nid);
                                    }
                                },
                                "{t(\"notifications-join-voice\")}"
                            }
                            button {
                                class: "btn btn-ghost btn-sm notif-action-deny",
                                onclick: {
                                    let nid = dismiss_id.clone();
                                    move |_| {
                                        chat_data.write().notifications.retain(|n| n.id != nid);
                                    }
                                },
                                "{t(\"notifications-dismiss\")}"
                            }
                        }
                    }
                    NotificationKind::Mention { .. } | NotificationKind::Other(_) => {
                        if is_unread {
                            rsx! {
                                button {
                                    class: "btn btn-ghost btn-sm notif-action-read",
                                    onclick: {
                                        let nid = mark_id.clone();
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
                        } else {
                            rsx! {}
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

async fn handle_friend_request_action(
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    account_id: String,
    user_id: String,
    notif_id: String,
    accept: bool,
) {
    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        tracing::warn!(
            "No backend found for friend-request notification account {}",
            account_id
        );
        return;
    };

    let guard = backend.read().await;
    if let Err(err) = guard.respond_to_friend_request(&user_id, accept).await {
        tracing::warn!(
            "respond_to_friend_request failed for account {} user {}: {}",
            account_id,
            user_id,
            err
        );
        return;
    }

    let refreshed_friends = if accept {
        match guard.get_friends().await {
            Ok(friends) => Some(friends),
            Err(err) => {
                tracing::warn!(
                    "get_friends failed after accepting friend request for account {}: {}",
                    account_id,
                    err
                );
                None
            }
        }
    } else {
        None
    };
    drop(guard);

    let mut cd = chat_data.write();
    cd.notifications
        .retain(|notification| notification.id != notif_id);

    if let Some(friends) = refreshed_friends {
        for friend in friends {
            if !cd.friends.get(&account_id).map_or(false, |v| v.iter().any(|existing| existing.id == friend.id)) {
                cd.friends.entry(account_id.clone()).or_default().push(friend);
            }
        }
    }
}
