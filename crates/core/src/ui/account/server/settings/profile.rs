//! Per-server profile settings.
//!
//! Allows the user to set a different display name (nickname) for a specific
//! server. Profile photo override is planned for Phase 2.11.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// Per-server profile settings panel.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn ServerProfileSettings(server_id: String, server_name: String) -> Element {
    let mut nickname = use_signal(String::new);
    let mut saved = use_signal(|| false);

    rsx! {
        div { class: "settings-section",
            h3 { class: "settings-section-title", "{t(\"server-settings-profile\")}" }

            // Nickname field
            div { class: "settings-field",
                label { class: "settings-label", "{t(\"server-profile-nickname\")}" }
                p { class: "settings-hint", "{t(\"server-profile-nickname-hint\")}" }
                input {
                    r#type: "text",
                    class: "settings-input",
                    placeholder: "{server_name}",
                    value: "{nickname}",
                    oninput: move |e| {
                        nickname.set(e.value());
                        saved.set(false);
                    },
                }
            }

            // Save button
            div { class: "settings-actions",
                button {
                    class: "btn-primary",
                    onclick: move |_| {
                        // TODO(phase-2.11): persist nickname via storage
                        saved.set(true);
                    },
                    "{t(\"server-profile-save\")}"
                }
                if saved() {
                    span { class: "settings-saved-badge", "✓ Saved" }
                }
            }
        }
    }
}
