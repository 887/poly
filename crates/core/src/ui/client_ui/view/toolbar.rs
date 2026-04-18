//! Plugin-declared view toolbar — sort / filter / tabs / action items.
//!
//! WP 5 initial render: static display of each option group. Local selected
//! state tracks the default-selected option per group, but changes don't
//! yet propagate back into `get_view_rows` — that wiring is a follow-up.
//! The `action_items` list is ignored for now (they'd render via
//! [`crate::ui::client_ui::ClientMenu`] when an overflow menu lands).
//!
//! ## Lemmy-style toolbar (D30 revival)
//!
//! When the descriptor's `sort_options` has **more than 4** entries we
//! switch from tab chips to a `<select>` dropdown (Lemmy has 19 sorts —
//! chips overflow badly). A live filter input (`.forum-filter-input`) and
//! a refresh button (`.forum-refresh-btn`) are always rendered — they are
//! no-ops for plain chat backends, valuable for feed-style backends.

use crate::i18n::t;
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
    /// User typed in the filter input.
    SetFilterText(String),
    /// User clicked the refresh button.
    Refresh,
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
pub fn ViewToolbar(
    toolbar: ViewToolbarData,
    /// Parent-owned filter signal — this toolbar writes the live input
    /// value into it so the body engines can client-side filter rows.
    #[props(default)]
    filter: Signal<String>,
    /// Parent-owned refresh tick — incremented on refresh button press so
    /// the parent can `.restart()` its `use_resource`.
    #[props(default)]
    refresh_tick: Signal<u32>,
) -> Element {
    let default_sort = default_id(&toolbar.sort_options);
    let default_filter = default_id(&toolbar.filter_options);
    let default_tab = default_id(&toolbar.tabs);

    let mut selected_sort = use_signal(|| default_sort);
    let mut selected_filter = use_signal(|| default_filter);
    let mut selected_tab = use_signal(|| default_tab);

    let sorts = toolbar.sort_options.clone();
    let filters = toolbar.filter_options.clone();
    let tabs = toolbar.tabs.clone();
    // D30 — Lemmy declares 19 sort options; chips overflow badly. Switch
    // to a `<select>` once the plugin declares more than 4 sorts.
    let use_select_for_sorts = sorts.len() > 4;
    let sort_selected_id = selected_sort.read().clone();

    let mut filter_sig = filter;
    let mut refresh_sig = refresh_tick;
    let filter_value = filter_sig.read().clone();

    rsx! {
        div { class: "client-view-toolbar forum-header", role: "toolbar",
            if !tabs.is_empty() {
                div { class: "client-view-toolbar-tabs view-toolbar-tabs forum-nav-tabs", role: "tablist",
                    for tab in tabs {
                        {
                            let id = tab.id.clone();
                            let is_selected = selected_tab.read().as_deref() == Some(id.as_str());
                            let label = t(&tab.label_key);
                            let cls = if is_selected {
                                "client-view-tab view-toolbar-tab forum-nav-tab active"
                            } else {
                                "client-view-tab view-toolbar-tab forum-nav-tab"
                            };
                            let aria_selected = if is_selected { "true" } else { "false" };
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "{cls}",
                                    role: "tab",
                                    "aria-selected": "{aria_selected}",
                                    onclick: move |_| selected_tab.set(Some(id.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            if !sorts.is_empty() {
                if use_select_for_sorts {
                    select {
                        class: "forum-sort-select",
                        "aria-label": "Sort",
                        value: "{sort_selected_id.clone().unwrap_or_default()}",
                        onchange: move |e| selected_sort.set(Some(e.value())),
                        for opt in sorts.clone() {
                            {
                                let id = opt.id.clone();
                                let label = t(&opt.label_key);
                                rsx! { option { key: "{id}", value: "{id}", "{label}" } }
                            }
                        }
                    }
                } else {
                    div { class: "client-view-toolbar-sorts view-toolbar-tabs forum-sort-tabs", role: "tablist",
                        for opt in sorts {
                            {
                                let id = opt.id.clone();
                                let is_selected = selected_sort.read().as_deref() == Some(id.as_str());
                                let label = t(&opt.label_key);
                                let cls = if is_selected {
                                    "client-view-sort view-toolbar-tab forum-sort-tab active"
                                } else {
                                    "client-view-sort view-toolbar-tab forum-sort-tab"
                                };
                                let aria_selected = if is_selected { "true" } else { "false" };
                                rsx! {
                                    button {
                                        key: "{id}",
                                        class: "{cls}",
                                        role: "tab",
                                        "aria-selected": "{aria_selected}",
                                        onclick: move |_| selected_sort.set(Some(id.clone())),
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !filters.is_empty() {
                div { class: "client-view-toolbar-filters view-toolbar-tabs", role: "tablist",
                    for opt in filters {
                        {
                            let id = opt.id.clone();
                            let is_selected = selected_filter.read().as_deref() == Some(id.as_str());
                            let label = t(&opt.label_key);
                            let cls = if is_selected {
                                "client-view-filter-chip view-toolbar-tab selected"
                            } else {
                                "client-view-filter-chip view-toolbar-tab"
                            };
                            let aria_selected = if is_selected { "true" } else { "false" };
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "{cls}",
                                    role: "tab",
                                    "aria-selected": "{aria_selected}",
                                    onclick: move |_| selected_filter.set(Some(id.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            // D30 — live filter input + refresh. Always rendered; they are
            // cheap for backends that don't need them and critical for
            // forum/feed backends that do.
            input {
                class: "forum-filter-input",
                r#type: "search",
                placeholder: "Filter…",
                "aria-label": "Filter items",
                value: "{filter_value}",
                oninput: move |e| filter_sig.set(e.value()),
            }
            button {
                class: "forum-refresh-btn",
                r#type: "button",
                "aria-label": "Refresh",
                title: "Refresh",
                onclick: move |_| {
                    let n = *refresh_sig.read();
                    refresh_sig.set(n.wrapping_add(1));
                },
                "↻"
            }
        }
    }
}

pub(super) fn default_id(opts: &[ToolbarOption]) -> Option<String> {
    opts.iter()
        .find(|o| o.default_selected)
        .map(|o| o.id.clone())
        .or_else(|| opts.first().map(|o| o.id.clone()))
}

/// D30 — policy helper: the toolbar renders a `<select>` dropdown when
/// there are more than 4 sort options, and tab chips otherwise.
pub(crate) fn should_use_sort_select(opts: &[ToolbarOption]) -> bool {
    opts.len() > 4
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn opt(id: &str, default_selected: bool) -> ToolbarOption {
        ToolbarOption {
            id: id.into(),
            label_key: format!("label-{id}"),
            icon: None,
            default_selected,
        }
    }

    #[test]
    fn default_id_picks_default_selected() {
        let opts = vec![opt("a", false), opt("b", true), opt("c", false)];
        assert_eq!(default_id(&opts), Some("b".into()));
    }

    #[test]
    fn default_id_falls_back_to_first_when_none_default() {
        let opts = vec![opt("a", false), opt("b", false)];
        assert_eq!(default_id(&opts), Some("a".into()));
    }

    #[test]
    fn default_id_empty_returns_none() {
        assert_eq!(default_id(&[]), None);
    }

    #[test]
    fn default_selection_marks_single_option() {
        let opts = vec![opt("a", false), opt("b", true), opt("c", false)];
        let chosen = default_id(&opts).unwrap();
        assert_eq!(chosen, "b");
        for o in &opts {
            let expected_selected = o.id == chosen;
            let is_selected = Some(o.id.as_str()) == Some(chosen.as_str()) && o.id == chosen;
            assert_eq!(is_selected, expected_selected);
        }
    }

    #[test]
    fn should_use_sort_select_threshold_is_five_or_more() {
        let four: Vec<_> = (0..4).map(|i| opt(&format!("s{i}"), false)).collect();
        let five: Vec<_> = (0..5).map(|i| opt(&format!("s{i}"), false)).collect();
        assert!(!should_use_sort_select(&four));
        assert!(should_use_sort_select(&five));
    }

    #[test]
    fn should_use_sort_select_empty_is_false() {
        assert!(!should_use_sort_select(&[]));
    }
}
