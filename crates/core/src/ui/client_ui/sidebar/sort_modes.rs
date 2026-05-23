//! `SidebarLayoutKind::SortModes` — Lemmy / Reddit per-server sort modes.
//!
//! Renders the items declared by the backend's `get_sidebar_declaration`
//! (`SidebarDeclaration.sections[0].items`) as a Discord-channel-style
//! list under the active server. Clicking a row dispatches the backend's
//! `invoke_sidebar_action(action_id)` — the backend is responsible for
//! flipping its current-sort state and emitting a refresh outcome.
//!
//! Sub-grouping (e.g. Reddit's `Top: hour / day / week`) is expressed via
//! `SidebarItem.parent_id`; nested children render under a collapsible
//! `<details>` of the parent.

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{BatchedSignal, ChatLists};
use crate::ui::client_ui::action_outcome::{handle_action_outcome, ActionOutcomeCx};
use crate::ui::client_ui::toast::ToastMessage;
use dioxus::prelude::*;
use poly_client::{SidebarDeclaration, SidebarItem};
use poly_ui_macros::{context_menu, ui_action};

/// Per-server sort-mode list. Each row is clickable and dispatches
/// `invoke_sidebar_action` on the active account's backend.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn SortModesLayout(decl: SidebarDeclaration) -> Element {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let nav: crate::state::BatchedSignal<crate::state::NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let mut active_id = use_signal(String::new);

    let account_id = nav.read().active_account_id.cloned();

    // Items from the first section. Group by parent_id so we can render
    // top-level rows + collapsible children.
    let items: Vec<SidebarItem> = decl
        .sections
        .first()
        .map(|s| s.items.clone())
        .unwrap_or_default();

    let top_level: Vec<SidebarItem> =
        items.iter().filter(|i| i.parent_id.is_none()).cloned().collect();
    let children_of = move |parent_id: &str| -> Vec<SidebarItem> {
        items
            .iter()
            .filter(|i| i.parent_id.as_deref() == Some(parent_id))
            .cloned()
            .collect()
    };

    let current = active_id.read().clone();

    rsx! {
        aside { class: "client-sidebar sort-modes-layout",
            h2 { class: "sidebar-header", {t("ui-sidebar-sort-modes-header")} }
            ul { class: "sort-modes-list", role: "tablist",
                {top_level.iter().map(|item| {
                    let id = item.id.clone();
                    let label = t(&item.label_key);
                    let kids = children_of(&id);
                    let selected = current == id;
                    let class = if selected {
                        "channel-item sort-modes-row active"
                    } else {
                        "channel-item sort-modes-row"
                    };
                    let account_id_top = account_id.clone();
                    let account_id_kids = account_id.clone();
                    let id_for_click = id.clone();
                    let onclick = move |_evt: MouseEvent| {
                        let id_disp = id_for_click.clone();
                        active_id.set(id_disp.clone());
                        let Some(account_id) = account_id_top.clone() else {
                            tracing::warn!("SortModesLayout: no active account — action {id_disp} ignored");
                            return;
                        };
                        let action_id = id_disp.clone();
                        let client_manager = client_manager;
                        spawn(async move {
                            dispatch_sort_action(client_manager, chat_lists, account_id, action_id).await;
                        });
                    };
                    let kids_clone = kids.clone();
                    let parent_id_str = id.clone();
                    rsx! {
                        li { key: "{id}",
                            div { class: "{class}", role: "tab", aria_selected: "{selected}",
                                tabindex: "0", onclick, "{label}"
                            }
                            if !kids_clone.is_empty() {
                                details { class: "sort-modes-children",
                                    summary { {t("ui-sidebar-sort-more")} }
                                    ul { class: "sort-modes-sublist",
                                        {kids_clone.iter().map(|child| {
                                            let cid = child.id.clone();
                                            let clabel = t(&child.label_key);
                                            let cselected = current == cid;
                                            let cclass = if cselected {
                                                "channel-item sort-modes-row child active"
                                            } else {
                                                "channel-item sort-modes-row child"
                                            };
                                            let account_id = account_id_kids.clone();
                                            let cid_click = cid.clone();
                                            let onclick = move |_evt: MouseEvent| {
                                                let id_disp = cid_click.clone();
                                                active_id.set(id_disp.clone());
                                                let Some(account_id) = account_id.clone() else { return; };
                                                let action_id = id_disp.clone();
                                                let client_manager = client_manager;
                                                spawn(async move {
                                                    dispatch_sort_action(client_manager, chat_lists, account_id, action_id).await;
                                                });
                                            };
                                            rsx! {
                                                li { key: "{cid}",
                                                    div { class: "{cclass}", role: "tab", aria_selected: "{cselected}",
                                                        tabindex: "0", onclick, "{clabel}"
                                                    }
                                                }
                                            }
                                        })}
                                    }
                                }
                            }
                            // Silence unused warnings when no children.
                            { let _ = parent_id_str; rsx! {} }
                        }
                    }
                })}
            }
        }
    }
}

async fn dispatch_sort_action(
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    account_id: String,
    action_id: String,
) {
    let outcome = client_manager.peek().with_backend(&account_id, async |b| {
        b.invoke_sidebar_action(&action_id).await
    }).await;
    // Bump the global sidebar-invalidated tick so the body engine's
    // use_resource (keyed on this tick via render_descriptor_inner)
    // re-fires and picks up the backend's new current-sort.
    // The tick now lives on `ChatLists` post Phase C.3.
    chat_lists.batch(|cl| {
        cl.sidebar_invalidated_tick = cl.sidebar_invalidated_tick.wrapping_add(1);
    });
    let Some(toast_queue) = try_consume_context::<Signal<Vec<ToastMessage>>>() else {
        tracing::info!("SortModesLayout: action outcome (no-toast-ctx): {outcome:?}");
        return;
    };
    let Some(refresh_sidebar) = try_consume_context::<Signal<u32>>() else {
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
