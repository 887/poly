//! Teams backend — server context menu extras.
//!
//! Adds Teams-specific items to the server right-click context menu,
//! such as meeting scheduling, file management, and app connectors.

use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Teams-specific context menu items for a server (Team).
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Teams backend.
/// In Teams, "servers" are Teams.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    // TODO(phase-3.4): Add Teams-specific context menu items
    // Examples: Schedule Meeting, Manage Files, Apps & Connectors, Team Settings
    rsx! {}
}
