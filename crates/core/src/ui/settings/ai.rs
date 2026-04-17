//! AI provider settings — configure API keys and model selection for
//! auto-responses, chat summaries, live translation, and the social agent.

use crate::i18n::t;
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the AI settings section.
pub enum AiSettingsAction {
    /// Set the API key for the configured AI provider.
    SetApiKey(String),
    /// Switch to a different AI provider.
    SetProvider(String),
}

impl UiAction for AiSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetApiKey(_key) => todo!("phase-E: persist AI provider API key"),
            Self::SetProvider(_provider) => todo!("phase-E: switch AI provider"),
        }
    }
}

/// Placeholder AI settings section.
///
/// Full implementation (provider selection, API key input, model picker,
/// test connection, feature toggles, usage tracking) is Phase 5 work.
/// This stub registers the section in the settings navigation so the UI
/// slot exists and users can see what's coming.
#[rustfmt::skip]
#[ui_action(AiSettingsAction)]
#[context_menu(inherit)]
#[component]
pub(super) fn AiSettings() -> Element {
    rsx! {
        div { class: "settings-section ai-settings",
            h2 { "{t(\"settings-ai\")}" }
            p { class: "settings-description", "{t(\"settings-ai-description\")}" }

            div { class: "ai-features-preview",
                h3 { class: "settings-subsection-title", "{t(\"settings-ai-features\")}" }
                div { class: "setup-features",
                    div { class: "setup-feature",
                        span { class: "setup-feature-icon", "💬" }
                        span { class: "setup-feature-text", "{t(\"settings-ai-feature-responses\")}" }
                    }
                    div { class: "setup-feature",
                        span { class: "setup-feature-icon", "📋" }
                        span { class: "setup-feature-text", "{t(\"settings-ai-feature-summaries\")}" }
                    }
                    div { class: "setup-feature",
                        span { class: "setup-feature-icon", "🌐" }
                        span { class: "setup-feature-text", "{t(\"settings-ai-feature-translate\")}" }
                    }
                    div { class: "setup-feature",
                        span { class: "setup-feature-icon", "🧠" }
                        span { class: "setup-feature-text", "{t(\"settings-ai-feature-memory\")}" }
                    }
                    div { class: "setup-feature",
                        span { class: "setup-feature-icon", "📅" }
                        span { class: "setup-feature-text", "{t(\"settings-ai-feature-outreach\")}" }
                    }
                    div { class: "setup-feature",
                        span { class: "setup-feature-icon", "🎨" }
                        span { class: "setup-feature-text", "{t(\"settings-ai-feature-image-gen\")}" }
                    }
                }
            }

            p { class: "settings-description settings-coming-soon",
                "Configure your AI provider API key here to enable these features. Coming soon."
            }
        }
    }
}
