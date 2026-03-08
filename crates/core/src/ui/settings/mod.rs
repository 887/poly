//! Settings page — app-level settings only.
//!
//! Account-specific settings (notifications) live in
//! [`crate::ui::account::settings`] instead.
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

mod accounts;
mod backup;
mod common;
mod diagnostics;
mod general;
mod identity;
mod language;
mod media;
mod theme;
mod voice_video;

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use dioxus::prelude::*;

use accounts::AccountsSettings;
use backup::BackupSettings;
use diagnostics::DiagnosticsPage;
use general::GeneralSettings;
use identity::IdentitySettings;
use language::LanguageSettings;
use media::MediaSettings;
use theme::ThemeSettings;
use voice_video::VoiceVideoSettings;

const NAV_SECTIONS: [(&str, SettingsSection); 9] = [
    ("settings-accounts", SettingsSection::Accounts),
    ("settings-voice-video", SettingsSection::VoiceVideo),
    ("settings-backup", SettingsSection::Backup),
    ("settings-identity", SettingsSection::Identity),
    ("settings-theme", SettingsSection::Theme),
    ("settings-media", SettingsSection::Media),
    ("settings-language", SettingsSection::Language),
    ("settings-general", SettingsSection::General),
    ("settings-diagnostics", SettingsSection::Diagnostics),
];

fn matches_settings_search(filter: &str, label: &str) -> bool {
    filter.is_empty() || label.to_lowercase().contains(filter)
}

#[rustfmt::skip]
#[component]
fn SettingsSearchBar(search_text: Signal<String>) -> Element {
    let current = search_text.read().clone();

    rsx! {
        div { class: "settings-search-bar",
            input {
                r#type: "text",
                class: "settings-search-input",
                placeholder: "{t(\"settings-search\")}",
                value: "{current}",
                oninput: move |e| search_text.set(e.value()),
            }
            if !current.is_empty() {
                button {
                    class: "settings-search-clear",
                    onclick: move |_| search_text.set(String::new()),
                    "×"
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn SettingsNavigation(
    current: SettingsSection,
    search_text: Signal<String>,
    on_select: EventHandler<SettingsSection>,
) -> Element {
    let filter = search_text.read().to_lowercase();

    rsx! {
        nav { class: "settings-nav",
            SettingsSearchBar { search_text }
            for (label_key , section) in NAV_SECTIONS {
                {
                    let label = t(label_key);
                    if matches_settings_search(&filter, &label) {
                        rsx! {
                            SettingsNavItem {
                                label,
                                active: current == section,
                                onclick: move |_| on_select.call(section),
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn SettingsContent(section: SettingsSection) -> Element {
    rsx! {
        div { class: "settings-content",
            match section {
                SettingsSection::Accounts | SettingsSection::Notifications => rsx! {
                    AccountsSettings {}
                },
                SettingsSection::Backup => rsx! {
                    BackupSettings {}
                },
                SettingsSection::Identity => rsx! {
                    IdentitySettings {}
                },
                SettingsSection::Theme | SettingsSection::Appearance => rsx! {
                    ThemeSettings {}
                },
                SettingsSection::Media => rsx! {
                    MediaSettings {}
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
                SettingsSection::Diagnostics => rsx! {
                    DiagnosticsPage {}
                },
            }
        }
    }
}

// Re-export SettingsNavItem as a private helper so it stays in this file.
/// Navigation item in the settings sidebar.
#[rustfmt::skip]
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

/// Settings page component (app-level).
///
/// Two-column layout: navigation sidebar + content area.
/// Each section is delegated to its own sub-module component.
///
/// Account-specific settings (notifications) are handled by
/// [`crate::ui::account::settings::AccountSettingsPage`] instead.
#[rustfmt::skip]
#[component]
pub fn SettingsPage() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let section = app_state.read().settings_section;
    // Subscribe to locale signal so nav labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();
    let search_text = use_signal(String::new);

    rsx! {
        div { class: "settings-page",
            SettingsNavigation {
                current: section,
                search_text,
                on_select: move |next| app_state.write().settings_section = next,
            }
            SettingsContent { section }
        }
    }
}
