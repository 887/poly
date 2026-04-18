//! Tree body engine — renders `get_view_rows` as a flat list with indentation.
//!
//! WP 5 initial: the plugin returns a flat ordered list of rows; the tree
//! hierarchy is not yet expressed through `ViewRow`, so this body engine
//! falls back to flat rendering. `TreeSpec::max_depth` and `root_page_size`
//! are honored when available: `max_depth` caps `rows.len()` at a conservative
//! upper bound so a misbehaving plugin can't blow up the UI; `root_page_size`
//! provides the initial visible count.

use super::list_body::fetch_first_page;
use dioxus::prelude::*;
use poly_client::TreeSpec;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn TreeBody(channel_id: String, account_id: String, spec: TreeSpec) -> Element {
    let rows_res = fetch_first_page(channel_id.clone(), account_id.clone());

    // Guard against runaway plugins — `max_depth * root_page_size` is a
    // reasonable upper ceiling on visible rows for the initial page.
    let max_rows = spec
        .root_page_size
        .saturating_mul(spec.max_depth.max(1))
        .max(spec.root_page_size) as usize;

    match &*rows_res.read_unchecked() {
        None => rsx! {
            div { class: "client-view-tree client-view-tree-loading",
                span { "Loading…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("TreeBody: get_view_rows failed: {err:?}");
            rsx! {
                div { class: "client-view-tree client-view-tree-error",
                    span { "Failed to load thread" }
                }
            }
        }
        Some(Ok(page)) => {
            let mut rows = page.rows.clone();
            if max_rows > 0 && rows.len() > max_rows {
                rows.truncate(max_rows);
            }
            if rows.is_empty() {
                rsx! {
                    div { class: "client-view-tree client-view-tree-empty",
                        span { "No items" }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-tree",
                        for (idx, row) in rows.into_iter().enumerate() {
                            {
                                let id = row.id.clone();
                                let primary = row.primary_text.clone();
                                let secondary = row.secondary_text.clone();
                                let meta = row.meta_text.clone();
                                // Without a parent/depth field on ViewRow, use
                                // a zero indent; the layout still renders flat
                                // and a follow-up can add depth once the WIT
                                // row carries it.
                                let depth = 0_u32;
                                let indent_px = (depth * 16) as i32;
                                let _ = idx;
                                rsx! {
                                    div {
                                        key: "{id}",
                                        class: "client-view-tree-row",
                                        style: "padding-left: {indent_px}px;",
                                        div { class: "client-view-row-primary", "{primary}" }
                                        if let Some(sec) = secondary {
                                            div { class: "client-view-row-secondary", "{sec}" }
                                        }
                                        if let Some(meta) = meta {
                                            div { class: "client-view-row-meta", "{meta}" }
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
