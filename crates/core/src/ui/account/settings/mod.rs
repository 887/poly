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
//! | Voice & Audio | `VoiceSettings` — mic/speaker device pickers + noise cancellation |
//!
//! ## 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**
//! of RSX + logic. Extract sub-components rather than growing any file.

mod notifications;
mod voice_settings;

use crate::i18n::t;
use dioxus::prelude::*;

use super::AccountBar;
use notifications::NotificationsSettings;
use voice_settings::VoiceSettings;

/// Account-scoped settings page.
///
/// Shows only account-relevant preferences: notification toggles and
/// voice/audio device settings. Global settings (theme, language,
/// voice/video, identity, backup) are handled by the app-level `SettingsPage`.
#[component]
pub fn AccountSettingsPage(backend: String, account_id: String) -> Element {
    // Subscribe to locale so labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    // Track which section is active ('notifications' or 'voice')
    let mut active_section = use_signal(|| "notifications".to_string());

    let sf_raw = search_text.read().clone();
    let sf = sf_raw.to_lowercase();
    // Helper: is this nav item visible given the current search filter?
    let shows = |label: &str| -> bool { sf.is_empty() || label.to_lowercase().contains(&sf) };
    let notif_label = t("settings-notifications");
    let voice_label = t("voice-audio-settings");

    let active = active_section.read().clone();

    rsx! {
        div { class: "channel-list-wrapper",
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
                // Notifications section nav item
                if shows(&notif_label) {
                    div {
                        class: if active == "notifications" { "settings-nav-item active" } else { "settings-nav-item" },
                        onclick: move |_| active_section.set("notifications".to_string()),
                        "{notif_label}"
                    }
                }
                // Voice & Audio section nav item
                if shows(&voice_label) {
                    div {
                        class: if active == "voice" { "settings-nav-item active" } else { "settings-nav-item" },
                        onclick: move |_| active_section.set("voice".to_string()),
                        "{voice_label}"
                    }
                }
            }
            AccountBar {}
        }
        div { class: "settings-content",
            div { class: "settings-header",
                h2 { "{t(\"account-settings-title\")} — {account_id.to_uppercase()}" }
            }
            if active == "notifications" {
                NotificationsSettings { account_id: account_id.clone() }
            }
            if active == "voice" {
                VoiceSettings {}
            }
        }
    }
}
