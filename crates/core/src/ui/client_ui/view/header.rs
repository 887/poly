//! Plugin-declared view header — title / subtitle / optional info block.
//!
//! FTL keys (`title_key`, `subtitle_key`) are resolved via the host
//! `crate::i18n::t` helper; plugin FTL bundles are merged into the host
//! store at plugin-init time, so plugin-contributed keys resolve the same
//! way host keys do (with raw-key fallback on miss). The optional
//! `info_block` is passed through the shared [`CustomBlock`] component.

use crate::i18n::t;
use crate::ui::client_ui::CustomBlock;
use dioxus::prelude::*;
use poly_client::ViewHeader as ViewHeaderData;
use poly_ui_macros::{context_menu, ui_action};

#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ViewHeader(header: ViewHeaderData) -> Element {
    // Resolve plugin-contributed FTL keys via the host i18n store; `t()` falls
    // back to the raw key when no translation is registered.
    let title = header.title_key.as_deref().map(t);
    let subtitle = header.subtitle_key.as_deref().map(t);
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
