//! `MechanismToggle` — single checkbox + label for one backend mechanism.
//!
//! Reactive hygiene:
//! - No `Signal::write()` — toggle state propagated via `on_toggle` callback.
//! - No render-time `.read()` subscription on hot-path signals.
//! - Disabled when `requires_host_cap` is `Some` (and v1 advertises no caps).

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// A single mechanism toggle row inside a `BackendCard`.
///
/// Props:
/// - `backend_id`: identifies the backend (used in `data-testid`).
/// - `mechanism_id`: stable mechanism ID string (e.g. `"captcha-sandbox"`).
/// - `label`: human-readable label (resolved FTL string or fallback ID).
/// - `enabled`: current on/off state as loaded from MCP.
/// - `requires_host_cap`: when `Some(cap_name)`, the mechanism is disabled
///   because v1 doesn't advertise that host capability. Rendered with a
///   tooltip explaining why.
/// - `on_toggle`: called with the new bool when the user clicks.
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
    let testid = format!("client-settings-backend-{backend_id}-mechanism-{mechanism_id}-toggle");
    let is_disabled = requires_host_cap.is_some();
    let tooltip = if is_disabled {
        t("client-settings-mechanism-disabled-host-cap")
    } else {
        String::new()
    };

    rsx! {
        div {
            class: if is_disabled { "client-settings-mechanism-row mechanism-disabled" } else { "client-settings-mechanism-row" },
            title: if is_disabled { "{tooltip}" } else { "" },
            label { class: "client-settings-mechanism-label",
                input {
                    r#type: "checkbox",
                    class: "client-settings-mechanism-checkbox",
                    "data-testid": "{testid}",
                    checked: enabled,
                    disabled: is_disabled,
                    onchange: move |e| {
                        if !is_disabled {
                            on_toggle.call(e.checked());
                        }
                    },
                }
                span { class: "client-settings-mechanism-name", "{label}" }
                if is_disabled {
                    span { class: "client-settings-mechanism-cap-badge",
                        title: "{tooltip}",
                        "⚠"
                    }
                }
            }
        }
    }
}
