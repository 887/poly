//! Stub bodies for the per-account overview sub-pages:
//! `Things you missed`, `Stats`, `Agents`.
//!
//! Each is a thin host-rendered component that gives the user something to
//! see while the per-page content is built out (see plan
//! `/home/laragana/.claude/plans/iridescent-finding-blossom.md`). Phase 2
//! agents will fill these in with the real data sources.

use crate::i18n::t;
use crate::state::{BatchedSignal, ChatData};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::NotificationKind;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the overview sub-pages (placeholder; click handlers are
/// per-card and will be wired during the per-backend Phase 2).
#[derive(Debug, Clone)]
pub enum OverviewSubpageAction {
    /// User clicked something in the missed/stats/agents view.
    ItemClick(String),
}

impl UiAction for OverviewSubpageAction {
    fn apply(self, _cx: ActionCx<'_>) {}
}

/// "Things you missed" — recent unread notifications + recent friend DMs.
#[ui_action(OverviewSubpageAction)]
#[context_menu(inherit)]
#[component]
pub fn OverviewMissedView(account_id: String) -> Element {
    let chat_data: BatchedSignal<ChatData> = use_context();
    let notifs: Vec<_> = chat_data
        .read()
        .notifications
        .iter()
        .filter(|n| n.account_id == account_id && !n.read)
        .cloned()
        .collect();
    let dm_unreads: Vec<_> = chat_data
        .read()
        .dm_channels
        .iter()
        .filter(|dm| dm.account_id == account_id && dm.unread_count > 0)
        .cloned()
        .collect();

    rsx! {
        div { class: "overview-page overview-missed-page",
            header { class: "overview-page-header",
                h2 { "{t(\"overview-page-missed-title\")}" }
                p { class: "overview-page-subtitle", "{t(\"overview-page-missed-subtitle\")}" }
            }
            if notifs.is_empty() && dm_unreads.is_empty() {
                p { class: "overview-page-empty", "{t(\"overview-empty-allcaughtup\")}" }
            } else {
                if !dm_unreads.is_empty() {
                    section { class: "overview-section",
                        h3 { "{t(\"overview-section-unread-dms\")}" }
                        div { class: "overview-card-grid",
                            for dm in dm_unreads.iter() {
                                {
                                    let dm_id = dm.id.clone();
                                    let dm_account_id = dm.account_id.clone();
                                    let backend_slug = dm.backend.slug().to_string();
                                    let instance_id = chat_data
                                        .read()
                                        .account_sessions
                                        .get(&dm_account_id)
                                        .map(|s| s.instance_id.clone())
                                        .unwrap_or_else(|| backend_slug.clone());
                                    rsx! {
                                        button {
                                            key: "{dm.id}",
                                            class: "client-view-card view-row-card overview-card-clickable",
                                            r#type: "button",
                                            onclick: move |_| {
                                                crate::nav!(Route::DmChat {
                                                    backend: backend_slug.clone(),
                                                    instance_id: instance_id.clone(),
                                                    account_id: dm_account_id.clone(),
                                                    dm_id: dm_id.clone(),
                                                });
                                            },
                                            div { class: "client-view-card-primary view-row-primary",
                                                "{dm.user.display_name}"
                                            }
                                            div { class: "client-view-card-meta view-row-meta",
                                                "{dm.unread_count} unread"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if !notifs.is_empty() {
                    section { class: "overview-section",
                        h3 { "{t(\"overview-section-unread-notifications\")}" }
                        div { class: "overview-card-grid",
                            for n in notifs.iter() {
                                {
                                    let n_id = n.id.clone();
                                    let n_account = n.account_id.clone();
                                    let backend_slug = n.backend.slug().to_string();
                                    let instance_id = chat_data
                                        .read()
                                        .account_sessions
                                        .get(&n_account)
                                        .map(|s| s.instance_id.clone())
                                        .unwrap_or_else(|| backend_slug.clone());
                                    let kind = n.kind.clone();
                                    rsx! {
                                        button {
                                            key: "{n.id}",
                                            class: "client-view-card view-row-card overview-card-clickable",
                                            r#type: "button",
                                            onclick: move |_| {
                                                // Resolve a navigation target for each notification
                                                // kind. Mention lacks server_id and we don't have a
                                                // cheap channel→server lookup in ChatData, so we
                                                // fall back to the per-account NotificationsRoute
                                                // — the user can still act on it from there.
                                                let route = match &kind {
                                                    NotificationKind::FriendRequest { .. } => {
                                                        Route::FriendsRoute {
                                                            backend: backend_slug.clone(),
                                                            instance_id: instance_id.clone(),
                                                            account_id: n_account.clone(),
                                                        }
                                                    }
                                                    NotificationKind::ServerInvite { server_id } => {
                                                        Route::ServerHome {
                                                            backend: backend_slug.clone(),
                                                            instance_id: instance_id.clone(),
                                                            account_id: n_account.clone(),
                                                            server_id: server_id.clone(),
                                                        }
                                                    }
                                                    NotificationKind::VoiceChannelInvite {
                                                        server_id,
                                                        channel_id,
                                                        ..
                                                    } => Route::ServerChat {
                                                        backend: backend_slug.clone(),
                                                        instance_id: instance_id.clone(),
                                                        account_id: n_account.clone(),
                                                        server_id: server_id.clone(),
                                                        channel_id: channel_id.clone(),
                                                    },
                                                    NotificationKind::ReauthRequired { backend_slug: bs } => {
                                                        Route::ReauthAccount {
                                                            backend: bs.clone(),
                                                            instance_id: instance_id.clone(),
                                                            account_id: n_account.clone(),
                                                        }
                                                    }
                                                    NotificationKind::Mention { .. }
                                                    | NotificationKind::Other(_) => {
                                                        Route::NotificationsRoute {
                                                            backend: backend_slug.clone(),
                                                            instance_id: instance_id.clone(),
                                                            account_id: n_account.clone(),
                                                        }
                                                    }
                                                };
                                                let nid = n_id.clone();
                                                chat_data.batch(move |cd| {
                                                    cd.notifications.retain(|notif| notif.id != nid);
                                                });
                                                crate::nav!(route);
                                            },
                                            div { class: "client-view-card-primary view-row-primary",
                                                "{n.preview}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// "Stats" — basic counts pulled from chat_data.
#[ui_action(OverviewSubpageAction)]
#[context_menu(inherit)]
#[component]
pub fn OverviewStatsView(account_id: String) -> Element {
    let chat_data: BatchedSignal<ChatData> = use_context();
    let cd = chat_data.read();
    let server_count = cd.servers.iter().filter(|s| s.account_id == account_id).count();
    let dm_count = cd.dm_channels.iter().filter(|d| d.account_id == account_id).count();
    let group_count = cd.groups.iter().filter(|g| g.account_id == account_id).count();
    let unread_total: u32 = cd
        .servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .map(|s| s.unread_count)
        .sum();
    let mention_total: u32 = cd
        .servers
        .iter()
        .filter(|s| s.account_id == account_id)
        .map(|s| s.mention_count)
        .sum();

    rsx! {
        div { class: "overview-page overview-stats-page",
            header { class: "overview-page-header",
                h2 { "{t(\"overview-page-stats-title\")}" }
                p { class: "overview-page-subtitle", "{t(\"overview-page-stats-subtitle\")}" }
            }
            div { class: "overview-stats-grid",
                StatCard { label: t("overview-stat-servers"), value: server_count.to_string() }
                StatCard { label: t("overview-stat-dms"), value: dm_count.to_string() }
                StatCard { label: t("overview-stat-groups"), value: group_count.to_string() }
                StatCard { label: t("overview-stat-unread"), value: unread_total.to_string() }
                StatCard { label: t("overview-stat-mentions"), value: mention_total.to_string() }
            }
        }
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn StatCard(label: String, value: String) -> Element {
    rsx! {
        div { class: "overview-stat-card",
            div { class: "overview-stat-value", "{value}" }
            div { class: "overview-stat-label", "{label}" }
        }
    }
}

/// "Agents" — list of channels and DMs where the user has turned on agent
/// features for this account. When none are active, render an empty-state
/// card explaining how to enable them via the 🤖 header button next to the
/// member-list toggle in any chat.
///
/// Per-channel/DM agent enablement isn't persisted yet (the agent panel is a
/// UI-only toggle today), so the active list is always empty until that lands.
/// The empty state still ships now so users discover where to enable it.
#[ui_action(OverviewSubpageAction)]
#[context_menu(inherit)]
#[component]
pub fn OverviewAgentsView(account_id: String) -> Element {
    let _ = account_id;
    rsx! {
        div { class: "overview-page overview-agents-page",
            header { class: "overview-page-header",
                h2 { "{t(\"overview-page-agents-title\")}" }
                p { class: "overview-page-subtitle", "{t(\"overview-page-agents-subtitle\")}" }
            }
            div { class: "overview-empty-state",
                div { class: "overview-empty-icon", "🤖" }
                h3 { class: "overview-empty-title",
                    "{t(\"overview-page-agents-empty-title\")}"
                }
                p { class: "overview-empty-body",
                    "{t(\"overview-page-agents-empty-body\")}"
                }
            }
        }
    }
}
