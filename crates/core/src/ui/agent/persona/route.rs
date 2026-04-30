//! PersonaManagementRoute — full-page persona management UI at `/agent/personas`.
//!
//! Lists all personas in a full-page layout with a prominent "Create" button.
//! Opens PersonaEditModal inline.

use super::list_panel::PersonaListPanel;
use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Full-page persona management component.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn PersonaManagementRoute() -> Element {
    rsx! {
        div { class: "persona-management-page",
            div { class: "special-page-header",
                h2 { class: "special-page-title", {t("persona-management-title")} }
            }
            div { class: "persona-management-body",
                p { class: "settings-description", {t("persona-management-desc")} }
                PersonaListPanel {}
            }
        }
    }
}
