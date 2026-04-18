//! Matrix backend — server context menu extras.
//!
//! Adds Matrix-specific items to the server right-click context menu,
//! such as room directory, space settings, and E2EE verification.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Matrix-specific context menu items for a server (Space).
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Matrix backend.
/// In Matrix, "servers" are Spaces.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    rsx! {
        // phase-3.2: Matrix extras not yet implemented
    }
}
