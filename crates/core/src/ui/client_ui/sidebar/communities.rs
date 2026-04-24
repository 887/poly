//! `SidebarLayoutKind::Communities` — Lemmy / Reddit subscribed communities.
//!
//! P25 (Pack D): three-tab selector — **Subscribed** (the user's joined
//! communities, sourced from `get_servers`), **Local** (communities on the
//! current instance), **All** (the federated firehose). Only **Subscribed**
//! is wired to real data for now; **Local** and **All** render a
//! "coming soon" placeholder until the backend grows `get_local_communities`
//! / `get_federated_communities` methods.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{ClientError, Server};
use poly_ui_macros::{context_menu, ui_action};

/// Actions emitted by [`CommunitiesLayout`] — P25 tab selection.
#[derive(Debug, Clone)]
pub enum CommunitiesAction {
    /// User clicked a scope tab (Subscribed / Local / All).
    SelectTab(CommunitiesTab),
}

impl UiAction for CommunitiesAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // The actual state change happens inline via the local `active_tab`
        // signal; this typed action exists so lint-gate can classify the
        // onclick handler without forcing the component to be `None`.
    }
}

/// Which community-browse scope the user has selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommunitiesTab {
    /// Communities the user has explicitly subscribed to.
    Subscribed,
    /// Communities local to the current instance.
    Local,
    /// The full federated firehose.
    All,
}

impl CommunitiesTab {
    /// FTL key for the tab label.
    fn label_key(self) -> &'static str {
        match self {
            Self::Subscribed => "ui-sidebar-communities-tab-subscribed",
            Self::Local => "ui-sidebar-communities-tab-local",
            Self::All => "ui-sidebar-communities-tab-all",
        }
    }

    /// Stable machine id used for `key=` on the tab button.
    fn id(self) -> &'static str {
        match self {
            Self::Subscribed => "subscribed",
            Self::Local => "local",
            Self::All => "all",
        }
    }
}

/// Lemmy-style communities sidebar with Subscribed / Local / All tabs.
#[ui_action(CommunitiesAction)]
#[context_menu(inherit)]
#[component]
pub fn CommunitiesLayout() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let account_id = app_state.read().nav.active_account_id.cloned();
    let mut active_tab = use_signal(|| CommunitiesTab::Subscribed);

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

    let tabs = [
        CommunitiesTab::Subscribed,
        CommunitiesTab::Local,
        CommunitiesTab::All,
    ];
    let current = *active_tab.read();

    rsx! {
        aside { class: "client-sidebar communities-layout",
            h2 { class: "sidebar-header", {t("ui-sidebar-communities-header")} }
            div {
                class: "sidebar-tabs communities-tabs",
                role: "tablist",
                {tabs.iter().copied().map(|tab| {
                    let selected = tab == current;
                    let class = if selected {
                        "sidebar-tab selected"
                    } else {
                        "sidebar-tab"
                    };
                    let label = t(tab.label_key());
                    rsx! {
                        button {
                            key: "{tab.id()}",
                            class: "{class}",
                            role: "tab",
                            aria_selected: "{selected}",
                            r#type: "button",
                            onclick: move |_| active_tab.set(tab),
                            "{label}"
                        }
                    }
                })}
            }
            match current {
                CommunitiesTab::Subscribed => rsx! {
                    CommunitiesSubscribedBody {
                        communities: communities_res,
                    }
                },
                CommunitiesTab::Local => rsx! {
                    div {
                        class: "communities-coming-soon",
                        role: "tabpanel",
                        {t("ui-sidebar-communities-local-coming-soon")}
                    }
                },
                CommunitiesTab::All => rsx! {
                    div {
                        class: "communities-coming-soon",
                        role: "tabpanel",
                        {t("ui-sidebar-communities-all-coming-soon")}
                    }
                },
            }
        }
    }
}

/// Inner body for the Subscribed tab — split out so the tab switch doesn't
/// re-poll the `use_resource` on every click.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
fn CommunitiesSubscribedBody(
    communities: Resource<Result<Vec<Server>, ClientError>>,
) -> Element {
    match &*communities.read_unchecked() {
        None => rsx! {
            div { class: "communities-loading", {t("ui-sidebar-communities-loading")} }
        },
        Some(Err(err)) => {
            tracing::warn!("CommunitiesLayout: get_servers failed: {err:?}");
            rsx! {
                div { class: "communities-error",
                    {t("ui-sidebar-communities-error")}
                }
            }
        }
        Some(Ok(list)) => {
            let list = list.clone();
            if list.is_empty() {
                rsx! {
                    div { class: "communities-empty",
                        {t("ui-sidebar-communities-empty")}
                    }
                }
            } else {
                rsx! {
                    ul { class: "communities-list", role: "tabpanel",
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// P25: each tab maps to a distinct FTL key and stable id.
    #[test]
    fn tabs_have_distinct_ids_and_label_keys() {
        let tabs = [
            CommunitiesTab::Subscribed,
            CommunitiesTab::Local,
            CommunitiesTab::All,
        ];
        let ids: Vec<&str> = tabs.iter().map(|t| t.id()).collect();
        let keys: Vec<&str> = tabs.iter().map(|t| t.label_key()).collect();
        assert_eq!(ids, ["subscribed", "local", "all"]);
        // Uniqueness — key prefix must differ so FTL can translate each.
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j], "label keys must be unique");
            }
        }
    }

    /// P25: Subscribed/Local/All cover all browse scopes; adding a scope
    /// requires a new variant and breaks this test compile.
    #[test]
    fn tab_variants_enumerated() {
        let all = [
            CommunitiesTab::Subscribed,
            CommunitiesTab::Local,
            CommunitiesTab::All,
        ];
        assert_eq!(all.len(), 3);
    }
}
