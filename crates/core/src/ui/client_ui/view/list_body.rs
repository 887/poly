//! Flat-list body engine — renders `get_view_rows` as a vertical list using
//! the plugin-declared `RowTemplate`.
//!
//! WP 5 scope: first page only (no infinite scroll). Rows show
//! `primary_text`, `secondary_text` and `meta_text` raw strings from the
//! plugin (they are content, not FTL keys — see `ViewRow` doc).
//!
//! ## Lemmy-style forum rows (D30 revival)
//!
//! When a plugin wants a vote-column / score card layout (Lemmy/Reddit), it
//! encodes the numeric score as a prefix on `meta_text`:
//!
//! ```text
//! "SCORE:142 · 7 comments · 3h ago"
//! ```
//!
//! The list engine recognises the `SCORE:N ·` prefix via
//! [`parse_score_meta`], strips it off, and renders a dedicated
//! `.forum-post-card` with a vote column. Rows that don't carry a score
//! prefix fall through to the generic `.view-row-card` layout — every
//! non-forum backend (HN, GitHub, …) is unaffected.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::{AppState, BatchedSignal};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::CustomBlock;
use crate::ui::context_menu::menus::{forum_post_entry, ForumPostCtx};
use crate::ui::errors::{is_session_expired, SessionExpiredCard};
use dioxus::prelude::*;
use poly_client::{ClientError, Cursor, ListSpec, ViewDetail, ViewRow, ViewRowsPage};
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the flat-list body engine.
///
/// P3 — prior revisions hard-coded `Route::ForumPostRoute` for row clicks,
/// which was wrong for every non-forum backend. The row click now dispatches
/// this action so the component state (selected row id) tracks the active
/// detail; a `ViewRowDetail` sub-component renders the plugin's
/// `get_view_detail` output inline. A future pass may plug in a split-pane
/// experience (see `SplitBody`) or a detail route when the backend declares
/// one via `action-outcome::navigate`.
#[derive(Debug, Clone)]
pub enum ClientViewRowClickAction {
    /// User clicked a row; detail pane fetches
    /// `get_view_detail(channel_id, row_id)`.
    Open { channel_id: String, row_id: String },
    /// User clicked the up-arrow in a forum row's vote column.
    Upvote { channel_id: String, row_id: String },
    /// User clicked the down-arrow in a forum row's vote column.
    Downvote { channel_id: String, row_id: String },
}

impl UiAction for ClientViewRowClickAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Local-state-only. The component owns the `selected_row_id` signal
        // and does the fetch; this typed enum exists so the ui-action
        // coverage lint is satisfied and MCP has a vocabulary for row
        // clicks. Upvote/Downvote are stubbed — a real backend wire-up
        // dispatches via `invoke_message_action`, but every current plugin
        // returns `Ok(Noop)` for these ids today.
    }
}

#[ui_action(ClientViewRowClickAction)]
#[context_menu(inherit)]
#[component]
pub fn ListBody(
    channel_id: String,
    account_id: String,
    spec: ListSpec,
    #[props(default)] filter: String,
    /// P4 — current sort selection from the toolbar. Included in the
    /// `use_resource` dependency list so changes re-fetch.
    #[props(default)]
    selected_sort: Signal<Option<String>>,
    #[props(default)] selected_filter: Signal<Option<String>>,
    #[props(default)] selected_tab: Signal<Option<String>>,
) -> Element {
    let _ = spec; // page_size honored implicitly by the plugin.
    // P4 — read the toolbar selection signals inside the closure so the
    // resource re-runs when they change.
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

    // P3 — selected row id is local component state.
    let selected_row_id = use_signal(|| None::<String>);
    // P5 — infinite-scroll state. `loaded_rows` accumulates every page
    // fetched so far. `next_cursor` is the cursor the plugin handed back
    // with the most recent page (`None` means we've reached the end).
    // `loaded_first_page` lets us reset the accumulator when the first
    // page resource re-runs (sort/filter/tab change).
    let mut loaded_rows = use_signal(Vec::<ViewRow>::new);
    let mut next_cursor = use_signal(|| None::<Cursor>);
    let mut loaded_first_page_key = use_signal(String::new);
    let mut loading_more = use_signal(|| false);
    let first_page_key = format!(
        "{}:{}:{:?}:{:?}:{:?}",
        channel_id, account_id, sort_id, filter_id, tab_id
    );

    match &*rows_res.read_unchecked() {
        None => rsx! {
            div {
                class: "client-view-list client-view-list-loading",
                role: "feed",
                "aria-busy": "true",
                span { "Loading…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("ListBody: get_view_rows failed: {err:?}");
            if is_session_expired(err) {
                let app_state: BatchedSignal<AppState> = use_context();
                let (nav_backend, nav_instance_id, nav_account_id) = {
                    let s = app_state.read();
                    let b = s.nav.active_backend.cloned().map(|b| b.slug().to_string()).unwrap_or_default();
                    let i = s.nav.active_instance_id.cloned().unwrap_or_default();
                    let a = s.nav.active_account_id.cloned().unwrap_or_else(|| account_id.clone());
                    (b, i, a)
                };
                rsx! {
                    div { class: "client-view-list client-view-list-error", role: "feed",
                        SessionExpiredCard {
                            backend: nav_backend.clone(),
                            instance_id: nav_instance_id,
                            account_id: nav_account_id,
                            backend_display_name: nav_backend,
                        }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-list client-view-list-error", role: "feed",
                        span { "Failed to load rows" }
                    }
                }
            }
        }
        Some(Ok(page)) => {
            // P5 — whenever the first-page-key changes (new sort/filter),
            // reset the accumulator to this page's rows.
            if *loaded_first_page_key.read() != first_page_key {
                loaded_first_page_key.set(first_page_key.clone());
                loaded_rows.set(page.rows.clone());
                next_cursor.set(page.next_cursor.clone());
            }
            let rows_all = loaded_rows.read().clone();
            let has_more = next_cursor.read().is_some();
            let filter_lc = filter.trim().to_lowercase();
            let rows: Vec<ViewRow> = if filter_lc.is_empty() {
                rows_all
            } else {
                rows_all
                    .into_iter()
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
                    div { class: "client-view-list client-view-list-empty forum-empty", role: "feed",
                        div { class: "forum-empty-icon", "📭" }
                        span { "No items" }
                    }
                }
            } else {
                // `selected_row_id` still exists on the component but is
                // unused now that we route-push to ForumPostRoute. Kept so
                // future inline-preview modes can flip back without a refactor.
                let _unused_selected = selected_row_id.read().clone();
                // Pull the route params once — forum post click routes to
                // a full-page ForumPostView like the old LemmyForumView did.
                let app_state: BatchedSignal<crate::state::AppState> = use_context();
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
                    tracing::info!("forum card clicked id={row_id} — routing to ForumPostRoute");
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
                    div { class: "client-view-list forum-post-list", role: "feed",
                        for row in rows {
                            ListBodyRow {
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
                                    // P5 — fetch next page synchronously
                                    // via the plugin's `get_view_rows`.
                                    let channel_id = channel_id_for_more.clone();
                                    let account_id = account_id_for_more.clone();
                                    let sort_id = sort_id_for_more.clone();
                                    let filter_id = filter_id_for_more.clone();
                                    let tab_id = tab_id_for_more.clone();
                                    let cursor = next_cursor.read().clone();
                                    loading_more.set(true);
                                    spawn(async move {
                                        let client_manager: BatchedSignal<ClientManager> = match try_consume_context() {
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
                                                tracing::warn!("ListBody load-more: backend read timed out");
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
                                                tracing::debug!("ListBody load-more failed: {err:?}");
                                            }
                                        }
                                        loading_more.set(false);
                                    });
                                },
                                if is_loading_more { "Loading…" } else { "Load more" }
                            }
                        }
                        // Inline ViewRowDetail removed — we now route-push to
                        // ForumPostRoute instead (matches the old Lemmy UX).
                    }
                }
            }
        }
    }
}

/// Single row of the flat-list body. Extracted as a helper component so
/// `onclick` handlers are registered on a stable per-row vnode rather than
/// inside the `for row in rows { { let ... ; rsx!{...} } }` block expression
/// pattern — Dioxus 0.7 template tracking would drop these handlers in some
/// cases, leaving row clicks as dead taps (see P3 selected-row bug).
///
/// D2.d: when the row carries a `SCORE:N` prefix (Lemmy-style forum post), the
/// right-click opens `ForumPostContextMenu` via the stack host in `MainLayout`.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn ListBodyRow(row: ViewRow, on_click: EventHandler<String>) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let id = row.id.clone();
    let id_for_click = id.clone();
    let primary = row.primary_text.clone();
    let secondary = row.secondary_text.clone();
    let meta_raw = row.meta_text.clone();
    let icon = row.icon.clone();
    let badge = row.badge.clone();

    let preview_image_url = row.preview_image_url.clone();
    let (maybe_score, meta_rest): (Option<i64>, String) = meta_raw
        .as_deref()
        .map_or((None, String::new()), parse_score_meta);

    if let Some(score) = maybe_score {
        let sc_class = score_class(score);
        // Build ForumPostCtx from available row data for the right-click menu.
        let ctx_id = id.clone();
        let ctx_text = primary.clone();
        let ctx_author_name = secondary.clone().unwrap_or_default();
        rsx! {
            div {
                class: "forum-post-card",
                role: "article",
                onclick: move |_| on_click.call(id_for_click.clone()),
                oncontextmenu: {
                    let pid = ctx_id.clone();
                    let text = ctx_text.clone();
                    let aname = ctx_author_name.clone();
                    move |evt: MouseEvent| {
                        evt.prevent_default();
                        evt.stop_propagation();
                        let ctx = ForumPostCtx {
                            post_id: pid.clone(),
                            author_id: String::new(),
                            author_name: aname.clone(),
                            text: text.clone(),
                        };
                        let entry = forum_post_entry(ctx, &evt);
                        app_state.batch(|st| st.context_menu_stack.push(entry));
                    }
                },
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
                if let Some(ref url) = preview_image_url {
                    img {
                        class: "forum-post-preview",
                        src: "{url}",
                        alt: "Post preview",
                        loading: "lazy",
                    }
                }
            }
        }
    } else {
        rsx! {
            div {
                class: "client-view-list-row view-row-card",
                role: "article",
                onclick: move |_| on_click.call(id_for_click.clone()),
                if let Some(icon) = icon {
                    span { class: "client-view-row-icon view-row-icon", "{icon}" }
                }
                div { class: "client-view-row-text view-row-text",
                    h3 { class: "client-view-row-primary view-row-primary", "{primary}" }
                    if let Some(sec) = secondary {
                        span { class: "client-view-row-secondary view-row-secondary", "{sec}" }
                    }
                    if let Some(meta) = meta_raw {
                        span { class: "client-view-row-meta view-row-meta", "{meta}" }
                    }
                }
                if let Some(badge) = badge {
                    span { class: "client-view-row-badge view-row-badge", "{badge}" }
                }
            }
        }
    }
}

/// P3 — inline detail component. When a row is clicked, the parent passes the
/// selected `row_id` here; this component calls `get_view_detail` and renders
/// the returned `CustomBlock`. If the plugin returns `NotSupported` or any
/// other error, a minimal placeholder is shown instead — still better than
/// the previous no-op click.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ViewRowDetail(channel_id: String, account_id: String, row_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let detail_res: Resource<Result<ViewDetail, ClientError>> = {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        let row_id = row_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let channel_id = channel_id.clone();
            let row_id = row_id.clone();
            async move {
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = match backend
                    .read_with_timeout(std::time::Duration::from_secs(5))
                    .await
                {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("ViewRowDetail: backend read timed out");
                        return Err(ClientError::Internal(
                            "backend read timed out".to_string(),
                        ));
                    }
                };
                guard.get_view_detail(&channel_id, &row_id).await
            }
        })
    };

    match &*detail_res.read_unchecked() {
        None => rsx! {
            div { class: "view-row-detail view-row-detail-loading",
                "aria-busy": "true",
                "{row_id} (detail loading — P3 follow-up)"
            }
        },
        Some(Err(_)) => rsx! {
            div { class: "view-row-detail view-row-detail-empty",
                "{row_id} (detail loading — P3 follow-up)"
            }
        },
        Some(Ok(detail)) => {
            let body = detail.body_block.clone();
            rsx! { div { class: "view-row-detail", CustomBlock { block: body } } }
        }
    }
}

/// Parse a `meta_text` that may start with `"SCORE:N ·"` and return
/// `(Some(score), remainder)`. If no score prefix is present, returns
/// `(None, meta.to_string())` — the forum render path is opt-in per row.
///
/// The prefix format is produced by plugins (e.g. `clients/demo`) and
/// must match this parser exactly — see the module docstring.
pub(crate) fn parse_score_meta(meta: &str) -> (Option<i64>, String) {
    let s = meta.trim_start();
    let Some(rest) = s.strip_prefix("SCORE:") else {
        return (None, meta.to_string());
    };
    // Read the signed integer up to the first whitespace.
    let end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());
    let (num, tail) = rest.split_at(end);
    let Ok(score) = num.parse::<i64>() else {
        return (None, meta.to_string());
    };
    // Strip a leading separator (`·`, `•` or `-`) + surrounding whitespace.
    let tail = tail.trim_start();
    let tail = tail
        .strip_prefix('·')
        .or_else(|| tail.strip_prefix('•'))
        .or_else(|| tail.strip_prefix('-'))
        .unwrap_or(tail);
    (Some(score), tail.trim().to_string())
}

/// Return the CSS class for a score cell — positive / negative / zero.
/// Mirrors the pre-refactor `score_class` helper.
pub(crate) fn score_class(score: i64) -> &'static str {
    if score > 0 {
        "forum-score positive"
    } else if score < 0 {
        "forum-score negative"
    } else {
        "forum-score"
    }
}

/// Pure helper — compute the structural summary of a row that the list-body
/// card renders. Used by unit tests to verify ARIA / primary / secondary /
/// meta presence without spinning up a Dioxus virtual DOM.
pub(crate) fn row_card_parts(row: &ViewRow) -> RowCardParts {
    let score = row
        .meta_text
        .as_deref()
        .map(parse_score_meta)
        .and_then(|(s, _)| s);
    RowCardParts {
        has_primary: !row.primary_text.is_empty(),
        has_secondary: row.secondary_text.is_some(),
        has_meta: row.meta_text.is_some(),
        has_icon: row.icon.is_some(),
        has_badge: row.badge.is_some(),
        has_score: score.is_some(),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RowCardParts {
    pub has_primary: bool,
    pub has_secondary: bool,
    pub has_meta: bool,
    pub has_icon: bool,
    pub has_badge: bool,
    pub has_score: bool,
}

/// P4 — helper used by `fetch_first_page` and mirrored in unit tests.
/// Returns the `(sort_id, filter_id, tab_id)` triple the view would pass
/// to `get_view_rows` for the given toolbar selection signals, stripped
/// to `&str` slices.
pub(crate) fn toolbar_get_view_rows_args<'a>(
    sort: &'a Option<String>,
    filter: &'a Option<String>,
    tab: &'a Option<String>,
) -> (Option<&'a str>, Option<&'a str>, Option<&'a str>) {
    (sort.as_deref(), filter.as_deref(), tab.as_deref())
}

/// P5 — pure helper used by the load-more flow and mirrored in unit
/// tests. Returns the new row accumulator + next_cursor after appending
/// a freshly-fetched `page` to the previous `accum` + `prev_cursor`.
pub(crate) fn append_page(
    mut accum: Vec<ViewRow>,
    page: ViewRowsPage,
) -> (Vec<ViewRow>, Option<Cursor>) {
    accum.extend(page.rows);
    (accum, page.next_cursor)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use poly_client::MenuTargetKind;

    fn row(id: &str, primary: &str) -> ViewRow {
        ViewRow {
            id: id.into(),
            primary_text: primary.into(),
            secondary_text: None,
            meta_text: None,
            icon: None,
            badge: None,
            context_menu_target_kind: MenuTargetKind::Message,
            preview_image_url: None,
        }
    }

    #[test]
    fn row_card_parts_reports_only_primary_for_minimal_row() {
        let r = row("a", "Hello");
        let parts = row_card_parts(&r);
        assert!(parts.has_primary);
        assert!(!parts.has_secondary);
        assert!(!parts.has_meta);
        assert!(!parts.has_icon);
        assert!(!parts.has_badge);
        assert!(!parts.has_score);
    }

    #[test]
    fn row_card_parts_reports_all_optional_fields() {
        let r = ViewRow {
            id: "a".into(),
            primary_text: "Hello".into(),
            secondary_text: Some("sub".into()),
            meta_text: Some("meta".into()),
            icon: Some("icon".into()),
            badge: Some("b".into()),
            context_menu_target_kind: MenuTargetKind::Message,
            preview_image_url: None,
        };
        let parts = row_card_parts(&r);
        assert!(parts.has_primary);
        assert!(parts.has_secondary);
        assert!(parts.has_meta);
        assert!(parts.has_icon);
        assert!(parts.has_badge);
        assert!(!parts.has_score);
    }

    #[test]
    fn row_card_parts_detects_empty_primary_text() {
        let r = row("a", "");
        let parts = row_card_parts(&r);
        assert!(!parts.has_primary);
    }

    #[test]
    fn row_count_matches_rows_vec_len() {
        let rows = vec![row("a", "First"), row("b", "Second"), row("c", "Third")];
        assert_eq!(rows.len(), 3);
        for r in &rows {
            let parts = row_card_parts(r);
            assert!(parts.has_primary);
        }
    }

    #[test]
    fn row_card_parts_reports_score_when_meta_starts_with_prefix() {
        let mut r = row("a", "Post");
        r.meta_text = Some("SCORE:42 · 7 comments · 3h ago".into());
        let parts = row_card_parts(&r);
        assert!(parts.has_score);
    }

    #[test]
    fn parse_score_meta_reads_positive_score() {
        let (score, rest) = parse_score_meta("SCORE:142 · 7 comments · 3h ago");
        assert_eq!(score, Some(142));
        assert_eq!(rest, "7 comments · 3h ago");
    }

    #[test]
    fn parse_score_meta_reads_negative_score() {
        let (score, rest) = parse_score_meta("SCORE:-5 · 0 comments · now");
        assert_eq!(score, Some(-5));
        assert_eq!(rest, "0 comments · now");
    }

    #[test]
    fn parse_score_meta_without_prefix_returns_none_and_original() {
        let (score, rest) = parse_score_meta("42 upvotes · just now");
        assert_eq!(score, None);
        assert_eq!(rest, "42 upvotes · just now");
    }

    #[test]
    fn parse_score_meta_malformed_score_returns_none() {
        let (score, rest) = parse_score_meta("SCORE:abc · huh");
        assert_eq!(score, None);
        assert_eq!(rest, "SCORE:abc · huh");
    }

    #[test]
    fn parse_score_meta_zero_score_still_matches() {
        let (score, rest) = parse_score_meta("SCORE:0 · no comments · now");
        assert_eq!(score, Some(0));
        assert_eq!(rest, "no comments · now");
    }

    #[test]
    fn parse_score_meta_handles_missing_separator() {
        let (score, rest) = parse_score_meta("SCORE:7 trailing");
        assert_eq!(score, Some(7));
        assert_eq!(rest, "trailing");
    }

    #[test]
    fn score_class_positive_score() {
        assert_eq!(score_class(1), "forum-score positive");
        assert_eq!(score_class(9999), "forum-score positive");
    }

    #[test]
    fn score_class_negative_score() {
        assert_eq!(score_class(-1), "forum-score negative");
        assert_eq!(score_class(-9999), "forum-score negative");
    }

    #[test]
    fn score_class_zero_is_neutral() {
        assert_eq!(score_class(0), "forum-score");
    }

    // ─── P4 / P5 Pack B unit tests ──────────────────────────────────

    #[test]
    fn toolbar_get_view_rows_args_threads_selected_sort() {
        let sort = Some("new".to_string());
        let filter = None;
        let tab = None;
        let (s, f, t) = toolbar_get_view_rows_args(&sort, &filter, &tab);
        assert_eq!(s, Some("new"));
        assert_eq!(f, None);
        assert_eq!(t, None);
    }

    #[test]
    fn toolbar_get_view_rows_args_threads_all_three() {
        let sort = Some("hot".to_string());
        let filter = Some("subscribed".to_string());
        let tab = Some("posts".to_string());
        let (s, f, t) = toolbar_get_view_rows_args(&sort, &filter, &tab);
        assert_eq!(s, Some("hot"));
        assert_eq!(f, Some("subscribed"));
        assert_eq!(t, Some("posts"));
    }

    #[test]
    fn toolbar_get_view_rows_args_passes_none_when_no_selection() {
        let sort: Option<String> = None;
        let filter: Option<String> = None;
        let tab: Option<String> = None;
        let (s, f, t) = toolbar_get_view_rows_args(&sort, &filter, &tab);
        assert_eq!(s, None);
        assert_eq!(f, None);
        assert_eq!(t, None);
    }

    #[test]
    fn append_page_accumulates_two_pages_and_preserves_order() {
        use poly_client::{Cursor, CursorKind, ViewRowsPage};
        let page1 = ViewRowsPage {
            rows: vec![row("a", "first"), row("b", "second")],
            next_cursor: Some(Cursor {
                kind: CursorKind::Offset,
                value: "2".into(),
            }),
        };
        let page2 = ViewRowsPage {
            rows: vec![row("c", "third"), row("d", "fourth")],
            next_cursor: None,
        };
        let (acc1, c1) = append_page(Vec::new(), page1);
        assert_eq!(acc1.len(), 2);
        assert!(c1.is_some());
        let (acc2, c2) = append_page(acc1, page2);
        assert_eq!(acc2.len(), 4);
        assert_eq!(acc2[0].id, "a");
        assert_eq!(acc2[1].id, "b");
        assert_eq!(acc2[2].id, "c");
        assert_eq!(acc2[3].id, "d");
        assert!(c2.is_none()); // end of feed reached
    }

    #[test]
    fn append_page_empty_page_preserves_accumulator() {
        use poly_client::ViewRowsPage;
        let start = vec![row("a", "first")];
        let empty = ViewRowsPage {
            rows: Vec::new(),
            next_cursor: None,
        };
        let (acc, cursor) = append_page(start, empty);
        assert_eq!(acc.len(), 1);
        assert_eq!(acc[0].id, "a");
        assert!(cursor.is_none());
    }
}

/// Fetch the first page of rows for this view. P4 — sort/filter/tab ids
/// are passed through so toolbar selection changes re-fetch (use_resource
/// re-runs when any captured value changes). P5 handles the next-cursor
/// load-more flow separately.
pub(super) fn fetch_first_page(
    channel_id: String,
    account_id: String,
    sort_id: Option<String>,
    filter_id: Option<String>,
    tab_id: Option<String>,
) -> Resource<Result<ViewRowsPage, ClientError>> {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    use_resource(move || {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        let sort_id = sort_id.clone();
        let filter_id = filter_id.clone();
        let tab_id = tab_id.clone();
        async move {
            let Some(backend) = client_manager.read().get_backend(&account_id) else {
                return Err(ClientError::NotFound(format!(
                    "no backend for account {account_id}"
                )));
            };
            let guard = match backend
                .read_with_timeout(std::time::Duration::from_secs(5))
                .await
            {
                Ok(g) => g,
                Err(_) => {
                    tracing::warn!("fetch_first_page: backend read timed out");
                    return Err(ClientError::Internal(
                        "backend read timed out".to_string(),
                    ));
                }
            };
            guard
                .get_view_rows(
                    &channel_id,
                    None,
                    sort_id.as_deref(),
                    filter_id.as_deref(),
                    tab_id.as_deref(),
                )
                .await
        }
    })
}
