//! Host component that renders plugin-authored sanitized HTML in a shadow-root.
//! WP 5 fills this in. For now, renders nothing.
//!
//! Security: sanitizes via `ammonia` (allowlist documented in plan §4.6), wraps
//! in a shadow-root, inlines scoped CSS.

use dioxus::prelude::*;
use poly_client::CustomBlock as CustomBlockData;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn CustomBlock(block: CustomBlockData) -> Element {
    let _ = block;
    rsx! {
        // WP 5: sanitize, render in shadow-root with scoped CSS
    }
}
