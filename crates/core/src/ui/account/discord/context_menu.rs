//! Discord backend — server context menu extras.
//!
//! Adds Discord-specific items to the server right-click context menu,
//! such as Server Boost, Sticker management, and integration settings.

use dioxus::prelude::*;

/// Discord-specific context menu items for a server.
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Discord backend.
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    // TODO(phase-3.3): Add Discord-specific context menu items
    // Examples: Server Boost, Sticker Management, Integrations, Audit Log
    rsx! {}
}
