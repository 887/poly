//! Poly native server — server context menu extras.
//!
//! Adds Poly-native-specific items to the server right-click context menu,
//! such as federation settings and server administration.

use dioxus::prelude::*;

/// Poly-native-specific context menu items for a server.
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Poly native backend.
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    // TODO(phase-3+): Add Poly-native-specific context menu items
    // Examples: Federation Settings, Server Admin, Custom Emoji Management
    rsx! {}
}
