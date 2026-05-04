//! `SidebarLayoutKind::RepoTree` — GitHub / Forgejo repo list with Issues /
//! PRs / Discussions sub-items.
//!
//! P27 (Pack D): the three child rows (Issues / PRs / Discussions) are now
//! **clickable** and dispatch `invoke_sidebar_action` with ids
//! `repo-{repo_id}-issues`, `repo-{repo_id}-pulls`, `repo-{repo_id}-discussions`.
//!
//! TODO(D28, long-term): migrate the hardcoded children to the plugin.
//! GitHub / Forgejo's `get_sidebar_declaration` should return a
//! `SidebarLayoutKind::Custom` carrying the 3 child `SidebarItem`s per repo,
//! linked via `parent_id` to the repo's section. Then `CustomSidebar`
//! renders the tree uniformly and this host-side file can disappear. Tracked
//! as a later pack so we don't block Pack D on a plugin WIT touch.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use crate::ui::client_ui::action_outcome::{handle_action_outcome, ActionOutcomeCx};
use crate::ui::client_ui::toast::ToastMessage;
use dioxus::prelude::*;
use poly_client::{ClientError, Server};
use poly_ui_macros::{context_menu, ui_action};

/// The three hard-coded tabs every repo exposes.
/// Tuple: `(kebab_suffix, label_key)`.
const REPO_TABS: &[(&str, &str)] = &[
    ("issues", "ui-sidebar-repo-issues"),
    ("pulls", "ui-sidebar-repo-pulls"),
    ("discussions", "ui-sidebar-repo-discussions"),
];

/// Build the canonical action id for a repo child row.
fn repo_action_id(repo_id: &str, tab_suffix: &str) -> String {
    format!("repo-{repo_id}-{tab_suffix}")
}

/// GitHub / Forgejo repo-tree sidebar.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn RepoTreeLayout() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let account_id = nav.read().active_account_id.cloned();

    let repos_res = {
        let account_id = account_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            async move {
                let Some(account_id) = account_id else {
                    return Ok::<Vec<Server>, ClientError>(Vec::new());
                };
                client_manager.peek().with_backend(&account_id, async |b| {
                    b.get_servers().await
                }).await
            }
        })
    };

    rsx! {
        aside { class: "client-sidebar repo-tree-layout",
            h2 { class: "sidebar-header", {t("ui-sidebar-repos-header")} }
            match &*repos_res.read_unchecked() {
                None => rsx! {
                    div { class: "repo-tree-loading", {t("ui-sidebar-repos-loading")} }
                },
                Some(Err(err)) => {
                    tracing::warn!("RepoTreeLayout: get_servers failed: {err:?}");
                    rsx! {
                        div { class: "repo-tree-error",
                            {t("ui-sidebar-repos-error")}
                        }
                    }
                }
                Some(Ok(repos)) => {
                    let repos = repos.clone();
                    if repos.is_empty() {
                        rsx! {
                            div { class: "repo-tree-empty",
                                {t("ui-sidebar-repos-empty")}
                            }
                        }
                    } else {
                        rsx! {
                            ul { class: "repo-tree-list",
                                {repos.into_iter().map(|r| {
                                    let repo_id = r.id.clone();
                                    let repo_name = r.name.clone();
                                    rsx! {
                                        li {
                                            key: "{repo_id}",
                                            class: "repo-tree-repo",
                                            div { class: "repo-tree-repo-name", "{repo_name}" }
                                            RepoTabs {
                                                repo_id: repo_id.clone(),
                                                account_id: account_id.clone(),
                                                client_manager,
                                            }
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

/// The three-tab list under one repo — extracted so closures can own their
/// own action-id string per tab without borrow-checker gymnastics.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn RepoTabs(
    repo_id: String,
    account_id: Option<String>,
    client_manager: BatchedSignal<ClientManager>,
) -> Element {
    rsx! {
        ul { class: "repo-tree-tabs",
            {REPO_TABS.iter().map(|(suffix, label_key)| {
                let action_id = repo_action_id(&repo_id, suffix);
                let label = t(label_key);
                let account_id = account_id.clone();
                let aid_for_click = action_id.clone();
                let onclick = move |_evt: MouseEvent| {
                    let Some(account_id) = account_id.clone() else {
                        tracing::warn!("RepoTreeLayout: no active account — action {aid_for_click} ignored");
                        return;
                    };
                    let action_id = aid_for_click.clone();
                    let client_manager = client_manager;
                    spawn(async move {
                        dispatch_repo_action(client_manager, account_id, action_id).await;
                    });
                };
                rsx! {
                    li {
                        key: "{suffix}",
                        class: "repo-tree-tab",
                        role: "button",
                        tabindex: "0",
                        onclick,
                        "{label}"
                    }
                }
            })}
        }
    }
}

async fn dispatch_repo_action(
    client_manager: BatchedSignal<ClientManager>,
    account_id: String,
    action_id: String,
) {
    let outcome = client_manager.peek().with_backend(&account_id, async |b| {
        b.invoke_sidebar_action(&action_id).await
    }).await;
    let Some(toast_queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() else {
        tracing::debug!("RepoTreeLayout: no toast queue in context — logging only");
        tracing::info!("RepoTreeLayout: action outcome (no-toast-ctx): {outcome:?}");
        return;
    };
    let Some(refresh_sidebar) = try_consume_context::<Signal<u32>>() else {
        tracing::debug!("RepoTreeLayout: no sidebar refresh signal in context");
        return;
    };
    let cx = ActionOutcomeCx {
        toast_queue,
        refresh_sidebar,
        refresh_target: None,
        client_manager,
        account_id,
    };
    handle_action_outcome(outcome, cx);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// P27: repo child action ids follow the canonical
    /// `repo-{repo_id}-{suffix}` shape the plan specifies.
    #[test]
    fn repo_action_id_canonical_shape() {
        assert_eq!(
            repo_action_id("drona23/poly", "issues"),
            "repo-drona23/poly-issues"
        );
        assert_eq!(repo_action_id("42", "pulls"), "repo-42-pulls");
        assert_eq!(
            repo_action_id("owner/name", "discussions"),
            "repo-owner/name-discussions"
        );
    }

    /// P27: the three tabs stay stable — a regression here means plugin
    /// authors would need to re-map their handlers.
    #[test]
    fn repo_tab_suffixes_match_plan() {
        let suffixes: Vec<&str> = REPO_TABS.iter().map(|(s, _)| *s).collect();
        assert_eq!(suffixes, ["issues", "pulls", "discussions"]);
    }
}
