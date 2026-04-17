//! Shared widget components used across multiple settings sections.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.

use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// A (value, display-label) pair for [`PolySelect`].
#[derive(Clone, PartialEq)]
pub(crate) struct SelectOption {
    pub(crate) value: &'static str,
    pub(crate) label: String,
}

/// Fully themed dropdown select — replaces the ugly native `<select>`.
///
/// The native OS select popup ignores CSS custom properties; this component
/// renders entirely in the webview so it respects the active theme.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub(crate) fn PolySelect(
    options: Vec<SelectOption>,
    /// Currently selected value.
    value: String,
    /// Called with the new value string when the user picks an option.
    onchange: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let current_label = options
        .iter()
        .find(|o| o.value == value)
        .map(|o| o.label.as_str())
        .unwrap_or(value.as_str());

    rsx! {
        div { class: "poly-select",
            // Trigger button
            div {
                class: if *open.read() { "poly-select-trigger open" } else { "poly-select-trigger" },
                onclick: move |_| {
                    let v = *open.read();
                    open.set(!v);
                },
                span { class: "poly-select-current", "{current_label}" }
                span { class: "poly-select-chevron", "▾" }
            }
            // Options panel
            if *open.read() {
                div { class: "poly-select-menu",
                    for opt in &options {
                        {
                            let opt_value = opt.value;
                            let is_active = opt.value == value;
                            let opt_label = opt.label.clone();
                            rsx! {
                                div {
                                    class: if is_active { "poly-select-option active" } else { "poly-select-option" },
                                    onclick: move |_| {
                                        open.set(false);
                                        onchange.call(opt_value.to_string());
                                    },
                                    "{opt_label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
