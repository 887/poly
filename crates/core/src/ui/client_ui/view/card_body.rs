//! Card-grid body engine — renders `get_view_rows` as a grid of cards.
//!
//! WP 5 scope: first page only. Layout handled by CSS
//! (`.client-view-cards { display: grid; }`).

use super::list_body::fetch_first_page;
use dioxus::prelude::*;
use poly_client::CardSpec;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn CardBody(channel_id: String, account_id: String, spec: CardSpec) -> Element {
    let _ = spec;
    let rows_res = fetch_first_page(channel_id.clone(), account_id.clone());

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
            rsx! {
                div { class: "client-view-cards client-view-cards-error", role: "feed",
                    span { "Failed to load cards" }
                }
            }
        }
        Some(Ok(page)) => {
            let rows = page.rows.clone();
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
                                rsx! {
                                    div {
                                        key: "{id}",
                                        class: "client-view-card view-row-card",
                                        role: "article",
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
