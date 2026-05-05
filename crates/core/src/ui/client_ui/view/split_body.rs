//! Split body engine — list on the left, detail on the right. Clicking a
//! row fetches `get_view_detail` for the selected row id.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::state::{AppState, BatchedSignal};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::CustomBlock;
use crate::ui::client_ui::use_view_resource::{use_view_resource, ViewQuery};
use crate::ui::errors::{is_session_expired, SessionExpiredCard};
use dioxus::prelude::*;
use poly_client::{ClientBackend, ClientError, ClientResult, SplitSpec, ViewDetail, ViewRowsPage};
use poly_ui_macros::{context_menu, ui_action};

// ── ViewQuery impls for this module ──────────────────────────────────────────

/// Query: fetch the first page of rows for a split-body view.
#[derive(Clone, PartialEq)]
struct SplitBodyRowsQuery {
    account_id: String,
    channel_id: String,
}

impl ViewQuery for SplitBodyRowsQuery {
    type Output = ViewRowsPage;
    fn account_id(&self) -> &str { &self.account_id }
    async fn fetch(&self, b: &dyn ClientBackend) -> ClientResult<Self::Output> {
        b.get_view_rows(&self.channel_id, None, None, None, None).await
    }
}

/// Actions for the split body engine — currently only row-selection.
#[derive(Debug, Clone)]
pub enum ClientViewSplitAction {
    /// User clicked a row in the master list; detail pane fetches its
    /// content via `get_view_detail(channel_id, row_id)`.
    SelectRow(String),
}

impl UiAction for ClientViewSplitAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Local-selection state is managed via a `Signal<Option<String>>`
        // inside the component; the typed enum exists to satisfy the
        // action-coverage lint and to give MCP/testing a vocabulary.
    }
}

#[ui_action(ClientViewSplitAction)]
#[context_menu(inherit)]
#[component]
pub fn SplitBody(channel_id: String, account_id: String, spec: SplitSpec) -> Element {
    let _ = spec;
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav_sig: BatchedSignal<crate::state::NavState> = use_context();
    let (nav_backend, nav_instance_id) = {
        let s = nav_sig.read();
        let b = s.active_backend.cloned().map(|b| b.slug().to_string()).unwrap_or_default();
        let i = s.active_instance_id.cloned().unwrap_or_default();
        (b, i)
    };

    let rows_res: Resource<ClientResult<ViewRowsPage>> = use_view_resource(SplitBodyRowsQuery {
        account_id: account_id.clone(),
        channel_id: channel_id.clone(),
    });

    let mut selected_id = use_signal(|| None::<String>);

    let detail_res: Resource<Result<ViewDetail, ClientError>> = {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            let channel_id = channel_id.clone();
            let sel = selected_id.read().clone();
            async move {
                let Some(row_id) = sel else {
                    return Err(ClientError::NotFound("no row selected".into()));
                };
                client_manager.peek().with_backend(&account_id, async |b| {
                    b.get_view_detail(&channel_id, &row_id).await
                }).await
            }
        })
    };

    let selected = selected_id.read().clone();

    rsx! {
        div { class: "client-view-split",
            div { class: "client-view-split-list",
                match &*rows_res.read_unchecked() {
                    None => rsx! { div { class: "client-view-split-loading", "Loading…" } },
                    Some(Err(err)) => {
                        tracing::debug!("SplitBody: get_view_rows failed: {err:?}");
                        if is_session_expired(err) {
                            rsx! {
                                SessionExpiredCard {
                                    backend: nav_backend.clone(),
                                    instance_id: nav_instance_id.clone(),
                                    account_id: account_id.clone(),
                                    backend_display_name: nav_backend.clone(),
                                }
                            }
                        } else {
                            rsx! { div { class: "client-view-split-error", "Failed to load rows" } }
                        }
                    }
                    Some(Ok(page)) => {
                        let rows = page.rows.clone();
                        if rows.is_empty() {
                            // F-GH-2: styled empty state with icon and helpful messaging
                            rsx! {
                                div { class: "client-view-split-empty",
                                    span { class: "split-empty-icon", "📭" }
                                    p { class: "split-empty-title", "Nothing here" }
                                    p { class: "split-empty-hint",
                                        "No open issues or pull requests found."
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                for row in rows {
                                    {
                                        let id = row.id.clone();
                                        let id_for_click = id.clone();
                                        let primary = row.primary_text.clone();
                                        let secondary = row.secondary_text.clone();
                                        let is_sel = selected.as_deref() == Some(id.as_str());
                                        let cls = if is_sel {
                                            "client-view-split-row selected"
                                        } else {
                                            "client-view-split-row"
                                        };
                                        rsx! {
                                            div {
                                                key: "{id}",
                                                class: "{cls}",
                                                onclick: move |_| selected_id.set(Some(id_for_click.clone())),
                                                div { class: "client-view-row-primary", "{primary}" }
                                                if let Some(sec) = secondary {
                                                    div { class: "client-view-row-secondary", "{sec}" }
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
            div { class: "client-view-split-detail",
                if selected_id.read().is_none() {
                    div { class: "client-view-split-placeholder", "Select an item" }
                } else {
                    match &*detail_res.read_unchecked() {
                        // P7 — visible loading state while the plugin
                        // resolves `get_view_detail`. The ARIA-busy
                        // attribute and the `.view-row-detail-spinner`
                        // class drive an SR announcement + the spin
                        // animation declared in the theme CSS.
                        None => rsx! {
                            div {
                                class: "client-view-split-loading view-row-detail-loading",
                                "aria-busy": "true",
                                role: "status",
                                span { class: "view-row-detail-spinner", "" }
                                span { "Loading…" }
                            }
                        },
                        Some(Err(err)) => {
                            // Guard against the stale sentinel: detail_res resolves to
                            // Err("no row selected") synchronously on mount (before any
                            // row is clicked). After the first click, selected_id is
                            // Some(_) but detail_res hasn't reset to None yet — it still
                            // holds the old sentinel Err. Treat that transient state as
                            // "loading" so the user sees the spinner, not "Failed to
                            // load detail".
                            let is_sentinel = matches!(err, ClientError::NotFound(msg) if msg == "no row selected");
                            if is_sentinel {
                                rsx! {
                                    div {
                                        class: "client-view-split-loading view-row-detail-loading",
                                        "aria-busy": "true",
                                        role: "status",
                                        span { class: "view-row-detail-spinner", "" }
                                        span { "Loading…" }
                                    }
                                }
                            } else if is_session_expired(err) {
                                tracing::debug!("SplitBody: get_view_detail failed (session expired): {err:?}");
                                rsx! {
                                    SessionExpiredCard {
                                        backend: nav_backend.clone(),
                                        instance_id: nav_instance_id.clone(),
                                        account_id: account_id.clone(),
                                        backend_display_name: nav_backend.clone(),
                                    }
                                }
                            } else {
                                tracing::debug!("SplitBody: get_view_detail failed: {err:?}");
                                rsx! { div { class: "client-view-split-error", "Failed to load detail" } }
                            }
                        }
                        Some(Ok(detail)) => {
                            let body = detail.body_block.clone();
                            rsx! { CustomBlock { block: body } }
                        }
                    }
                }
            }
        }
    }
}
