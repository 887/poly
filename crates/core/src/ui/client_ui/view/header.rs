//! Plugin-declared view header — title / subtitle / optional info block.
//!
//! FTL keys (`title_key`, `subtitle_key`) are rendered as the key string
//! itself for now; plugin-bundle FTL merging is a follow-up (see
//! `ClientMenu` render_leaf for the same holding pattern). The optional
//! `info_block` is passed through the shared [`CustomBlock`] component.

use crate::ui::client_ui::CustomBlock;
use dioxus::prelude::*;
use poly_client::ViewHeader as ViewHeaderData;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ViewHeader(header: ViewHeaderData) -> Element {
    let title = header.title_key.clone();
    let subtitle = header.subtitle_key.clone();
    let info_block = header.info_block.clone();

    rsx! {
        div { class: "client-view-header",
            if let Some(title) = title {
                div { class: "client-view-header-title", "{title}" }
            }
            if let Some(subtitle) = subtitle {
                div { class: "client-view-header-subtitle", "{subtitle}" }
            }
            if let Some(block) = info_block {
                CustomBlock { block }
            }
        }
    }
}
