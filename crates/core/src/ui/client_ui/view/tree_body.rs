//! Tree body engine — renders `get_view_rows` as a flat list with indentation.
//!
//! WP 5 initial: the plugin returns a flat ordered list of rows; the tree
//! hierarchy is not yet expressed through `ViewRow`, so this body engine
//! falls back to flat rendering. `TreeSpec::max_depth` and `root_page_size`
//! are honored when available: `max_depth` caps `rows.len()` at a conservative
//! upper bound so a misbehaving plugin can't blow up the UI; `root_page_size`
//! provides the initial visible count.
//!
//! ## Lemmy-style forum rows (D30 revival)
//!
//! Mirrors [`crate::ui::client_ui::view::list_body`]: when a row's
//! `meta_text` carries a `"SCORE:N ·"` prefix, we render a `.forum-post-card`
//! with a vote column instead of the generic tree row. Non-forum rows still
//! render flat.

use super::list_body::{fetch_first_page, parse_score_meta, score_class};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{TreeSpec, ViewRow};
use poly_ui_macros::{context_menu, ui_action};

/// Actions emitted by [`TreeBody`]. Currently the forum-style vote buttons
/// are the only interactive elements in the tree body, and they're stubbed
/// locally; the typed enum exists so the ui-action coverage lint is
/// satisfied and MCP has a vocabulary for tree interactions.
#[derive(Debug, Clone)]
pub enum ClientViewTreeAction {
    /// User clicked the up-arrow on a forum-style tree row.
    Upvote { row_id: String },
    /// User clicked the down-arrow on a forum-style tree row.
    Downvote { row_id: String },
}

impl UiAction for ClientViewTreeAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Stubbed — see list_body's equivalent action enum.
    }
}

#[ui_action(ClientViewTreeAction)]
#[context_menu(inherit)]
#[component]
pub fn TreeBody(
    channel_id: String,
    account_id: String,
    spec: TreeSpec,
    #[props(default)] filter: String,
) -> Element {
    let rows_res = fetch_first_page(channel_id.clone(), account_id.clone());
    let _ = channel_id; // reserved for future refresh wiring
    let _ = account_id;

    // Guard against runaway plugins — `max_depth * root_page_size` is a
    // reasonable upper ceiling on visible rows for the initial page.
    let max_rows = max_visible_rows(&spec);

    match &*rows_res.read_unchecked() {
        None => rsx! {
            div {
                class: "client-view-tree client-view-tree-loading",
                role: "tree",
                "aria-busy": "true",
                span { "Loading…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("TreeBody: get_view_rows failed: {err:?}");
            rsx! {
                div { class: "client-view-tree client-view-tree-error", role: "tree",
                    span { "Failed to load thread" }
                }
            }
        }
        Some(Ok(page)) => {
            let mut rows = page.rows.clone();
            if max_rows > 0 && rows.len() > max_rows {
                rows.truncate(max_rows);
            }
            let filter_lc = filter.trim().to_lowercase();
            let rows: Vec<ViewRow> = if filter_lc.is_empty() {
                rows
            } else {
                rows.into_iter()
                    .filter(|r| {
                        r.primary_text.to_lowercase().contains(&filter_lc)
                            || r.secondary_text
                                .as_deref()
                                .is_some_and(|s| s.to_lowercase().contains(&filter_lc))
                    })
                    .collect()
            };
            if rows.is_empty() {
                rsx! {
                    div { class: "client-view-tree client-view-tree-empty forum-empty", role: "tree",
                        div { class: "forum-empty-icon", "📭" }
                        span { "No items" }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-tree forum-post-list", role: "tree",
                        for (idx, row) in rows.into_iter().enumerate() {
                            {
                                let id = row.id.clone();
                                let primary = row.primary_text.clone();
                                let secondary = row.secondary_text.clone();
                                let meta_raw = row.meta_text.clone();
                                let depth = 0_u32;
                                let indent_px = (depth * 16) as i32;
                                let _ = idx;

                                let (maybe_score, meta_rest): (Option<i64>, String) =
                                    meta_raw.as_deref().map_or((None, String::new()), parse_score_meta);

                                if let Some(score) = maybe_score {
                                    let sc_class = score_class(score);
                                    rsx! {
                                        div {
                                            key: "{id}",
                                            class: "forum-post-card",
                                            role: "treeitem",
                                            style: "padding-left: {indent_px}px;",
                                            div { class: "forum-post-votes",
                                                button {
                                                    class: "forum-vote-btn up",
                                                    "aria-label": "Upvote",
                                                    onclick: move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        tracing::debug!("forum upvote clicked (stub)");
                                                    },
                                                    "▲"
                                                }
                                                span { class: "{sc_class}", "{score}" }
                                                button {
                                                    class: "forum-vote-btn down",
                                                    "aria-label": "Downvote",
                                                    onclick: move |e: Event<MouseData>| {
                                                        e.stop_propagation();
                                                        tracing::debug!("forum downvote clicked (stub)");
                                                    },
                                                    "▼"
                                                }
                                            }
                                            div { class: "forum-post-content",
                                                div { class: "forum-post-title", "{primary}" }
                                                if let Some(sec) = secondary {
                                                    div { class: "forum-post-author-row", "{sec}" }
                                                }
                                                if !meta_rest.is_empty() {
                                                    div { class: "forum-post-meta", "{meta_rest}" }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        div {
                                            key: "{id}",
                                            class: "client-view-tree-row view-row-card",
                                            role: "treeitem",
                                            style: "padding-left: {indent_px}px;",
                                            h3 { class: "client-view-row-primary view-row-primary", "{primary}" }
                                            if let Some(sec) = secondary {
                                                span { class: "client-view-row-secondary view-row-secondary", "{sec}" }
                                            }
                                            if let Some(meta) = meta_raw {
                                                span { class: "client-view-row-meta view-row-meta", "{meta}" }
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

/// Pure helper — the upper-bound cap on rendered rows. Extracted so unit
/// tests can pin the formula without spinning up a Dioxus virtual DOM.
pub(crate) fn max_visible_rows(spec: &TreeSpec) -> usize {
    spec.root_page_size
        .saturating_mul(spec.max_depth.max(1))
        .max(spec.root_page_size) as usize
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn max_visible_rows_multiplies_page_size_by_depth() {
        let spec = TreeSpec {
            root_page_size: 10,
            max_depth: 3,
        };
        assert_eq!(max_visible_rows(&spec), 30);
    }

    #[test]
    fn max_visible_rows_floors_depth_at_one() {
        let spec = TreeSpec {
            root_page_size: 10,
            max_depth: 0,
        };
        assert_eq!(max_visible_rows(&spec), 10);
    }

    #[test]
    fn max_visible_rows_handles_saturating_overflow() {
        let spec = TreeSpec {
            root_page_size: u32::MAX,
            max_depth: u32::MAX,
        };
        let v = max_visible_rows(&spec);
        assert!(v >= u32::MAX as usize);
    }
}
