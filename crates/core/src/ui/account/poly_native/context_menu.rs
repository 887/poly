//! Poly native server — server context menu extras.
//!
//! Adds Poly-native-specific items to the server right-click context menu,
//! such as federation settings and server administration.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Poly-native-specific context menu items for a server.
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the Poly native backend.
#[ui_action(inherit)]
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    rsx! {
        // phase-3+: Poly-native extras not yet implemented
    }
}
