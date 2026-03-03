//! Demo backend — server context menu extras.
//!
//! Adds demo-specific items to the server right-click context menu,
//! such as "Regenerate Demo Data" and "Reset Demo State".

use crate::i18n::t;
use dioxus::prelude::*;

/// Demo-specific context menu items for a server.
///
/// These items appear below the common context menu items when
/// right-clicking a server icon that belongs to the demo backend.
#[component]
pub fn ServerContextMenuExtras(server_id: String, account_id: String) -> Element {
    rsx! {
        div { class: "context-menu-separator" }

        // Demo-specific: Regenerate demo data
        div {
            class: "context-menu-item",
            onclick: move |_| {
                // TODO(phase-2.11): Regenerate demo data for this server
                tracing::debug!("Demo: regenerate data for server {server_id}");
            },
            span { class: "context-menu-label", "{t(\"demo-regenerate-data\")}" }
        }
    }
}
