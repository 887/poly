//! Agent-panel route adapters.
//!
//! All three `/agent*` routes render the same `AgentPage`, only differing in
//! which section is pre-selected. That keeps the agent sub-nav (Integrations
//! / Profile / Personas) visible on every URL — bookmarking
//! `/agent/personas` no longer drops the user into a chromeless page.

use crate::ui::agent::{AgentPage, AgentSection};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// `/agent` — default section (Integrations).
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn AgentRoute() -> Element {
    rsx! {
        AgentPage { initial_section: None }
    }
}

/// `/agent/:section` — opens AgentPage with `section` pre-selected.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn AgentSectionRoute(section: String) -> Element {
    let initial = AgentSection::from_slug(&section);
    rsx! {
        AgentPage { initial_section: Some(initial) }
    }
}

/// `/agent/personas` — kept as an explicit route so bookmarks resolve to the
/// expected section. Renders the same shell as the other agent URLs.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn PersonasRoute() -> Element {
    rsx! {
        AgentPage { initial_section: Some(AgentSection::Personas) }
    }
}

/// Global search page — browse the full node tree of all accounts.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn SearchRoute() -> Element {
    rsx! {
        crate::ui::search::SearchPage { locked_account_id: None }
    }
}
