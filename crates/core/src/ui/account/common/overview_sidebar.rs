//! Overview sidebar (column 3) for the per-account `/overview/*` routes.
//!
//! Renders a Discord-channel-style menu with 4 sub-pages — each navigates
//! to its own route so browser back/forward works:
//!
//! - 🏠 General (Servers) — `/{...}/overview`
//! - 📬 Things you missed   — `/{...}/overview/missed`
//! - 📊 Stats               — `/{...}/overview/stats`
//! - 🤖 Agents              — `/{...}/overview/agents`
//!
//! Uses the same `special-page-sidebar` styling as Friends/Notifications so
//! the column width and look stay consistent.

use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the per-account overview sidebar.
#[derive(Debug, Clone)]
pub enum OverviewSidebarAction {
    /// User clicked one of the overview sub-page links.
    NavSubpage(String),
}

impl UiAction for OverviewSidebarAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Click handlers route via crate::nav! at the call site; this enum
        // exists only to satisfy the action-coverage lint.
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum OverviewPage {
    General,
    Missed,
    Stats,
    Agents,
}

fn current_page(route: &Route) -> OverviewPage {
    match route {
        Route::ServerOverviewMissedRoute { .. } => OverviewPage::Missed,
        Route::ServerOverviewStatsRoute { .. } => OverviewPage::Stats,
        Route::ServerOverviewAgentsRoute { .. } => OverviewPage::Agents,
        // lint-allow-unused: Route has dozens of variants; we deliberately
        // map any non-overview route to the General page.
        #[allow(clippy::wildcard_enum_match_arm)]
        _ => OverviewPage::General,
    }
}

/// "Overview" panel rendered in column 3 on every per-account overview route.
#[ui_action(OverviewSidebarAction)]
#[context_menu(inherit)]
#[component]
pub fn OverviewSidebar() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let _nav = navigator();
    let route: Route = use_route();
    let active = current_page(&route);

    let (backend, instance_id, account_id) = {
        let nav_state = app_state.read();
        let backend = nav_state.nav.active_backend.cloned().map_or_else(|| "demo".to_string(), |b| b.slug().to_string());
        let instance = nav_state.nav.active_instance_id.cloned()
            .unwrap_or_else(|| "demo".to_string());
        let account = nav_state.nav.active_account_id.cloned()
            .unwrap_or_default();
        (backend, instance, account)
    };

    let header_title = t("account-bar-overview-tooltip");

    rsx! {
        div { class: "special-page-sidebar-header",
            h2 { class: "special-page-sidebar-title", "{header_title}" }
        }
        div { class: "special-page-sidebar-nav",
            OverviewMenuButton {
                icon: "🏠",
                label: t("overview-page-general"),
                active: active == OverviewPage::General,
                onclick: {
                    let backend = backend.clone();
                    let instance_id = instance_id.clone();
                    let account_id = account_id.clone();
                    move |_| {
                        crate::nav!(Route::ServerOverviewRoute {
                            backend: backend.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                        });
                    }
                },
            }
            OverviewMenuButton {
                icon: "📬",
                label: t("overview-page-missed"),
                active: active == OverviewPage::Missed,
                onclick: {
                    let backend = backend.clone();
                    let instance_id = instance_id.clone();
                    let account_id = account_id.clone();
                    move |_| {
                        crate::nav!(Route::ServerOverviewMissedRoute {
                            backend: backend.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                        });
                    }
                },
            }
            OverviewMenuButton {
                icon: "📊",
                label: t("overview-page-stats"),
                active: active == OverviewPage::Stats,
                onclick: {
                    let backend = backend.clone();
                    let instance_id = instance_id.clone();
                    let account_id = account_id.clone();
                    move |_| {
                        crate::nav!(Route::ServerOverviewStatsRoute {
                            backend: backend.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                        });
                    }
                },
            }
            OverviewMenuButton {
                icon: "🤖",
                label: t("overview-page-agents"),
                active: active == OverviewPage::Agents,
                onclick: {
                    let backend = backend.clone();
                    let instance_id = instance_id.clone();
                    let account_id = account_id.clone();
                    move |_| {
                        crate::nav!(Route::ServerOverviewAgentsRoute {
                            backend: backend.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                        });
                    }
                },
            }
        }
    }
}

#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn OverviewMenuButton(
    icon: &'static str,
    label: String,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if active {
        "special-page-sidebar-button active overview-menu-button"
    } else {
        "special-page-sidebar-button overview-menu-button"
    };
    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            onclick: move |evt| onclick.call(evt),
            span { class: "overview-menu-icon", "{icon}" }
            span { class: "overview-menu-label", "{label}" }
        }
    }
}
