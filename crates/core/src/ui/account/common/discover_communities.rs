//! Discover Communities view — search communities/subreddits across Lemmy and Reddit.
//!
//! Shows a search input and (for Lemmy) scope tabs. Results are fetched via
//! `ClientBackend::search_communities`. An empty query shows nothing — the user
//! must type at least one character to trigger a search.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{BatchedSignal, use_reactive_effect};
use crate::ui::routes::Route;
use dioxus::prelude::*;
use poly_client::{CommunityScope, CommunitySearchSupport, Server};
use poly_ui_macros::{context_menu, ui_action};

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum LoadState {
    Idle,
    Loading,
    Results(Vec<Server>),
    Error(String),
}

// ── Community card ────────────────────────────────────────────────────────────

#[ui_action(None)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
fn CommunityCard(
    server: Server,
    on_open: EventHandler<Server>,
) -> Element {
    let name = server.name.clone();
    let description = server
        .description
        .clone()
        .unwrap_or_default();
    let icon = server.icon_url.clone();
    let s_for_click = server.clone();
    rsx! {
        div { class: "discover-card",
            if let Some(icon_url) = icon {
                img {
                    class: "discover-card-icon",
                    src: "{icon_url}",
                    alt: "{name} icon",
                }
            }
            div { class: "discover-card-body",
                span { class: "discover-card-name", "{name}" }
                if !description.is_empty() {
                    p { class: "discover-card-desc", "{description}" }
                }
            }
            {
                let onclick = move |_evt: MouseEvent| on_open.call(s_for_click.clone());
                rsx! {
                    button {
                        class: "discover-card-open btn-secondary",
                        onclick,
                        "{t(\"ui-discover-action-open\")}"
                    }
                }
            }
        }
    }
}

// ── Main view ─────────────────────────────────────────────────────────────────

/// Root "Discover Communities" view.
///
/// Props:
/// - `account_id` — active account ID used to look up the backend.
/// - `instance_id` — federated instance id (for routing on community open).
/// - `backend_slug` — determines whether scope tabs are shown.
#[ui_action(None)]
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn DiscoverCommunitiesView(
    account_id: String,
    instance_id: String,
    backend_slug: String,
) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();

    let caps = poly_client::capabilities_for_slug(&backend_slug);
    let show_scope_tabs = matches!(
        caps.community_search,
        CommunitySearchSupport::SubscribedLocalAll
    );

    let mut query = use_signal(String::new);
    let mut scope = use_signal(|| CommunityScope::All);
    let mut load_state: Signal<LoadState> = use_signal(|| LoadState::Idle);

    // Deps tuple: (query_string, scope). use_reactive_effect re-fires on change.
    let query_str = query.read().clone();
    let scope_val = *scope.read();
    use_reactive_effect((query_str, scope_val), move |(q, sc)| {
        if q.trim().is_empty() {
            load_state.set(LoadState::Idle);
            return;
        }
        load_state.set(LoadState::Loading);
        let aid = account_id.clone();
        spawn(async move {
            let backend_arc = client_manager.read().get_backend(&aid);
            let Some(backend_arc) = backend_arc else {
                load_state.set(LoadState::Error(t("ui-discover-error-no-backend")));
                return;
            };
            let guard = match backend_arc.read_with_timeout(std::time::Duration::from_secs(10)).await {
                Ok(g) => g,
                Err(_) => {
                    load_state.set(LoadState::Error(t("ui-discover-error-timeout")));
                    return;
                }
            };
            match guard.search_communities(&q, sc, None).await {
                Ok(page) => load_state.set(LoadState::Results(page.items)),
                Err(e) => load_state.set(LoadState::Error(e.to_string())),
            }
        });
    });

    let backend_slug_for_open = backend_slug.clone();
    let instance_id_for_open = instance_id.clone();
    let on_open = move |server: Server| {
        let nav = navigator();
        nav.push(Route::ServerHome {
            backend: backend_slug_for_open.clone(),
            instance_id: instance_id_for_open.clone(),
            account_id: server.account_id.clone(),
            server_id: server.id.clone(),
        });
    };

    let state_snapshot = load_state.read().clone();

    rsx! {
        div { class: "discover-view",
            div { class: "discover-header",
                h2 { class: "discover-title", "{t(\"ui-discover-title\")}" }
            }
            div { class: "discover-search-row",
                {
                    let oninput = move |e: FormEvent| query.set(e.value());
                    rsx! {
                        input {
                            class: "discover-search-input",
                            r#type: "text",
                            placeholder: "{t(\"ui-discover-search-placeholder\")}",
                            value: "{query.read()}",
                            oninput,
                        }
                    }
                }
            }
            if show_scope_tabs {
                div { class: "discover-scope-tabs",
                    for (tab_scope, tab_key) in [
                        (CommunityScope::Subscribed, "ui-discover-tab-subscribed"),
                        (CommunityScope::Local, "ui-discover-tab-local"),
                        (CommunityScope::All, "ui-discover-tab-all"),
                    ] {
                        {
                            let onclick = move |_evt: MouseEvent| scope.set(tab_scope);
                            rsx! {
                                button {
                                    class: if *scope.read() == tab_scope { "discover-tab active" } else { "discover-tab" },
                                    onclick,
                                    "{t(tab_key)}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "discover-results",
                match state_snapshot {
                    LoadState::Idle => rsx! {},
                    LoadState::Loading => rsx! {
                        div { class: "discover-loading", "{t(\"ui-discover-loading\")}" }
                    },
                    LoadState::Error(msg) => rsx! {
                        div { class: "discover-error", "{msg}" }
                    },
                    LoadState::Results(servers) => {
                        if servers.is_empty() {
                            rsx! {
                                div { class: "discover-no-results", "{t(\"ui-discover-no-results\")}" }
                            }
                        } else {
                            rsx! {
                                for server in servers {
                                    CommunityCard {
                                        key: "{server.id}",
                                        server: server.clone(),
                                        on_open: on_open.clone(),
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
