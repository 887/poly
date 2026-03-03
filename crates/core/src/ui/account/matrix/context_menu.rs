//! Matrix backend — server context menu extras.
//!
//! Adds Matrix-specific items to the server right-click context menu,
//! such as room directory, space settings, and E2EE verification.

use dioxus::prelude::*;

/// Matrix-specific context menu items for a server (Space).
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Matrix backend.
/// In Matrix, "servers" are Spaces.
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    // TODO(phase-3.2): Add Matrix-specific context menu items
    // Examples: Room Directory, Space Settings, E2EE Verification, Explore Rooms
    rsx! {}
}
