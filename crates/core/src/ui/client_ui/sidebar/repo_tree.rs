//! `SidebarLayoutKind::RepoTree` — GitHub / Forgejo repo list with Issues /
//! PRs / Discussions sub-items.
//!
//! Renders the account's servers (treated as repos at the client layer) as a
//! flat list; each repo expands to three hard-coded children. Richer tree
//! structure (branches, folders) is a WP 5 concern tied to the views layer.

use crate::client_manager::ClientManager;
use crate::state::AppState;
use dioxus::prelude::*;
use poly_client::{ClientError, Server};
use poly_ui_macros::{context_menu, ui_action};

/// The three hard-coded tabs every repo exposes.
const REPO_TABS: &[(&str, &str)] = &[
    ("issues", "Issues"),
    ("prs", "Pull Requests"),
    ("discussions", "Discussions"),
];

/// GitHub / Forgejo repo-tree sidebar.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn RepoTreeLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let account_id = app_state.read().nav.active_account_id.clone();

    let repos_res = {
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
        aside { class: "client-sidebar repo-tree-layout",
            h2 { class: "sidebar-header", "Repositories" }
            match &*repos_res.read_unchecked() {
                None => rsx! {
                    div { class: "repo-tree-loading", "Loading repositories…" }
                },
                Some(Err(err)) => {
                    tracing::warn!("RepoTreeLayout: get_servers failed: {err:?}");
                    rsx! {
                        div { class: "repo-tree-error",
                            "Failed to load repositories"
                        }
                    }
                }
                Some(Ok(repos)) => {
                    let repos = repos.clone();
                    if repos.is_empty() {
                        rsx! {
                            div { class: "repo-tree-empty",
                                "No repositories connected"
                            }
                        }
                    } else {
                        rsx! {
                            ul { class: "repo-tree-list",
                                {repos.into_iter().map(|r| rsx! {
                                    li {
                                        key: "{r.id}",
                                        class: "repo-tree-repo",
                                        div { class: "repo-tree-repo-name", "{r.name}" }
                                        ul { class: "repo-tree-tabs",
                                            {REPO_TABS.iter().map(|(id, label)| rsx! {
                                                li {
                                                    key: "{id}",
                                                    class: "repo-tree-tab",
                                                    "{label}"
                                                }
                                            })}
                                        }
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
