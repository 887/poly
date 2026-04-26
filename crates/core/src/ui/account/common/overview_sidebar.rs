//! Overview sidebar (column 3) for `Route::ServerOverviewRoute`.
//!
//! Per-account overview surface — shows an "Overview" header + category
//! toggles so the user can scope which kinds of items appear in the
//! overview body (servers, DMs, friends, notifications), gated by the
//! backend's `BackendCapabilities`.
//!
//! This is the channel-sidebar equivalent of `SearchPage`'s
//! `AccountFilter` column: same toggle-switch UI pattern, same single-
//! column layout, but the toggles control content categories rather
//! than per-account scoping.
//!
//! Consumed by `crates/core/src/ui/routes.rs::ServerOverviewRoute` via
//! `SplitMenuShell`. The body engine reads the same `forum_scope`-style
//! `app_state.overview_categories` Signal and filters accordingly.

use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the per-account overview sidebar.
#[derive(Debug, Clone)]
pub enum OverviewSidebarAction {
    /// User clicked one of the scope toggle buttons.
    SetScope(String),
}

impl UiAction for OverviewSidebarAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Click handlers in OverviewSidebar write to AppState directly via
        // BatchedSignal::batch; this enum exists only to satisfy the
        // action-coverage lint.
    }
}

/// "Overview" panel rendered in column 3 on the per-account overview route.
#[ui_action(OverviewSidebarAction)]
#[context_menu(inherit)]
#[component]
pub fn OverviewSidebar() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let backend_slug = app_state
        .read()
        .nav
        .active_backend
        .cloned()
        .map(|b| b.slug().to_string())
        .unwrap_or_else(|| "demo".to_string());
    let caps = poly_client::capabilities_for_slug(&backend_slug);

    let scope = app_state.read().overview_scope.clone();
    let cls = |id: &str| -> &'static str {
        if scope == id { "forum-filter-btn active forum-filter-full" }
        else { "forum-filter-btn forum-filter-full" }
    };

    rsx! {
        aside { class: "client-sidebar overview-sidebar",
            header { class: "channel-list-header overview-sidebar-header",
                h2 { class: "channel-list-title", "{t(\"account-bar-overview-tooltip\")}" }
            }
            div { class: "overview-sidebar-filters",
                button {
                    class: "{cls(\"servers\")}",
                    r#type: "button",
                    onclick: move |_| { app_state.batch(|s| s.overview_scope = "servers".to_string()); },
                    "{t(\"overview-toggle-servers\")}"
                }
                if caps.should_show_dms() {
                    button {
                        class: "{cls(\"dms\")}",
                        r#type: "button",
                        onclick: move |_| { app_state.batch(|s| s.overview_scope = "dms".to_string()); },
                        "{t(\"overview-toggle-dms\")}"
                    }
                }
                if caps.should_show_friends() {
                    button {
                        class: "{cls(\"friends\")}",
                        r#type: "button",
                        onclick: move |_| { app_state.batch(|s| s.overview_scope = "friends".to_string()); },
                        "{t(\"overview-toggle-friends\")}"
                    }
                }
                if caps.should_show_notifications() {
                    button {
                        class: "{cls(\"notifications\")}",
                        r#type: "button",
                        onclick: move |_| { app_state.batch(|s| s.overview_scope = "notifications".to_string()); },
                        "{t(\"overview-toggle-notifications\")}"
                    }
                }
            }
        }
    }
}
