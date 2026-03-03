//! Account-scoped settings page.
//!
//! A focused settings view that shows only account-specific preferences.
//! Unlike the app-level `SettingsPage`, this page scopes everything to
//! one account and omits global concerns (theme, language, identity, etc.).
//!
//! ## Sections
//! | Section | Component |
//! |---|---|
//! | Notifications | `NotificationsSettings` — per-account notification toggles |
//!
//! ## 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**
//! of RSX + logic. Extract sub-components rather than growing any file.

mod notifications;

use crate::i18n::t;
use dioxus::prelude::*;

use notifications::NotificationsSettings;

/// Account-scoped settings page.
///
/// Shows only account-relevant preferences: notification toggles.
/// Global settings (theme, language, voice/video, identity, backup) are
/// handled by the app-level `SettingsPage` instead.
#[component]
pub fn AccountSettingsPage(backend: String, account_id: String) -> Element {
    // Subscribe to locale so labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);

    let sf_raw = search_text.read().clone();
    let sf = sf_raw.to_lowercase();
    // Helper: is this nav item visible given the current search filter?
    let shows = |label: &str| -> bool { sf.is_empty() || label.to_lowercase().contains(&sf) };
    let notif_label = t("settings-notifications");

    rsx! {
        div { class: "settings-page",
            nav { class: "settings-nav",
                // Search bar
                div { class: "settings-search-bar",
                    input {
                        r#type: "text",
                        class: "settings-search-input",
                        placeholder: "{t(\"settings-search\")}",
                        value: "{sf_raw}",
                        oninput: move |e| search_text.set(e.value()),
                    }
                    if !sf_raw.is_empty() {
                        button {
                            class: "settings-search-clear",
                            onclick: move |_| search_text.set(String::new()),
                            "×"
                        }
                    }
                }
                // Notifications section
                if shows(&notif_label) {
                    div { class: "settings-nav-item active", "{notif_label}" }
                }
            }
            div { class: "settings-content",
                div { class: "settings-header",
                    h2 { "{t(\"account-settings-title\")} — {account_id.to_uppercase()}" }
                }
                NotificationsSettings { account_id: account_id.clone() }
            }
        }
    }
}
