//! Settings page — accounts, backup, identity, theme, language, general,
//! voice/video, and notifications.
//!
//! The module is split into sub-modules by section to keep each file
//! under the 150-line component rule.
//!
//! ## 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**
//! of RSX + logic. Extract sub-components rather than growing any file.
//!
//! ## Module layout
//! | Module | Contents |
//! |---|---|
//! | `common` | `PolySelect`, `SelectOption` |
//! | `accounts` | `AccountsSettings` |
//! | `backup` | `BackupSettings` + full two-step wizard |
//! | `identity` | `IdentitySettings`, `MnemonicModal` |
//! | `theme` | `ThemeSettings` + pickers/editors |
//! | `language` | `LanguageSettings` |
//! | `general` | `GeneralSettings` (reset / nuke) |
//! | `voice_video` | `VoiceVideoSettings` |
//! | `notifications` | `NotificationsSettings` |

mod accounts;
mod backup;
mod common;
mod general;
mod identity;
mod language;
mod notifications;
mod theme;
mod voice_video;

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use dioxus::prelude::*;

use accounts::AccountsSettings;
use backup::BackupSettings;
use general::GeneralSettings;
use identity::IdentitySettings;
use language::LanguageSettings;
use notifications::NotificationsSettings;
use theme::ThemeSettings;
use voice_video::VoiceVideoSettings;

// Re-export SettingsNavItem as a private helper so it stays in this file.
/// Navigation item in the settings sidebar.
#[component]
fn SettingsNavItem(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            class: if active { "settings-nav-item active" } else { "settings-nav-item" },
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

/// Settings page component.
///
/// Two-column layout: navigation sidebar + content area.
/// Each section is delegated to its own sub-module component.
#[component]
pub fn SettingsPage() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let section = app_state.read().settings_section;
    // Subscribe to locale signal so nav labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    let sf_raw = search_text.read().clone();
    let sf = sf_raw.to_lowercase();
    // Helper: is this nav item visible given the current search filter?
    let shows = |label: &str| -> bool { sf.is_empty() || label.to_lowercase().contains(&sf) };

    rsx! {
        div { class: "settings-page",
            // Settings navigation
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
                if shows(&t("settings-voice-video")) {
                    SettingsNavItem {
                        label: t("settings-voice-video"),
                        active: section == SettingsSection::VoiceVideo,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::VoiceVideo;
                        },
                    }
                }
                if shows(&t("settings-notifications")) {
                    SettingsNavItem {
                        label: t("settings-notifications"),
                        active: section == SettingsSection::Notifications,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::Notifications;
                        },
                    }
                }
                if shows(&t("settings-accounts")) {
                    SettingsNavItem {
                        label: t("settings-accounts"),
                        active: section == SettingsSection::Accounts,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::Accounts;
                        },
                    }
                }
                if shows(&t("settings-backup")) {
                    SettingsNavItem {
                        label: t("settings-backup"),
                        active: section == SettingsSection::Backup,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::Backup;
                        },
                    }
                }
                if shows(&t("settings-identity")) {
                    SettingsNavItem {
                        label: t("settings-identity"),
                        active: section == SettingsSection::Identity,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::Identity;
                        },
                    }
                }
                if shows(&t("settings-theme")) {
                    SettingsNavItem {
                        label: t("settings-theme"),
                        active: section == SettingsSection::Theme,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::Theme;
                        },
                    }
                }
                if shows(&t("settings-language")) {
                    SettingsNavItem {
                        label: t("settings-language"),
                        active: section == SettingsSection::Language,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::Language;
                        },
                    }
                }
                if shows(&t("settings-general")) {
                    SettingsNavItem {
                        label: t("settings-general"),
                        active: section == SettingsSection::General,
                        onclick: move |_| {
                            app_state.write().settings_section = SettingsSection::General;
                        },
                    }
                }
            }

            // Settings content — each section in its own sub-module.
            div { class: "settings-content",
                match section {
                    SettingsSection::Accounts => rsx! {
                        AccountsSettings {}
                    },
                    SettingsSection::Backup => rsx! {
                        BackupSettings {}
                    },
                    SettingsSection::Identity => rsx! {
                        IdentitySettings {}
                    },
                    SettingsSection::Theme => rsx! {
                        ThemeSettings {}
                    },
                    SettingsSection::Appearance => rsx! {
                        ThemeSettings {}
                    },
                    SettingsSection::Language => rsx! {
                        LanguageSettings {}
                    },
                    SettingsSection::General => rsx! {
                        GeneralSettings {}
                    },
                    SettingsSection::VoiceVideo => rsx! {
                        VoiceVideoSettings {}
                    },
                    SettingsSection::Notifications => rsx! {
                        NotificationsSettings {}
                    },
                }
            }
        }
    }
}
