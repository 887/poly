//! Stoat backend — server context menu extras.
//!
//! Adds Stoat-specific items to the server right-click context menu,
//! such as bot management and webhook configuration.

use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Stoat-specific context menu items for a server.
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Stoat backend.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    // TODO(phase-3.1): Add Stoat-specific context menu items
    // Examples: Manage Bots, Webhooks, Server Discovery
    rsx! {}
}
