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

use super::list_body::{fetch_first_page, parse_score_meta, score_class, ViewRowDetail};
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{Cursor, TreeSpec, ViewRow};
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
    /// P4 — toolbar selection signals, same semantics as ListBody.
    #[props(default)]
    selected_sort: Signal<Option<String>>,
    #[props(default)] selected_filter: Signal<Option<String>>,
    #[props(default)] selected_tab: Signal<Option<String>>,
) -> Element {
    let sort_id = selected_sort.read().clone();
    let filter_id = selected_filter.read().clone();
    let tab_id = selected_tab.read().clone();
    let rows_res = fetch_first_page(
        channel_id.clone(),
        account_id.clone(),
        sort_id.clone(),
        filter_id.clone(),
        tab_id.clone(),
    );

    // P3 (TreeBody) — selected row id for inline detail rendering.
    let selected_row_id = use_signal(|| None::<String>);
    // P5 — infinite scroll state (mirrors ListBody).
    let mut loaded_rows = use_signal(Vec::<ViewRow>::new);
    let mut next_cursor = use_signal(|| None::<Cursor>);
    let mut loaded_first_page_key = use_signal(String::new);
    let mut loading_more = use_signal(|| false);
    let first_page_key = format!(
        "{}:{}:{:?}:{:?}:{:?}",
        channel_id, account_id, sort_id, filter_id, tab_id
    );

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
            // P5 — reset accumulator when first-page key changes.
            if *loaded_first_page_key.read() != first_page_key {
                loaded_first_page_key.set(first_page_key.clone());
                loaded_rows.set(page.rows.clone());
                next_cursor.set(page.next_cursor.clone());
            }
            let mut rows = loaded_rows.read().clone();
            if max_rows > 0 && rows.len() > max_rows {
                rows.truncate(max_rows);
            }
            let has_more = next_cursor.read().is_some();
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
                // selected_row_id retained for future inline-preview; unused now.
                let _unused_selected = selected_row_id.read().clone();
                // Pull route params — forum post click routes to full-page
                // ForumPostView (matches the old LemmyForumView behavior).
                let app_state: Signal<crate::state::AppState> = use_context();
                let snap = app_state.read();
                let backend_slug_for_click = snap
                    .nav
                    .active_backend
                    .cloned()
                    .map(|b| b.slug().to_string())
                    .unwrap_or_default();
                let instance_id_for_click = snap.nav.active_instance_id.cloned().unwrap_or_default();
                let server_id_for_click = snap.nav.selected_server.cloned().unwrap_or_default();
                let channel_id_for_click = snap
                    .nav
                    .selected_channel
                    .cloned()
                    .unwrap_or_else(|| channel_id.clone());
                let account_id_for_click = account_id.clone();
                drop(snap);
                let nav = navigator();
                let on_row_click = EventHandler::new(move |row_id: String| {
                    tracing::info!("forum tree card clicked id={row_id} — routing to ForumPostRoute");
                    nav.push(crate::ui::routes::Route::ForumPostRoute {
                        backend: backend_slug_for_click.clone(),
                        instance_id: instance_id_for_click.clone(),
                        account_id: account_id_for_click.clone(),
                        server_id: server_id_for_click.clone(),
                        channel_id: channel_id_for_click.clone(),
                        post_id: row_id,
                    });
                });
                let channel_id_for_more = channel_id.clone();
                let account_id_for_more = account_id.clone();
                let sort_id_for_more = sort_id.clone();
                let filter_id_for_more = filter_id.clone();
                let tab_id_for_more = tab_id.clone();
                let is_loading_more = *loading_more.read();
                rsx! {
                    div { class: "client-view-tree forum-post-list", role: "tree",
                        for row in rows {
                            TreeBodyRow {
                                key: "{row.id}",
                                row: row.clone(),
                                on_click: on_row_click,
                            }
                        }
                        if has_more {
                            button {
                                class: "forum-load-more",
                                r#type: "button",
                                disabled: is_loading_more,
                                onclick: move |_| {
                                    let channel_id = channel_id_for_more.clone();
                                    let account_id = account_id_for_more.clone();
                                    let sort_id = sort_id_for_more.clone();
                                    let filter_id = filter_id_for_more.clone();
                                    let tab_id = tab_id_for_more.clone();
                                    let cursor = next_cursor.read().clone();
                                    loading_more.set(true);
                                    spawn(async move {
                                        let client_manager: Signal<ClientManager> = match try_consume_context() {
                                            Some(cm) => cm,
                                            None => {
                                                loading_more.set(false);
                                                return;
                                            }
                                        };
                                        let backend = match client_manager.read().get_backend(&account_id) {
                                            Some(b) => b,
                                            None => {
                                                loading_more.set(false);
                                                return;
                                            }
                                        };
                                        let guard = match backend
                                            .read_with_timeout(std::time::Duration::from_secs(5))
                                            .await
                                        {
                                            Ok(g) => g,
                                            Err(_) => {
                                                tracing::warn!("TreeBody load-more: backend read timed out");
                                                loading_more.set(false);
                                                return;
                                            }
                                        };
                                        let result = guard.get_view_rows(
                                            &channel_id,
                                            cursor,
                                            sort_id.as_deref(),
                                            filter_id.as_deref(),
                                            tab_id.as_deref(),
                                        ).await;
                                        drop(guard);
                                        match result {
                                            Ok(page) => {
                                                loaded_rows.write().extend(page.rows);
                                                next_cursor.set(page.next_cursor);
                                            }
                                            Err(err) => {
                                                tracing::debug!("TreeBody load-more failed: {err:?}");
                                            }
                                        }
                                        loading_more.set(false);
                                    });
                                },
                                if is_loading_more { "Loading…" } else { "Load more" }
                            }
                        }
                        // Inline ViewRowDetail removed — route-push to
                        // ForumPostRoute matches old LemmyForumView.
                    }
                }
            }
        }
    }
}

/// Single row of the tree body. Extracted as a helper component so Dioxus
/// 0.7 template tracking keeps `onclick` handlers bound across renders
/// (see the equivalent `ListBodyRow` docstring in `list_body.rs`).
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn TreeBodyRow(row: ViewRow, on_click: EventHandler<String>) -> Element {
    let id = row.id.clone();
    let id_for_click = id.clone();
    let primary = row.primary_text.clone();
    let secondary = row.secondary_text.clone();
    let meta_raw = row.meta_text.clone();
    let depth = 0_u32;
    let indent_px = (depth * 16) as i32;

    let (maybe_score, meta_rest): (Option<i64>, String) = meta_raw
        .as_deref()
        .map_or((None, String::new()), parse_score_meta);

    if let Some(score) = maybe_score {
        let sc_class = score_class(score);
        rsx! {
            div {
                class: "forum-post-card",
                role: "treeitem",
                style: "padding-left: {indent_px}px;",
                onclick: move |_| on_click.call(id_for_click.clone()),
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
                class: "client-view-tree-row view-row-card",
                role: "treeitem",
                style: "padding-left: {indent_px}px;",
                onclick: move |_| on_click.call(id_for_click.clone()),
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
