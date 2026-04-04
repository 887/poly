//! Setup wizard — single-page welcome screen for first launch.
//!
//! Poly is a multi-account messenger that's plugin-based. On first launch,
//! we show a brief explanation of what Poly is and let users get started
//! immediately with demo data pre-loaded.
//!
//! Key generation and recovery phrase management are handled in
//! Settings → Identity, accessible after setup completes.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use dioxus::prelude::*;

/// Feature bullet point for the welcome page.
#[rustfmt::skip]
#[component]
fn FeatureBullet(icon: String, text: String) -> Element {
    rsx! {
        div { class: "setup-feature",
            span { class: "setup-feature-icon", "{icon}" }
            span { class: "setup-feature-text", "{text}" }
        }
    }
}

/// Setup wizard component shown on first launch.
///
/// Single-page welcome screen explaining what Poly is:
/// - Multi-account messenger client
/// - Plugin-based architecture
/// - Loaded with demo data to explore
///
/// Clicking "Get Started" completes setup and enters the main app with
/// demo data active by default. Identity key management is available
/// in Settings → Identity.
///
/// `on_complete` receives an empty account ID — key generation is deferred
/// to Settings → Identity.
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

                div { class: "setup-features",
                    FeatureBullet {
                        icon: "🔌".to_string(),
                        text: t("setup-feature-plugins"),
                    }
                    FeatureBullet {
                        icon: "👥".to_string(),
                        text: t("setup-feature-multi-account"),
                    }
                    FeatureBullet {
                        icon: "🧪".to_string(),
                        text: t("setup-feature-demo"),
                    }
                    FeatureBullet {
                        icon: "🤖".to_string(),
                        text: t("setup-feature-ai"),
                    }
                    FeatureBullet {
                        icon: "🌐".to_string(),
                        text: t("setup-feature-translate"),
                    }
                    FeatureBullet {
                        icon: "🔑".to_string(),
                        text: t("setup-feature-keys"),
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
