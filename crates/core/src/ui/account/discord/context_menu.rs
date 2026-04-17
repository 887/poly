//! Discord backend — server context menu extras.
//!
//! Adds Discord-specific items to the server right-click context menu,
//! such as Server Boost, Sticker management, and integration settings.

use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Discord-specific context menu items for a server.
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Discord backend.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    todo!("phase-3.3: Discord-specific context menu items — Server Boost, Sticker Management, Integrations, Audit Log")
}
