//! `MechanismToggle` — one mechanism row styled to match the polished
//! plugin-section toggle rows (e.g. Poly Server's "Use WebSocket").
//!
//! Reactive hygiene: callback-based, no signal writes here.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
pub fn MechanismToggle(
    backend_id: String,
    mechanism_id: String,
    label: String,
    enabled: bool,
    /// When `Some`, the toggle is disabled with a tooltip.
    requires_host_cap: Option<String>,
    on_toggle: EventHandler<bool>,
) -> Element {
    let testid = format!(
        "client-settings-backend-{backend_id}-mechanism-{mechanism_id}-toggle"
    );
    let is_disabled = requires_host_cap.is_some();
    let tooltip = if is_disabled {
        t("client-settings-mechanism-disabled-host-cap")
    } else {
        String::new()
    };

    rsx! {
        div {
            class: if is_disabled { "settings-toggle-row mechanism-disabled" } else { "settings-toggle-row" },
            title: if is_disabled { "{tooltip}" } else { "" },
            div { class: "settings-toggle-label-group",
                label { class: "settings-toggle-label", "{label}" }
            }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    "data-testid": "{testid}",
                    checked: enabled,
                    disabled: is_disabled,
                    onchange: move |e| {
                        if !is_disabled {
                            on_toggle.call(e.checked());
                        }
                    },
                }
                span { class: "toggle-slider" }
            }
        }
    }
}
