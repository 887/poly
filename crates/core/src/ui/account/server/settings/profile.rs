//! Per-server profile settings.
//!
//! Allows the user to set a different display name (nickname) for a specific
//! server. Profile photo override is planned for Phase 2.11.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

pub enum ServerProfileSettingsAction {
    SetNickname(String),
    Save,
}

impl UiAction for ServerProfileSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetNickname(_) => todo!("phase-E: update server nickname input"),
            Self::Save => todo!("phase-E: persist server nickname via storage"),
        }
    }
}

/// Per-server profile settings panel.
#[ui_action(ServerProfileSettingsAction)]
#[rustfmt::skip]
#[context_menu(none)]
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn server_profile_settings_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<ServerProfileSettingsAction>();
        let _ = ServerProfileSettingsAction::SetNickname("TestNick".to_string());
        let _ = ServerProfileSettingsAction::Save;
    }
}
