//! Flat-list body engine — renders `get_view_rows` as a vertical list using
//! the plugin-declared `RowTemplate`.
//!
//! WP 5 scope: first page only (no infinite scroll). Rows show
//! `primary_text`, `secondary_text` and `meta_text` raw strings from the
//! plugin (they are content, not FTL keys — see `ViewRow` doc).

use crate::client_manager::ClientManager;
use crate::state::AppState;
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{ClientError, ListSpec, ViewRowsPage};
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the flat-list body engine — currently only row-selection,
/// which navigates to the per-post forum-thread route. WP 5.C will wire
/// plugin-declared `action-outcome::navigate` so this hardcoded mapping
/// can fall away.
#[derive(Debug, Clone)]
pub enum ClientViewListAction {
    /// User clicked a row; the host pushes `Route::ForumPostRoute`.
    OpenRow(String),
}

impl UiAction for ClientViewListAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Navigation happens inline via `navigator()` — the typed enum is
        // here so the ui-action-coverage lint is satisfied and MCP has a
        // vocabulary for enumerating row-click actions.
    }
}

#[ui_action(ClientViewListAction)]
#[context_menu(inherit)]
#[component]
pub fn ListBody(channel_id: String, account_id: String, spec: ListSpec) -> Element {
    let _ = spec; // page_size honored implicitly by the plugin.
    let rows_res = fetch_first_page(channel_id.clone(), account_id.clone());
    let app_state: Signal<AppState> = use_context();
    let nav = navigator();

    // Route-params for row-click navigation — mirrors the old LemmyForumView.
    let (backend_slug, instance_id, account_for_route, server_id, channel_for_route) = {
        let s = app_state.read();
        (
            s.nav
                .active_backend
                .as_ref()
                .map(|b| b.slug().to_string())
                .unwrap_or_default(),
            s.nav.active_instance_id.clone().unwrap_or_default(),
            s.nav.active_account_id.clone().unwrap_or_default(),
            s.nav.selected_server.clone().unwrap_or_default(),
            s.nav.selected_channel.clone().unwrap_or_default(),
        )
    };

    match &*rows_res.read_unchecked() {
        None => rsx! {
            div { class: "client-view-list client-view-list-loading",
                span { "Loading…" }
            }
        },
        Some(Err(err)) => {
            tracing::debug!("ListBody: get_view_rows failed: {err:?}");
            rsx! {
                div { class: "client-view-list client-view-list-error",
                    span { "Failed to load rows" }
                }
            }
        }
        Some(Ok(page)) => {
            let rows = page.rows.clone();
            if rows.is_empty() {
                rsx! {
                    div { class: "client-view-list client-view-list-empty",
                        span { "No items" }
                    }
                }
            } else {
                rsx! {
                    div { class: "client-view-list",
                        for row in rows {
                            {
                                let id = row.id.clone();
                                let id_for_click = id.clone();
                                let primary = row.primary_text.clone();
                                let secondary = row.secondary_text.clone();
                                let meta = row.meta_text.clone();
                                let icon = row.icon.clone();
                                let badge = row.badge.clone();
                                let backend_slug = backend_slug.clone();
                                let instance_id = instance_id.clone();
                                let account_for_route = account_for_route.clone();
                                let server_id = server_id.clone();
                                let channel_for_route = channel_for_route.clone();
                                rsx! {
                                    div {
                                        key: "{id}",
                                        class: "client-view-list-row",
                                        onclick: move |_| {
                                            nav.push(Route::ForumPostRoute {
                                                backend: backend_slug.clone(),
                                                instance_id: instance_id.clone(),
                                                account_id: account_for_route.clone(),
                                                server_id: server_id.clone(),
                                                channel_id: channel_for_route.clone(),
                                                post_id: id_for_click.clone(),
                                            });
                                        },
                                        if let Some(icon) = icon {
                                            span { class: "client-view-row-icon", "{icon}" }
                                        }
                                        div { class: "client-view-row-text",
                                            div { class: "client-view-row-primary", "{primary}" }
                                            if let Some(sec) = secondary {
                                                div { class: "client-view-row-secondary", "{sec}" }
                                            }
                                            if let Some(meta) = meta {
                                                div { class: "client-view-row-meta", "{meta}" }
                                            }
                                        }
                                        if let Some(badge) = badge {
                                            span { class: "client-view-row-badge", "{badge}" }
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
