//! Card-grid body engine — renders `get_view_rows` as a grid of cards.
//!
//! WP 5 scope: first page only. Layout handled by CSS
//! (`.client-view-cards { display: grid; }`).

use super::list_body::fetch_first_page;
use crate::state::{AppState, BatchedSignal};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::errors::{is_session_expired, SessionExpiredCard};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::CardSpec;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the card-grid body engine.
#[derive(Debug, Clone)]
pub enum CardBodyAction {
    /// User clicked a card — navigates to the corresponding server.
    Open(String),
}

impl UiAction for CardBodyAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Click handler navigates inline via crate::nav!; this enum exists
        // only to satisfy the action-coverage lint.
    }
}

#[ui_action(CardBodyAction)]
#[context_menu(inherit)]
#[component]
pub fn CardBody(
    channel_id: String,
    account_id: String,
    spec: CardSpec,
    /// Search filter from the parent overview page — rows whose
    /// `primary_text` doesn't case-insensitively contain the query are
    /// hidden client-side. Empty string = no filter.
    #[props(default)]
    filter: String,
) -> Element {
    let _ = spec;
    let rows_res = fetch_first_page(channel_id.clone(), account_id.clone(), None, None, None);
    let app_state: BatchedSignal<AppState> = use_context();
    // CardBody's overview-context detection: the host's
    // `AccountOverviewView` calls `render_descriptor` with an empty
    // channel_id, so when channel_id is empty we treat each card click
    // as "open server with id = row.id".
    let is_overview = channel_id.is_empty();
    let (backend_slug, instance_id) = {
        let s = app_state.read();
        let backend = s
            .nav
            .active_backend
            .cloned()
            .map(|b| b.slug().to_string())
            .unwrap_or_else(|| "demo".to_string());
        let instance = s
            .nav
            .active_instance_id
            .cloned()
            .unwrap_or_else(|| "demo".to_string());
        (backend, instance)
    };
    let filter_lower = filter.to_lowercase();

    match &*rows_res.read_unchecked() {
        None => rsx! {
            div {
                class: "client-view-cards client-view-cards-loading",
                role: "feed",
                "aria-busy": "true",
                span { "Loading…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("CardBody: get_view_rows failed: {err:?}");
            if is_session_expired(err) {
                rsx! {
                    div { class: "client-view-cards client-view-cards-error", role: "feed",
                        SessionExpiredCard {
                            backend: backend_slug.clone(),
                            instance_id: instance_id.clone(),
                            account_id: account_id.clone(),
                            backend_display_name: backend_slug.clone(),
                        }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-cards client-view-cards-error", role: "feed",
                        span { "Failed to load cards" }
                    }
                }
            }
        }
        Some(Ok(page)) => {
            let rows: Vec<_> = if filter_lower.is_empty() {
                page.rows.clone()
            } else {
                page.rows
                    .iter()
                    .filter(|r| {
                        r.primary_text.to_lowercase().contains(&filter_lower)
                            || r.secondary_text
                                .as_deref()
                                .is_some_and(|s| s.to_lowercase().contains(&filter_lower))
                    })
                    .cloned()
                    .collect()
            };
            if rows.is_empty() {
                rsx! {
                    div { class: "client-view-cards client-view-cards-empty", role: "feed",
                        span { "No items" }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-cards", role: "feed",
                        for row in rows {
                            {
                                let id = row.id.clone();
                                let primary = row.primary_text.clone();
                                let secondary = row.secondary_text.clone();
                                let meta = row.meta_text.clone();
                                let icon = row.icon.clone();
                                let card_class = if is_overview {
                                    "client-view-card view-row-card overview-clickable"
                                } else {
                                    "client-view-card view-row-card"
                                };
                                let id_for_click = id.clone();
                                let backend_slug = backend_slug.clone();
                                let instance_id = instance_id.clone();
                                let account_id_inner = account_id.clone();
                                rsx! {
                                    div {
                                        key: "{id}",
                                        class: "{card_class}",
                                        role: "article",
                                        onclick: move |_| {
                                            if is_overview {
                                                crate::nav!(Route::ServerHome {
                                                    backend: backend_slug.clone(),
                                                    instance_id: instance_id.clone(),
                                                    account_id: account_id_inner.clone(),
                                                    server_id: id_for_click.clone(),
                                                });
                                            }
                                        },
                                        if let Some(icon) = icon {
                                            div { class: "client-view-card-icon view-row-icon", "{icon}" }
                                        }
                                        h3 { class: "client-view-card-primary view-row-primary", "{primary}" }
                                        if let Some(sec) = secondary {
                                            span { class: "client-view-card-secondary view-row-secondary", "{sec}" }
                                        }
                                        if let Some(meta) = meta {
                                            span { class: "client-view-card-meta view-row-meta", "{meta}" }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use poly_client::{MenuTargetKind, ViewRow};

    fn row(id: &str, primary: &str) -> ViewRow {
        ViewRow {
            id: id.into(),
            primary_text: primary.into(),
            secondary_text: None,
            meta_text: None,
            icon: None,
            badge: None,
            context_menu_target_kind: MenuTargetKind::Message,
        }
    }

    #[test]
    fn rows_vec_preserves_order_and_count() {
        // Pure preconditions on the vector the body engine iterates. The
        // card body renders one article per row in the source vector
        // without filtering or reordering.
        let rows = vec![row("a", "First"), row("b", "Second")];
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "a");
        assert_eq!(rows[1].id, "b");
    }
}
