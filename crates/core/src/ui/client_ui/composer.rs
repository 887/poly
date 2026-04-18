//! Host hooks for plugin-declared composer buttons and per-message actions.
//! WP 6 fills this in.

use dioxus::prelude::*;
use poly_client::{ComposerButton, MenuItem};
use poly_ui_macros::{context_menu, ui_action};

/// Renders plugin-contributed buttons in the composer toolbar.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ComposerHooks(channel_id: String, buttons: Vec<ComposerButton>) -> Element {
    let _ = (channel_id, buttons);
    rsx! {
        // WP 6: render plugin composer buttons per composer-slot
    }
}

/// Renders plugin-contributed per-message action items.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn MessageActions(
    channel_id: String,
    message_id: String,
    items: Vec<MenuItem>,
) -> Element {
    let _ = (channel_id, message_id, items);
    rsx! {
        // WP 6: merge into message hover/overflow menu
    }
}
