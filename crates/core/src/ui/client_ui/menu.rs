//! Host component that renders plugin-declared context menu items.
//!
//! Consumes `Vec<MenuItem>` and renders via the existing `ContextMenuItem`
//! primitives. WP 2 fills this in.

use dioxus::prelude::*;
use poly_client::{MenuItem, MenuTargetKind};
use poly_ui_macros::{context_menu, ui_action};

/// WP 2 will flesh this out. For now it renders nothing.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientMenu(
    target: MenuTargetKind,
    target_id: String,
    items: Vec<MenuItem>,
) -> Element {
    let _ = (target, target_id, items);
    rsx! {
        // WP 2: render plugin-declared menu items grouped by slot
    }
}
