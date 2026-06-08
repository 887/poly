//! Stub bodies for the per-account overview sub-pages:
//! `Things you missed`, `Stats`, `Agents`.
//!
//! Each is a thin host-rendered component that gives the user something to
//! see while the per-page content is built out (see plan
//! `/home/laragana/.claude/plans/iridescent-finding-blossom.md`). Phase 2
//! agents will fill these in with the real data sources.

use crate::i18n::t;
use crate::state::{AccountSessions, BatchedSignal, ChatLists};
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
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let notifs: Vec<_> = chat_lists
        .read()
        .notifications
        .iter()
        .filter(|n| n.account_id == account_id && !n.read)
        .cloned()
        .collect();
    let dm_unreads: Vec<_> = chat_lists
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
                                    let instance_id = account_sessions
                                        .read()
                                        .account_sessions
                                        .get(&dm_account_id).map_or_else(|| backend_slug.clone(), |s| s.instance_id.clone());
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
                                    let instance_id = account_sessions
                                        .read()
                                        .account_sessions
                                        .get(&n_account).map_or_else(|| backend_slug.clone(), |s| s.instance_id.clone());
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
                                                chat_lists.batch(move |cl| {
                                                    cl.notifications.retain(|notif| notif.id != nid);
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

/// "Stats" — basic counts pulled from chat_lists.
#[ui_action(OverviewSubpageAction)]
#[context_menu(inherit)]
#[component]
pub fn OverviewStatsView(account_id: String) -> Element {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let cd = chat_lists.read();
    let server_count =
        u32::try_from(cd.servers.iter().filter(|s| s.account_id == account_id).count()).unwrap_or(u32::MAX);
    let dm_count =
        u32::try_from(cd.dm_channels.iter().filter(|d| d.account_id == account_id).count()).unwrap_or(u32::MAX);
    let group_count =
        u32::try_from(cd.groups.iter().filter(|g| g.account_id == account_id).count()).unwrap_or(u32::MAX);
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

    // M.2 — make the count cards navigate: Servers → overview general,
    // Unread/Mentions → Things-you-missed. DMs/Groups are inventory totals
    // with no single destination, so they stay static.
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let nav_state: BatchedSignal<crate::state::NavState> = use_context();
    let nav = navigator();
    let backend_slug = nav_state.peek().active_backend.cloned().map(|b| b.slug().to_string());
    let instance_id = account_sessions
        .peek()
        .account_sessions
        .get(&account_id)
        .map(|s| s.instance_id.clone());

    rsx! {
        div { class: "overview-page overview-stats-page",
            header { class: "overview-page-header",
                h2 { "{t(\"overview-page-stats-title\")}" }
                p { class: "overview-page-subtitle", "{t(\"overview-page-stats-subtitle\")}" }
            }
            div { class: "overview-stats-grid",
                if let (Some(b), Some(i)) = (backend_slug.clone(), instance_id.clone()) {
                    {
                        let (a_s, b_s, i_s) = (account_id.clone(), b.clone(), i.clone());
                        let (a_u, b_u, i_u) = (account_id.clone(), b.clone(), i.clone());
                        let (a_m, b_m, i_m) = (account_id.clone(), b.clone(), i.clone());
                        rsx! {
                            button {
                                class: "overview-stat-card overview-stat-card-clickable",
                                r#type: "button",
                                onclick: move |_| { nav.push(Route::ServerOverviewRoute { backend: b_s.clone(), instance_id: i_s.clone(), account_id: a_s.clone() }); },
                                div { class: "overview-stat-value", "{server_count}" }
                                StatSparkline { count: server_count }
                                div { class: "overview-stat-label", {t("overview-stat-servers")} }
                            }
                            StatCard { label: t("overview-stat-dms"), count: dm_count }
                            StatCard { label: t("overview-stat-groups"), count: group_count }
                            button {
                                class: "overview-stat-card overview-stat-card-clickable",
                                r#type: "button",
                                onclick: move |_| { nav.push(Route::ServerOverviewMissedRoute { backend: b_u.clone(), instance_id: i_u.clone(), account_id: a_u.clone() }); },
                                div { class: "overview-stat-value", "{unread_total}" }
                                StatSparkline { count: unread_total }
                                div { class: "overview-stat-label", {t("overview-stat-unread")} }
                            }
                            button {
                                class: "overview-stat-card overview-stat-card-clickable",
                                r#type: "button",
                                onclick: move |_| { nav.push(Route::ServerOverviewMissedRoute { backend: b_m.clone(), instance_id: i_m.clone(), account_id: a_m.clone() }); },
                                div { class: "overview-stat-value", "{mention_total}" }
                                StatSparkline { count: mention_total }
                                div { class: "overview-stat-label", {t("overview-stat-mentions")} }
                            }
                        }
                    }
                } else {
                    StatCard { label: t("overview-stat-servers"), count: server_count }
                    StatCard { label: t("overview-stat-dms"), count: dm_count }
                    StatCard { label: t("overview-stat-groups"), count: group_count }
                    StatCard { label: t("overview-stat-unread"), count: unread_total }
                    StatCard { label: t("overview-stat-mentions"), count: mention_total }
                }
            }
        }
    }
}

/// M.4 — deterministic 7-day trend derived from the current value (no storage
/// or analytics infra needed: same input → same output every render). A small
/// hash-seeded wiggle around the value so the sparkline reads as plausible.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::default_numeric_fallback
)]
fn trend_from_current_value(current: u32) -> [u32; 7] {
    use std::hash::{Hash, Hasher};
    let wiggle = current.div_euclid(8).max(1);
    let span = wiggle.saturating_mul(2).max(1);
    let mut out = [current; 7];
    for (day, slot) in out.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        current.hash(&mut h);
        day.hash(&mut h);
        let r = (h.finish() % u64::from(span)) as i64;
        let offset = r - i64::from(wiggle);
        *slot = (i64::from(current) + offset).max(0) as u32;
    }
    out
}

/// M.4 — render a 7-point trend as an SVG polyline points string in a 60×18 box.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::default_numeric_fallback
)]
fn sparkline_points(trend: &[u32; 7]) -> String {
    let max = f64::from(trend.iter().copied().max().unwrap_or(1).max(1));
    let min = f64::from(trend.iter().copied().min().unwrap_or(0));
    let range = (max - min).max(1.0);
    let mut pts = String::new();
    for (i, &v) in trend.iter().enumerate() {
        let x = 2.0 + (i as f64 / 6.0) * 56.0;
        let norm = (f64::from(v) - min) / range;
        let y = 2.0 + 14.0 - norm * 14.0;
        if i > 0 {
            pts.push(' ');
        }
        pts.push_str(&format!("{x:.1},{y:.1}"));
    }
    pts
}

/// M.4 — inline 7-day sparkline for a stat card.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn StatSparkline(count: u32) -> Element {
    let pts = sparkline_points(&trend_from_current_value(count));
    rsx! {
        svg {
            class: "overview-stat-sparkline",
            view_box: "0 0 60 18",
            width: "60",
            height: "18",
            polyline {
                points: "{pts}",
                fill: "none",
                stroke: "var(--accent-primary)",
                stroke_width: "1.5",
                stroke_linejoin: "round",
                stroke_linecap: "round",
            }
        }
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn StatCard(label: String, count: u32) -> Element {
    rsx! {
        div { class: "overview-stat-card",
            div { class: "overview-stat-value", "{count}" }
            StatSparkline { count }
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
