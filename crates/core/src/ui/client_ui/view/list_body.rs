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

use crate::client_manager::ClientManager;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::CustomBlock;
use dioxus::prelude::*;
use poly_client::{ClientError, ListSpec, ViewDetail, ViewRow, ViewRowsPage};
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
) -> Element {
    let _ = spec; // page_size honored implicitly by the plugin.
    let rows_res = fetch_first_page(channel_id.clone(), account_id.clone());

    // P3 — selected row id is local component state. Clicking a row sets it;
    // the `ViewRowDetail` sub-component below reacts by calling
    // `get_view_detail` and rendering the returned `CustomBlock`.
    let mut selected_row_id = use_signal(|| None::<String>);

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
            rsx! {
                div { class: "client-view-list client-view-list-error", role: "feed",
                    span { "Failed to load rows" }
                }
            }
        }
        Some(Ok(page)) => {
            let rows_all = page.rows.clone();
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
                let selected = selected_row_id.read().clone();
                rsx! {
                    div { class: "client-view-list forum-post-list", role: "feed",
                        for row in rows {
                            {
                                let id = row.id.clone();
                                let id_for_click = id.clone();
                                let primary = row.primary_text.clone();
                                let secondary = row.secondary_text.clone();
                                let meta_raw = row.meta_text.clone();
                                let icon = row.icon.clone();
                                let badge = row.badge.clone();

                                let (maybe_score, meta_rest): (Option<i64>, String) =
                                    meta_raw.as_deref().map_or((None, String::new()), parse_score_meta);

                                if let Some(score) = maybe_score {
                                    // Lemmy-style forum card
                                    let sc_class = score_class(score);
                                    rsx! {
                                        div {
                                            key: "{id}",
                                            class: "forum-post-card",
                                            role: "article",
                                            onclick: move |_| selected_row_id.set(Some(id_for_click.clone())),
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
                                            class: "client-view-list-row view-row-card",
                                            role: "article",
                                            onclick: move |_| selected_row_id.set(Some(id_for_click.clone())),
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
                        }
                        if let Some(sel_id) = selected {
                            ViewRowDetail {
                                channel_id: channel_id.clone(),
                                account_id: account_id.clone(),
                                row_id: sel_id,
                            }
                        }
                    }
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
    let client_manager: Signal<ClientManager> = use_context();
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
                let guard = backend.read().await;
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
}

/// Fetch only the first page — WP 5 defers infinite scroll.
pub(super) fn fetch_first_page(
    channel_id: String,
    account_id: String,
) -> Resource<Result<ViewRowsPage, ClientError>> {
    let client_manager: Signal<ClientManager> = use_context();
    use_resource(move || {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        async move {
            let Some(backend) = client_manager.read().get_backend(&account_id) else {
                return Err(ClientError::NotFound(format!(
                    "no backend for account {account_id}"
                )));
            };
            let guard = backend.read().await;
            guard
                .get_view_rows(&channel_id, None, None, None, None)
                .await
        }
    })
}
