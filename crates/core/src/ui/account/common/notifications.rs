//! Notifications view — aggregated notifications from all backends.
//!
//! Common implementation shared across all messenger backends.
//!
//! Reads from `BatchedSignal<ChatLists>` and displays notifications with
//! source badges indicating which backend/account they came from.
//! Features: mark-as-read, mark-all-as-read, filter by backend,
//! accept/decline friend requests, server invites, and voice invites.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.8): Wire notifications to backend data

use crate::state::BatchedSignal;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AccountSessions, AppState, ChatLists, NavState};
use crate::state::chat_data::backend_badge;
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::routes::Route;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::{
    BackendCapabilities, BackendType, ClientError, ConnectionStatus, FriendModel, NotificationKind,
    NotificationSupport, VoiceSupport, slug_supports_creating_server,
};
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the notifications view.
#[derive(Debug, Clone)]
pub enum NotificationsViewAction {
    /// User changed the kind filter.
    SetFilter(NotificationMenuFilter),
    /// User marked all (matching filter) notifications as read.
    MarkAllRead,
    /// User accepted a friend request.
    AcceptFriendRequest { notif_id: String, user_id: String },
    /// User denied a friend request.
    DenyFriendRequest { notif_id: String, user_id: String },
    /// User accepted a server invite.
    AcceptServerInvite(String),
    /// User dismissed a notification.
    Dismiss(String),
    /// User clicked reauth for an account.
    Reauth(String),
}

impl UiAction for NotificationsViewAction {
    fn apply(self, cx: ActionCx<'_>) {
        match self {
            Self::SetFilter(filter) => {
                // NotificationsView provides kind_filter via context so SetFilter can update it.
                if let Some(mut sig) = dioxus::prelude::try_consume_context::<Signal<NotificationMenuFilter>>() {
                    sig.set(filter);
                }
            }
            Self::MarkAllRead => {
                // Mark all notifications as read for the active account by removing them from
                // ChatLists. Cannot honour the current kind-filter (component-local Signal) from
                // here, so this removes all notifications for the active account unconditionally.
                let Some(chat_lists) = dioxus::prelude::try_consume_context::<BatchedSignal<ChatLists>>()
                else {
                    return;
                };
                let account_id = cx.nav.active_account_id.cloned().unwrap_or_default();
                if !account_id.is_empty() {
                    chat_lists.batch(move |cl| {
                        cl.notifications.retain(|n| n.account_id != account_id);
                    });
                }
            }
            Self::AcceptFriendRequest { notif_id, user_id } => {
                let Some(chat_lists) =
                    dioxus::prelude::try_consume_context::<BatchedSignal<ChatLists>>()
                else {
                    return;
                };
                let Some(client_manager) =
                    dioxus::prelude::try_consume_context::<BatchedSignal<ClientManager>>()
                else {
                    return;
                };
                // Derive account_id from the notification record itself.
                let account_id = {
                    let cl = chat_lists.peek();
                    cl.notifications
                        .iter()
                        .find(|n| n.id == notif_id)
                        .map(|n| n.account_id.clone())
                        .unwrap_or_default()
                };
                if account_id.is_empty() {
                    return;
                }
                // Optimistically remove the notification, then fire the backend call.
                {
                    let nid = notif_id.clone();
                    chat_lists.batch(move |cl| cl.notifications.retain(|n| n.id != nid));
                }
                let chat_lists_clone = chat_lists;
                dioxus::prelude::spawn(async move {
                    handle_friend_request_action(
                        client_manager,
                        chat_lists_clone,
                        account_id,
                        user_id,
                        notif_id,
                        true,
                    )
                    .await;
                });
            }
            Self::DenyFriendRequest { notif_id, user_id } => {
                let Some(chat_lists) =
                    dioxus::prelude::try_consume_context::<BatchedSignal<ChatLists>>()
                else {
                    return;
                };
                let Some(client_manager) =
                    dioxus::prelude::try_consume_context::<BatchedSignal<ClientManager>>()
                else {
                    return;
                };
                let account_id = {
                    let cl = chat_lists.peek();
                    cl.notifications
                        .iter()
                        .find(|n| n.id == notif_id)
                        .map(|n| n.account_id.clone())
                        .unwrap_or_default()
                };
                if account_id.is_empty() {
                    return;
                }
                {
                    let nid = notif_id.clone();
                    chat_lists.batch(move |cl| cl.notifications.retain(|n| n.id != nid));
                }
                let chat_lists_clone = chat_lists;
                dioxus::prelude::spawn(async move {
                    handle_friend_request_action(
                        client_manager,
                        chat_lists_clone,
                        account_id,
                        user_id,
                        notif_id,
                        false,
                    )
                    .await;
                });
            }
            Self::AcceptServerInvite(notif_id) | Self::Dismiss(notif_id) => {
                // Remove the notification from ChatLists. Deeper backend operations (joining a
                // server for an invite) require UI flow not expressible in apply(); the inline
                // component handles those. From the action system we at minimum clear the badge.
                let Some(chat_lists) =
                    dioxus::prelude::try_consume_context::<BatchedSignal<ChatLists>>()
                else {
                    return;
                };
                chat_lists.batch(move |cl| cl.notifications.retain(|n| n.id != notif_id));
            }
            Self::Reauth(account_id) => {
                // Navigator is optional in ActionCx — only available at runtime.
                if let Some(ref nav) = cx.navigator {
                    let instance_id = cx
                        .nav
                        .active_instance_id
                        .cloned()
                        .unwrap_or_default();
                    let backend = cx
                        .nav
                        .active_backend
                        .cloned()
                        .map(|b| b.slug().to_string())
                        .unwrap_or_default();
                    nav.push(Route::ReauthAccount {
                        backend,
                        instance_id,
                        account_id,
                    });
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotificationMenuFilter {
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

    /// True if this filter is meaningful for a backend with the given capabilities.
    ///
    /// The registry is derived from `BackendCapabilities` and the backend slug so
    /// a new plugin can't accidentally grow a "Voice invites" filter just by
    /// showing up in the account list — the capability declaration is the single
    /// source of truth.
    #[must_use] 
    pub fn supported_by(self, caps: &BackendCapabilities, slug: &str) -> bool {
        if matches!(caps.notifications, NotificationSupport::None) {
            return false;
        }
        match self {
            Self::All | Self::Mentions | Self::Other => true,
            Self::FriendRequests => !matches!(caps.friends, FriendModel::None),
            Self::ServerInvites => slug_supports_creating_server(slug),
            Self::VoiceInvites => matches!(caps.voice, VoiceSupport::Full),
        }
    }
}

/// Build the sidebar filter list for a given backend using its capability set.
///
/// Kept as a pub(crate) free function so the inline regression test below
/// can pin the matrix without instantiating the component.
/// Callers must supply the `BackendCapabilities` from
/// `client_manager.peek().capabilities_for_slug(slug)`.
pub(crate) fn filters_for_backend(slug: &str, caps: poly_client::BackendCapabilities) -> Vec<NotificationMenuFilter> {
    [
        NotificationMenuFilter::All,
        NotificationMenuFilter::Mentions,
        NotificationMenuFilter::FriendRequests,
        NotificationMenuFilter::ServerInvites,
        NotificationMenuFilter::VoiceInvites,
        NotificationMenuFilter::Other,
    ]
    .into_iter()
    .filter(|f| f.supported_by(&caps, slug))
    .collect()
}

/// Notifications view component.
///
/// Shows notifications scoped to the given account, with filtering by kind
/// and mark-read actions. `backend_slug` drives the capability-based filter
/// registry (WP-5) — e.g., GitHub hides FriendRequests/ServerInvites/VoiceInvites
/// because its BackendCapabilities declares no friends/servers/voice.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(NotificationsViewAction)]
#[component]
pub fn NotificationsView(account_id: String, backend_slug: String) -> Element {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();
    let mut kind_filter = use_signal(|| NotificationMenuFilter::All);
    use_context_provider(|| kind_filter);
    let notifications = chat_lists.read().notifications.iter()
        .filter(|n| n.account_id == account_id)
        .cloned()
        .collect::<Vec<_>>();
    let notifications_title = t("notifications-title");
    let notifications_empty = t("notifications-empty");
    let notifications_mark_read = t("notifications-mark-read");

    // Apply filter
    let filtered: Vec<_> = notifications
        .iter()
        .filter(|n| kind_filter.read().matches(&n.kind))
        .cloned()
        .collect();

    let total_count = notifications.len();
    // Detect whether the active account's stored token is rejected (401).
    // Only surface the reauth CTA in that case — hidden otherwise.
    let client_manager_sig: BatchedSignal<ClientManager> = use_context();
    let caps = client_manager_sig.peek().capabilities_for_slug(&backend_slug);
    let sidebar_filters = filters_for_backend(&backend_slug, caps);
    let needs_reauth = client_manager_sig
        .read()
        .connection_statuses
        .get(&account_id)
        .is_some_and(ConnectionStatus::needs_reauth);
    let reauth_instance_id = account_sessions
        .read()
        .account_sessions
        .get(&account_id).map_or_else(|| "demo".to_string(), |s| s.instance_id.clone());

    rsx! {
        SplitMenuShell {
            root_class: "notifications-shell".to_string(),
            sidebar_class: "special-page-sidebar notifications-sidebar".to_string(),
            content_class: "special-page-content notifications-content".to_string(),
            sidebar: rsx! {
                div { class: "special-page-sidebar-header",
                    h2 { class: "special-page-sidebar-title", "{notifications_title}" }
                    p { class: "special-page-sidebar-description", "{notifications_empty}" }
                }
                if needs_reauth {
                    {
                        let aid = account_id.clone();
                        let slug = backend_slug.clone();
                        let iid = reauth_instance_id.clone();
                        rsx! {
                            div { class: "special-page-sidebar-nav notifications-reauth-cta",
                                button {
                                    class: "special-page-sidebar-button notif-reauth-button",
                                    onclick: move |_| {
                                        crate::nav!(Route::ReauthAccount {
                                            backend: slug.clone(),
                                            instance_id: iid.clone(),
                                            account_id: aid.clone(),
                                        });
                                    },
                                    span { class: "notif-reauth-icon", "🔑" }
                                    span { class: "special-page-sidebar-button-label",
                                        "{t(\"notifications-reconnect\")}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "special-page-sidebar-nav",
                    for filter in sidebar_filters {
                        NotificationSidebarButton {
                            key: "{filter.label_key()}",
                            label: t(filter.label_key()),
                            count: notifications.iter().filter(|n| filter.matches(&n.kind)).count(),
                            active: *kind_filter.read() == filter,
                            onclick: move |_| kind_filter.set(filter),
                        }
                    }
                }
                div { class: "special-page-sidebar-section notifications-sidebar-actions",
                    if total_count > 0 {
                        button {
                            class: "special-page-sidebar-button",
                            onclick: move |_| crate::dispatch_action!(NotificationsViewAction::MarkAllRead, app_state, nav_state, navigator()),
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
                                if total_count > 0 {
                                    span { class: "notif-badge", " {total_count}" }
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

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn NotificationList(notifications: Vec<poly_client::Notification>) -> Element {
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
                    let time_ago = format_time_ago(notif.timestamp);
                    let notif_id = notif.id.clone();
                    rsx! {
                        div { class: "notification-item",
                            NotificationItemContent {
                                notif_id: notif_id.clone(),
                                account_id: notif.account_id.clone(),
                                kind: notif.kind.clone(),
                                badge: badge.to_string(),
                                preview,
                                time_ago,
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Inner content for a single notification item, with kind-specific action buttons.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn NotificationItemContent(
    notif_id: String,
    account_id: String,
    kind: NotificationKind,
    badge: String,
    preview: String,
    time_ago: String,
) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();
    let (kind_icon, kind_label) = match &kind {
        NotificationKind::Mention { .. } => ("💬", "Mention"),
        NotificationKind::FriendRequest { .. } => ("👤", "Friend Request"),
        NotificationKind::ServerInvite { .. } => ("🏠", "Server Invite"),
        NotificationKind::VoiceChannelInvite { .. } => ("🔊", "Voice Invite"),
        NotificationKind::ReauthRequired { .. } => ("🔑", "Reconnect"),
        NotificationKind::Other(_) => ("🔔", "Notification"),
    };

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
                        let nid_accept = notif_id.clone();
                        let nid_deny = notif_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-success btn-sm notif-action-accept",
                                onclick: {
                                    let uid = user_id.clone();
                                    let nid = nid_accept.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::AcceptFriendRequest { notif_id: nid.clone(), user_id: uid.clone() },
                                        app_state, nav_state, navigator()
                                    )
                                },
                                "{t(\"notifications-accept\")}"
                            }
                            button {
                                class: "btn btn-ghost btn-sm notif-action-deny",
                                onclick: {
                                    let uid = user_id.clone();
                                    let nid = nid_deny.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::DenyFriendRequest { notif_id: nid.clone(), user_id: uid.clone() },
                                        app_state, nav_state, navigator()
                                    )
                                },
                                "{t(\"notifications-deny\")}"
                            }
                        }
                    }
                    NotificationKind::ServerInvite { .. } => {
                        let nid_accept = notif_id.clone();
                        let nid_deny = notif_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-success btn-sm notif-action-accept",
                                onclick: {
                                    let nid = nid_accept.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::AcceptServerInvite(nid.clone()),
                                        app_state, nav_state, navigator()
                                    )
                                },
                                "{t(\"notifications-accept\")}"
                            }
                            button {
                                class: "btn btn-ghost btn-sm notif-action-deny",
                                onclick: {
                                    let nid = nid_deny.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::Dismiss(nid.clone()),
                                        app_state, nav_state, navigator()
                                    )
                                },
                                "{t(\"notifications-decline\")}"
                            }
                        }
                    }
                    NotificationKind::VoiceChannelInvite { .. } => {
                        let nid_join = notif_id.clone();
                        let nid_dismiss = notif_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-success btn-sm notif-action-join",
                                onclick: {
                                    let nid = nid_join.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::Dismiss(nid.clone()),
                                        app_state, nav_state, navigator()
                                    )
                                },
                                "{t(\"notifications-join-voice\")}"
                            }
                            button {
                                class: "btn btn-ghost btn-sm notif-action-deny",
                                onclick: {
                                    let nid = nid_dismiss.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::Dismiss(nid.clone()),
                                        app_state, nav_state, navigator()
                                    )
                                },
                                "{t(\"notifications-dismiss\")}"
                            }
                        }
                    }
                    NotificationKind::ReauthRequired { .. } => {
                        let aid = account_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-warning btn-sm notif-action-reauth",
                                onclick: move |_| crate::dispatch_action!(
                                    NotificationsViewAction::Reauth(aid.clone()),
                                    app_state, nav_state, navigator()
                                ),
                                "{t(\"notifications-reconnect\")}"
                            }
                        }
                    }
                    NotificationKind::Mention { .. } | NotificationKind::Other(_) => {
                        let nid = notif_id.clone();
                        rsx! {
                            button {
                                class: "btn btn-ghost btn-sm notif-action-read",
                                onclick: {
                                    let nid = nid.clone();
                                    move |_| crate::dispatch_action!(
                                        NotificationsViewAction::Dismiss(nid.clone()),
                                        app_state, nav_state, navigator()
                                    )
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

/// Format a timestamp as relative time (e.g., "5 minutes ago").
fn format_time_ago(ts: chrono::DateTime<chrono::Utc>) -> String {
    use crate::i18n::{t, t_args};

    let now = chrono::Utc::now();
    // lint-allow-unused: chrono Duration subtraction is checked internally;
    // overflow only on >290y deltas which the UI cannot produce.
    #[allow(clippy::arithmetic_side_effects)]
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
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    account_id: String,
    user_id: String,
    notif_id: String,
    accept: bool,
) {
    let refreshed_friends = match client_manager.peek().with_backend(&account_id, async |b| {
        match b.as_social_graph() {
            Some(sg) => {
                sg.respond_to_friend_request(&user_id, accept).await?;
                let friends = if accept {
                    match sg.get_friends().await {
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
                Ok(friends)
            }
            None => Err(ClientError::NotSupported(
                "respond_to_friend_request: backend has no social graph".to_string(),
            )),
        }
    }).await {
        Ok(friends) => friends,
        Err(err) => {
            tracing::warn!(
                "respond_to_friend_request failed for account {} user {}: {}",
                account_id, user_id, err
            );
            return;
        }
    };

    chat_lists.batch(move |cl| {
        cl.notifications
            .retain(|notification| notification.id != notif_id);

        if let Some(friends) = refreshed_friends {
            for friend in friends {
                if !cl.friends.get(&account_id).is_some_and(|v| v.iter().any(|existing| existing.id == friend.id)) {
                    cl.friends.entry(account_id.clone()).or_default().push(friend);
                }
            }
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn contains(filters: &[NotificationMenuFilter], f: NotificationMenuFilter) -> bool {
        filters.contains(&f)
    }

    /// Helper: build filters using the runtime registry seeded by `ClientManager::new()`.
    fn filters_for_slug(slug: &str) -> Vec<NotificationMenuFilter> {
        let cm = crate::client_manager::ClientManager::new();
        let caps = cm.capabilities_for_slug(slug);
        filters_for_backend(slug, caps)
    }

    #[test]
    fn discord_shows_all_filters() {
        let filters = filters_for_slug("discord");
        assert!(contains(&filters, NotificationMenuFilter::All));
        assert!(contains(&filters, NotificationMenuFilter::Mentions));
        assert!(contains(&filters, NotificationMenuFilter::FriendRequests));
        assert!(contains(&filters, NotificationMenuFilter::ServerInvites));
        assert!(contains(&filters, NotificationMenuFilter::VoiceInvites));
        assert!(contains(&filters, NotificationMenuFilter::Other));
    }

    #[test]
    fn stoat_omits_voice_and_server_invites() {
        // Stoat is full social chat but has no voice and does not support server
        // creation in Poly's model, so both voice and server invites must be hidden.
        let filters = filters_for_slug("stoat");
        assert!(contains(&filters, NotificationMenuFilter::FriendRequests));
        assert!(
            !contains(&filters, NotificationMenuFilter::ServerInvites),
            "stoat does not support creating servers — server invites must be hidden"
        );
        assert!(
            !contains(&filters, NotificationMenuFilter::VoiceInvites),
            "stoat has no voice — voice invites must be hidden"
        );
    }

    #[test]
    fn matrix_omits_voice_invites() {
        // Our current Matrix declaration has VoiceSupport::None.
        let filters = filters_for_slug("matrix");
        assert!(
            !contains(&filters, NotificationMenuFilter::VoiceInvites),
            "matrix has no voice — voice invites must be hidden"
        );
    }

    #[test]
    fn github_shows_only_read_activity_filters() {
        // GitHub is read-only with Activity-style notifications: no friends,
        // no create_server, no voice — the sidebar should collapse down.
        let filters = filters_for_slug("github");
        assert!(contains(&filters, NotificationMenuFilter::All));
        assert!(contains(&filters, NotificationMenuFilter::Mentions));
        assert!(contains(&filters, NotificationMenuFilter::Other));
        assert!(!contains(&filters, NotificationMenuFilter::FriendRequests));
        assert!(!contains(&filters, NotificationMenuFilter::ServerInvites));
        assert!(!contains(&filters, NotificationMenuFilter::VoiceInvites));
    }

    #[test]
    fn hackernews_has_no_filters() {
        // HN has NotificationSupport::None, so the entire sidebar should be empty.
        // (In practice the NotificationsRoute also redirects HN away, but this is
        // the defensive last line of defence.)
        let filters = filters_for_slug("hackernews");
        assert!(
            filters.is_empty(),
            "HN has NotificationSupport::None — filters: {filters:?}"
        );
    }

    #[test]
    fn lemmy_shows_mentions_no_social_invites() {
        // Lemmy has notifications (private messages / mentions) but no friends
        // and no create_server in Poly's declaration.
        let filters = filters_for_slug("lemmy");
        assert!(contains(&filters, NotificationMenuFilter::Mentions));
        assert!(!contains(&filters, NotificationMenuFilter::FriendRequests));
        assert!(!contains(&filters, NotificationMenuFilter::VoiceInvites));
    }

    #[test]
    fn unknown_slug_has_no_filters() {
        // Default capability preset is READ_ONLY_FEED → NotificationSupport::None.
        let filters = filters_for_slug("definitely-not-a-real-plugin");
        assert!(filters.is_empty());
    }

    #[test]
    fn notifications_view_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<NotificationsViewAction>();
        let _ = NotificationsViewAction::SetFilter(NotificationMenuFilter::All);
        let _ = NotificationsViewAction::MarkAllRead;
        let _ = NotificationsViewAction::AcceptFriendRequest {
            notif_id: "n1".into(),
            user_id: "u1".into(),
        };
        let _ = NotificationsViewAction::DenyFriendRequest {
            notif_id: "n2".into(),
            user_id: "u2".into(),
        };
        let _ = NotificationsViewAction::AcceptServerInvite("n3".into());
        let _ = NotificationsViewAction::Dismiss("n4".into());
        let _ = NotificationsViewAction::Reauth("acc".into());
    }
}
