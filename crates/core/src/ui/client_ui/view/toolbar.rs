//! Plugin-declared view toolbar — sort / filter / tabs / action items.
//!
//! WP 5 initial render: static display of each option group. Local selected
//! state tracks the default-selected option per group, but changes don't
//! yet propagate back into `get_view_rows` — that wiring is a follow-up.
//! The `action_items` list is ignored for now (they'd render via
//! [`crate::ui::client_ui::ClientMenu`] when an overflow menu lands).

use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_client::{ToolbarOption, ViewToolbar as ViewToolbarData};
use poly_ui_macros::{context_menu, ui_action};

/// Actions emitted by [`ViewToolbar`]. Currently these only update local
/// selected state; re-issuing `get_view_rows` with the new sort/filter/tab
/// id is a follow-up (WP 5.C / WP 6 territory).
#[derive(Debug, Clone)]
pub enum ClientViewToolbarAction {
    /// User clicked a tab.
    SelectTab(String),
    /// User clicked a sort option.
    SelectSort(String),
    /// User clicked a filter chip.
    SelectFilter(String),
}

impl UiAction for ClientViewToolbarAction {
    fn apply(self, _cx: ActionCx<'_>) {
        // Local-state-only actions; wiring back into the view fetch
        // happens in a follow-up. Kept as a no-op stub so the typed action
        // enum is valid today.
    }
}

#[ui_action(ClientViewToolbarAction)]
#[context_menu(inherit)]
#[component]
pub fn ViewToolbar(toolbar: ViewToolbarData) -> Element {
    let default_sort = default_id(&toolbar.sort_options);
    let default_filter = default_id(&toolbar.filter_options);
    let default_tab = default_id(&toolbar.tabs);

    let mut selected_sort = use_signal(|| default_sort);
    let mut selected_filter = use_signal(|| default_filter);
    let mut selected_tab = use_signal(|| default_tab);

    let sorts = toolbar.sort_options.clone();
    let filters = toolbar.filter_options.clone();
    let tabs = toolbar.tabs.clone();

    rsx! {
        div { class: "client-view-toolbar",
            if !tabs.is_empty() {
                div { class: "client-view-toolbar-tabs",
                    for tab in tabs {
                        {
                            let id = tab.id.clone();
                            let is_selected = selected_tab.read().as_deref() == Some(id.as_str());
                            let label = tab.label_key.clone();
                            let cls = if is_selected {
                                "client-view-tab selected"
                            } else {
                                "client-view-tab"
                            };
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "{cls}",
                                    onclick: move |_| selected_tab.set(Some(id.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            if !sorts.is_empty() {
                div { class: "client-view-toolbar-sorts",
                    for opt in sorts {
                        {
                            let id = opt.id.clone();
                            let is_selected = selected_sort.read().as_deref() == Some(id.as_str());
                            let label = opt.label_key.clone();
                            let cls = if is_selected {
                                "client-view-sort selected"
                            } else {
                                "client-view-sort"
                            };
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "{cls}",
                                    onclick: move |_| selected_sort.set(Some(id.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            if !filters.is_empty() {
                div { class: "client-view-toolbar-filters",
                    for opt in filters {
                        {
                            let id = opt.id.clone();
                            let is_selected = selected_filter.read().as_deref() == Some(id.as_str());
                            let label = opt.label_key.clone();
                            let cls = if is_selected {
                                "client-view-filter-chip selected"
                            } else {
                                "client-view-filter-chip"
                            };
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "{cls}",
                                    onclick: move |_| selected_filter.set(Some(id.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn default_id(opts: &[ToolbarOption]) -> Option<String> {
    opts.iter()
        .find(|o| o.default_selected)
        .map(|o| o.id.clone())
        .or_else(|| opts.first().map(|o| o.id.clone()))
}
