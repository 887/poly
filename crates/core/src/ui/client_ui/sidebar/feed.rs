//! `SidebarLayoutKind::Feed` — HN / Mastodon feed tabs.
//!
//! P26 (Pack D): the six HN feed rows are now **clickable** — each row
//! dispatches the backend's `invoke_sidebar_action` with a stable action ID
//! (`feed-top`, `feed-new`, …, `feed-jobs`), and the selected feed is tracked
//! locally so the user sees visual feedback. Routing to the actual feed view
//! is a later pack (E); for now the selection shows which feed would be
//! navigated to.
//!
//! TODO(D28, long-term): migrate to a `SidebarLayoutKind::Custom` declared
//! by the hackernews plugin itself — the plugin ships the 6 feeds as
//! declared `sidebar-items`, and the generic `CustomSidebar` renders them.
//! That moves the feed list from host-hardcoded to plugin-declared and lets
//! alternate HN-style backends (e.g. Lobste.rs) declare their own feeds.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::AppState;
use crate::ui::client_ui::action_outcome::{handle_action_outcome, ActionOutcomeCx};
use crate::ui::client_ui::toast::ToastMessage;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Static hard-coded feed tabs used by HN-style sidebars.
/// Tuple fields: `(action_id, label_key)`.
const FEEDS: &[(&str, &str)] = &[
    ("feed-top", "ui-sidebar-feed-top"),
    ("feed-new", "ui-sidebar-feed-new"),
    ("feed-best", "ui-sidebar-feed-best"),
    ("feed-ask", "ui-sidebar-feed-ask"),
    ("feed-show", "ui-sidebar-feed-show"),
    ("feed-jobs", "ui-sidebar-feed-jobs"),
];

/// HN-style list of feed tabs. Each row is clickable and dispatches
/// `invoke_sidebar_action` on the active account's backend.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn FeedLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    // Track which feed the user selected most recently. Used only for
    // visual feedback; the routing side-effect is driven by the
    // `invoke_sidebar_action` → `ActionOutcome::Navigate` path.
    let mut active_feed = use_signal(String::new);

    let account_id = app_state.read().nav.active_account_id.cloned();
    let current = active_feed.read().clone();

    rsx! {
        aside { class: "client-sidebar feed-layout",
            h2 { class: "sidebar-header", {t("ui-sidebar-feed-header")} }
            if !current.is_empty() {
                div {
                    class: "feed-active-indicator",
                    aria_live: "polite",
                    {format!("{}: {}", t("ui-sidebar-feed-selected"), current)}
                }
            }
            ul { class: "feed-list", role: "tablist",
                {FEEDS.iter().map(|(id, label_key)| {
                    let id_s: String = (*id).to_string();
                    let label = t(label_key);
                    let selected = current == id_s;
                    let class = if selected { "feed-row selected" } else { "feed-row" };
                    let account_id = account_id.clone();
                    let client_manager = client_manager;
                    let id_for_click = id_s.clone();
                    let onclick = move |_evt: MouseEvent| {
                        let id_disp = id_for_click.clone();
                        active_feed.set(id_disp.clone());
                        let Some(account_id) = account_id.clone() else {
                            tracing::warn!("FeedLayout: no active account — action {id_disp} ignored");
                            return;
                        };
                        let action_id = id_disp.clone();
                        let client_manager = client_manager;
                        spawn(async move {
                            dispatch_feed_action(client_manager, account_id, action_id).await;
                        });
                    };
                    rsx! {
                        li {
                            key: "{id_s}",
                            class: "{class}",
                            role: "tab",
                            aria_selected: "{selected}",
                            tabindex: "0",
                            onclick,
                            "{label}"
                        }
                    }
                })}
            }
        }
    }
}

/// Invoke `invoke_sidebar_action` on the backend and route the outcome
/// through the shared [`handle_action_outcome`] handler.
async fn dispatch_feed_action(
    client_manager: Signal<ClientManager>,
    account_id: String,
    action_id: String,
) {
    let Some(backend) = client_manager.read().get_backend(&account_id) else {
        tracing::warn!("FeedLayout: no backend for account {account_id}");
        return;
    };
    let outcome = {
        let guard = backend.read().await;
        guard.invoke_sidebar_action(&action_id).await
    };
    let Some(toast_queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() else {
        tracing::debug!("FeedLayout: no toast queue in context — logging only");
        tracing::info!("FeedLayout: action outcome (no-toast-ctx): {outcome:?}");
        return;
    };
    let Some(refresh_sidebar) = try_consume_context::<Signal<u32>>() else {
        tracing::debug!("FeedLayout: no sidebar refresh signal in context");
        return;
    };
    let cx = ActionOutcomeCx {
        toast_queue,
        refresh_sidebar,
        refresh_target: None,
        client_manager,
        account_id,
    };
    handle_action_outcome(outcome, cx);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::FEEDS;

    /// P26: the six HN feed action IDs are stable and prefixed `feed-`.
    /// Plugins rely on this contract to match on the action ID when
    /// `invoke_sidebar_action` lands.
    #[test]
    fn feed_action_ids_are_stable() {
        let ids: Vec<&str> = FEEDS.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            ids,
            ["feed-top", "feed-new", "feed-best", "feed-ask", "feed-show", "feed-jobs"]
        );
        for id in &ids {
            assert!(
                id.starts_with("feed-"),
                "feed action ids must be `feed-*` (got {id})"
            );
        }
    }

    /// P26: every feed row has a non-empty FTL label key; prevents
    /// regressions where a new feed is added without a translation.
    #[test]
    fn feed_label_keys_non_empty() {
        for (id, key) in FEEDS {
            assert!(!key.is_empty(), "feed {id} has empty label key");
            assert!(
                key.starts_with("ui-sidebar-feed-"),
                "feed label keys must follow the `ui-sidebar-feed-*` convention ({key})"
            );
        }
    }
}
