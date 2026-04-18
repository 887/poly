//! Host component that renders plugin-declared non-chat views.
//! Dispatches across list/card/tree/split body engines. WP 5 fills this in.

use dioxus::prelude::*;
use poly_client::ViewDescriptor;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientView(channel_id: String, descriptor: ViewDescriptor) -> Element {
    let _ = (channel_id, descriptor);
    rsx! {
        // WP 5: dispatch on descriptor.body to ListBody / CardBody / TreeBody / SplitBody
    }
}
