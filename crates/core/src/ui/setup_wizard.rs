//! Setup wizard — single-page welcome screen for first launch.

use crate::i18n::t;
use dioxus::prelude::*;
use poly_ui_macros::context_menu;

/// A large feature card for the welcome screen.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn FeatureCard(icon: String, title: String, body: String) -> Element {
    rsx! {
        div { class: "setup-feature-card",
            div { class: "setup-feature-card-icon", "{icon}" }
            div { class: "setup-feature-card-content",
                h3 { class: "setup-feature-card-title", "{title}" }
                p { class: "setup-feature-card-body", "{body}" }
            }
        }
    }
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
pub fn SetupWizard(on_complete: EventHandler<String>) -> Element {
    rsx! {
        div { class: "setup-wizard",
            div { class: "setup-step setup-welcome",
                h1 { class: "setup-title", "{t(\"setup-welcome-title\")}" }
                p { class: "setup-description setup-tagline",
                    "{t(\"setup-welcome-tagline\")}"
                }

                div { class: "setup-feature-cards",
                    FeatureCard {
                        icon: "🌐".to_string(),
                        title: t("setup-card-connect-title"),
                        body: t("setup-card-connect-body"),
                    }
                    FeatureCard {
                        icon: "🤖".to_string(),
                        title: t("setup-card-ai-title"),
                        body: t("setup-card-ai-body"),
                    }
                    FeatureCard {
                        icon: "🔑".to_string(),
                        title: t("setup-card-byoa-title"),
                        body: t("setup-card-byoa-body"),
                    }
                }

                button {
                    class: "btn btn-primary setup-start-btn",
                    onclick: move |_| {
                        on_complete.call(String::new());
                    },
                    "{t(\"setup-get-started\")}"
                }
            }
        }
    }
}
