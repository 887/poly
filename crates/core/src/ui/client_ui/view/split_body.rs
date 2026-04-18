//! Split body engine — list on the left, detail on the right. Clicking a
//! row fetches `get_view_detail` for the selected row id.

use crate::client_manager::ClientManager;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::client_ui::CustomBlock;
use dioxus::prelude::*;
use poly_client::{ClientError, SplitSpec, ViewDetail, ViewRowsPage};
use poly_ui_macros::{context_menu, ui_action};

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
    let client_manager: Signal<ClientManager> = use_context();

    let rows_res: Resource<Result<ViewRowsPage, ClientError>> = {
        let account_id = account_id.clone();
        let channel_id = channel_id.clone();
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
    };

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

    let selected = selected_id.read().clone();

    rsx! {
        div { class: "client-view-split",
            div { class: "client-view-split-list",
                match &*rows_res.read_unchecked() {
                    None => rsx! { div { class: "client-view-split-loading", "Loading…" } },
                    Some(Err(err)) => {
                        tracing::debug!("SplitBody: get_view_rows failed: {err:?}");
                        rsx! { div { class: "client-view-split-error", "Failed to load rows" } }
                    }
                    Some(Ok(page)) => {
                        let rows = page.rows.clone();
                        if rows.is_empty() {
                            rsx! { div { class: "client-view-split-empty", "No items" } }
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
                            tracing::debug!("SplitBody: get_view_detail failed: {err:?}");
                            rsx! { div { class: "client-view-split-error", "Failed to load detail" } }
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
