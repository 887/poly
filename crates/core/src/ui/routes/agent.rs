//! Agent-panel route adapter components.
//!
//! Covers the agent page, persona management, and global search.

use crate::ui::agent::{AgentPage, PersonaManagementRoute as PersonaManagementRouteComponent};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Agent page — app-level, not account-scoped.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn AgentRoute() -> Element {
    rsx! {
        AgentPage {}
    }
}

/// Agent page with a specific section pre-selected via URL.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn AgentSectionRoute(section: String) -> Element {
    let _ = section; // consumed by router; AgentPage reads section from its own state
    rsx! {
        AgentPage {}
    }
}

/// Persona management page at `/agent/personas`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn PersonasRoute() -> Element {
    rsx! {
        PersonaManagementRouteComponent {}
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
