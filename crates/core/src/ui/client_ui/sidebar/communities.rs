//! `SidebarLayoutKind::Communities` — Lemmy / Reddit subscribed communities.
//!
//! Renders the backend's servers (Lemmy treats a community as a "server"
//! internally) as a flat list so Lemmy users see their subscriptions
//! immediately. Deeper community metadata (icons, subscriber counts) is a
//! WP 4 follow-up.

use crate::client_manager::ClientManager;
use crate::state::AppState;
use dioxus::prelude::*;
use poly_client::{ClientError, Server};
use poly_ui_macros::{context_menu, ui_action};

/// Lemmy-style flat list of subscribed communities.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn CommunitiesLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let account_id = app_state.read().nav.active_account_id.clone();

    let communities_res = {
        let account_id = account_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            async move {
                let Some(account_id) = account_id else {
                    return Ok::<Vec<Server>, ClientError>(Vec::new());
                };
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = backend.read().await;
                guard.get_servers().await
            }
        })
    };

    rsx! {
        aside { class: "client-sidebar communities-layout",
            h2 { class: "sidebar-header", "Communities" }
            match &*communities_res.read_unchecked() {
                None => rsx! {
                    div { class: "communities-loading", "Loading communities…" }
                },
                Some(Err(err)) => {
                    tracing::warn!("CommunitiesLayout: get_servers failed: {err:?}");
                    rsx! {
                        div { class: "communities-error",
                            "Failed to load communities"
                        }
                    }
                }
                Some(Ok(list)) => {
                    let list = list.clone();
                    if list.is_empty() {
                        rsx! {
                            div { class: "communities-empty",
                                "No communities subscribed"
                            }
                        }
                    } else {
                        rsx! {
                            ul { class: "communities-list",
                                {list.into_iter().map(|c| rsx! {
                                    li {
                                        key: "{c.id}",
                                        class: "community-row",
                                        "{c.name}"
                                    }
                                })}
                            }
                        }
                    }
                }
            }
        }
    }
}
