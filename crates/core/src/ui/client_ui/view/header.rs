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
        div { class: "client-view-header view-header",
            if let Some(title) = title {
                h2 { class: "client-view-header-title view-header-title", "{title}" }
            }
            if let Some(subtitle) = subtitle {
                p { class: "client-view-header-subtitle view-header-subtitle", "{subtitle}" }
            }
            if let Some(block) = info_block {
                div { class: "view-header-info",
                    CustomBlock { block }
                }
            }
        }
    }
}

/// Pure helper — which header parts (title / subtitle / info-block) should
/// render given the descriptor. Exists so the `ViewHeader` contract can be
/// verified without spinning up a Dioxus virtual DOM.
pub(crate) fn header_parts(header: &ViewHeaderData) -> (bool, bool, bool) {
    (
        header.title_key.is_some(),
        header.subtitle_key.is_some(),
        header.info_block.is_some(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn only_title_renders_when_subtitle_and_info_absent() {
        let header = ViewHeaderData {
            title_key: Some("my-title".into()),
            subtitle_key: None,
            info_block: None,
        };
        assert_eq!(header_parts(&header), (true, false, false));
    }

    #[test]
    fn all_three_render_when_all_present() {
        let header = ViewHeaderData {
            title_key: Some("t".into()),
            subtitle_key: Some("s".into()),
            info_block: None,
        };
        assert_eq!(header_parts(&header), (true, true, false));
    }

    #[test]
    fn empty_header_renders_nothing() {
        let header = ViewHeaderData {
            title_key: None,
            subtitle_key: None,
            info_block: None,
        };
        assert_eq!(header_parts(&header), (false, false, false));
    }
}
