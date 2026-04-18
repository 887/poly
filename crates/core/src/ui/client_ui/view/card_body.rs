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
            div { class: "client-view-cards client-view-cards-loading",
                span { "Loading…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("CardBody: get_view_rows failed: {err:?}");
            rsx! {
                div { class: "client-view-cards client-view-cards-error",
                    span { "Failed to load cards" }
                }
            }
        }
        Some(Ok(page)) => {
            let rows = page.rows.clone();
            if rows.is_empty() {
                rsx! {
                    div { class: "client-view-cards client-view-cards-empty",
                        span { "No items" }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-cards",
                        for row in rows {
                            {
                                let id = row.id.clone();
                                let primary = row.primary_text.clone();
                                let secondary = row.secondary_text.clone();
                                let meta = row.meta_text.clone();
                                let icon = row.icon.clone();
                                rsx! {
                                    div { key: "{id}", class: "client-view-card",
                                        if let Some(icon) = icon {
                                            div { class: "client-view-card-icon", "{icon}" }
                                        }
                                        div { class: "client-view-card-primary", "{primary}" }
                                        if let Some(sec) = secondary {
                                            div { class: "client-view-card-secondary", "{sec}" }
                                        }
                                        if let Some(meta) = meta {
                                            div { class: "client-view-card-meta", "{meta}" }
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
