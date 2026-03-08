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
mod plugin_settings;
mod plugins;
mod theme;
mod voice_video;

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use crate::ui::routes::Route;
use dioxus::prelude::*;

use accounts::AccountsSettings;
use backup::BackupSettings;
use diagnostics::DiagnosticsPage;
use general::GeneralSettings;
use identity::IdentitySettings;
use language::LanguageSettings;
use media::MediaSettings;
use plugin_settings::{DemoPluginSettings, PluginSettingsPage};
use plugins::PluginsSettings;
use theme::ThemeSettings;
use voice_video::VoiceVideoSettings;

const NAV_SECTIONS: [(&str, SettingsSection); 11] = [
    ("settings-accounts", SettingsSection::Accounts),
    ("settings-voice-video", SettingsSection::VoiceVideo),
    ("settings-backup", SettingsSection::Backup),
    ("settings-identity", SettingsSection::Identity),
    ("settings-theme", SettingsSection::Theme),
    ("settings-media", SettingsSection::Media),
    ("settings-language", SettingsSection::Language),
    ("settings-general", SettingsSection::General),
    ("settings-plugins", SettingsSection::Plugins),
    ("settings-diagnostics", SettingsSection::Diagnostics),
    // Demo always visible so it never disappears when demo is toggled off
    ("settings-demo", SettingsSection::Demo),
];

/// All searchable nodes in the settings tree.
///
/// Each entry is an (i18n label key, section) pair. When the user types in the
/// settings search bar, their query is matched against the translated label for
/// every node across all sections. Matching nodes are shown as a flat list that
/// the user can click to jump directly to the relevant section.
///
/// Ordered: section headers come first, then their sub-items.
const SETTINGS_NODES: &[(&str, SettingsSection)] = &[
    // Accounts
    ("settings-accounts", SettingsSection::Accounts),
    ("settings-add-account", SettingsSection::Accounts),
    ("settings-account-settings", SettingsSection::Accounts),
    // Voice & Video
    ("settings-voice-video", SettingsSection::VoiceVideo),
    ("voice-input-device", SettingsSection::VoiceVideo),
    ("voice-output-device", SettingsSection::VoiceVideo),
    ("voice-input-volume", SettingsSection::VoiceVideo),
    ("voice-output-volume", SettingsSection::VoiceVideo),
    ("voice-mic-test", SettingsSection::VoiceVideo),
    ("voice-input-mode", SettingsSection::VoiceVideo),
    ("voice-input-vad", SettingsSection::VoiceVideo),
    ("voice-input-ptt", SettingsSection::VoiceVideo),
    ("voice-noise-suppression", SettingsSection::VoiceVideo),
    ("voice-echo-cancel", SettingsSection::VoiceVideo),
    ("voice-camera-preview", SettingsSection::VoiceVideo),
    ("voice-noise-cancel", SettingsSection::VoiceVideo),
    // Backup Servers
    ("settings-backup", SettingsSection::Backup),
    ("settings-add-backup", SettingsSection::Backup),
    // Identity
    ("settings-identity", SettingsSection::Identity),
    ("settings-your-id", SettingsSection::Identity),
    ("settings-export-recovery", SettingsSection::Identity),
    // Theme
    ("settings-theme", SettingsSection::Theme),
    ("settings-theme-preset", SettingsSection::Theme),
    ("settings-color-mode", SettingsSection::Theme),
    ("settings-color-overrides", SettingsSection::Theme),
    ("settings-theme-custom-css", SettingsSection::Theme),
    ("settings-theme-import", SettingsSection::Theme),
    ("settings-theme-export", SettingsSection::Theme),
    // Media
    ("settings-media", SettingsSection::Media),
    ("settings-media-active-provider", SettingsSection::Media),
    // Language
    ("settings-language", SettingsSection::Language),
    // General
    ("settings-general", SettingsSection::General),
    ("settings-reset-app", SettingsSection::General),
    ("settings-nuke-app", SettingsSection::General),
    // Plugins
    ("settings-plugins", SettingsSection::Plugins),
    // Diagnostics
    ("settings-diagnostics", SettingsSection::Diagnostics),
    // Demo — always present so the toggle never vanishes from the sidebar
    ("settings-demo", SettingsSection::Demo),
    ("plugin-demo-setting-enabled-label", SettingsSection::Demo),
];

/// Returns true if this section has at least one searchable node whose
/// translated label contains `q` (case-insensitive). Always true when `q` is empty.
fn section_has_search_match(section: SettingsSection, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    SETTINGS_NODES
        .iter()
        .any(|(key, s)| *s == section && t(key).to_lowercase().contains(q))
}

/// Fire-and-forget JS: smooth-scroll the `.settings-content` container so that
/// the section with id `settings-section-{slug}` is near the top of the viewport.
fn scroll_to_section_anchor(slug: &str) {
    let id = format!("settings-section-{slug}");
    let js = format!(
        "(() => {{ \
            const el = document.getElementById('{id}'); \
            const c = el && el.closest('.settings-content'); \
            if (el && c) c.scrollTo({{ top: el.offsetTop - 16, behavior: 'smooth' }}); \
        }})()"
    );
    let _ = document::eval(&js);
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

/// Nav sidebar for settings.
///
/// All items are always visible. Non-matching nav items are dimmed when search
/// is active. Clicking an item scrolls the content area to that section and
/// pushes the corresponding deep-link URL.
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
            div { class: "settings-nav-header",
                h3 { class: "settings-nav-title", "{t(\"settings-title\")}" }
            }
            SettingsSearchBar { search_text }
            for (label_key, section) in NAV_SECTIONS {
                {
                    let label = t(label_key);
                    let has_match = section_has_search_match(section, &filter);
                    let active = current == section;
                    let class = match (active, has_match) {
                        (true, _) => "settings-nav-item active",
                        (false, true) => "settings-nav-item",
                        (false, false) => "settings-nav-item settings-nav-item-dimmed",
                    };
                    rsx! {
                        div {
                            class,
                            onclick: move |_| on_select.call(section),
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// All settings sections stacked vertically for single-scroll navigation.
///
/// Each section is wrapped in a div with id `settings-section-{slug}` so the
/// scroll helper can jump to it. Sections with no nodes matching the current
/// search query are visually dimmed but still visible.
#[rustfmt::skip]
#[component]
fn SettingsAllSections(search_query: String) -> Element {
    let q = search_query.to_lowercase();
    rsx! {
        for (_label_key, section) in NAV_SECTIONS {
            {
                let slug = section.to_slug();
                let id = format!("settings-section-{slug}");
                let has_match = section_has_search_match(section, &q);
                let class = if has_match {
                    "settings-section-block"
                } else {
                    "settings-section-block settings-section-dimmed"
                };
                // Inject the plugin-section divider before the first plugin section,
                // but only when search is not active (dimming is sufficient hint).
                let is_first_plugin = section == SettingsSection::Plugins && q.is_empty();
                rsx! {
                    if is_first_plugin {
                        div { class: "settings-plugin-divider",
                            span { class: "settings-plugin-divider-label",
                                "{t(\"settings-plugins-section-divider\")}"
                            }
                            span { class: "settings-plugin-divider-badge",
                                "{t(\"settings-plugins-badge\")}"
                            }
                        }
                    }
                    div { id, class,
                        match section {
                            SettingsSection::Accounts | SettingsSection::Notifications => rsx! {
                                AccountsSettings {}
                            },
                            SettingsSection::VoiceVideo => rsx! {
                                VoiceVideoSettings {}
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
                            SettingsSection::Plugins => rsx! {
                                PluginsSettings {}
                            },
                            SettingsSection::PluginSettings => rsx! {
                                PluginSettingsPage {}
                            },
                            SettingsSection::Demo => rsx! {
                                DemoPluginSettings {}
                            },
                            SettingsSection::Diagnostics => rsx! {
                                DiagnosticsPage {}
                            },
                        }
                    }
                }
            }
        }
        // Spacer ensures the last nav section can always be scrolled to the top of the viewport.
        div { class: "settings-scroll-spacer" }
    }
}

/// Settings page component (app-level).
///
/// VS Code-style single-scroll layout: the navigation sidebar shows all
/// sections; any section item can be clicked to smooth-scroll the content
/// area to that section. The search bar dims non-matching sections and
/// auto-scrolls to the first match.
///
/// Deep-linking via `/settings/:section` scrolls to the target section on load.
///
/// Account-specific settings (notifications) are handled by
/// [`crate::ui::account::settings::AccountSettingsPage`] instead.
#[rustfmt::skip]
#[component]
pub fn SettingsPage() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    let nav = use_navigator();

    // Memo: isolate settings_section so effects only re-run when IT changes.
    let section_memo = use_memo(move || app_state.read().settings_section);

    // Scroll to the active section whenever it changes (inc. initial route load).
    // Defer with setTimeout to ensure DOM has rendered before scrolling.
    use_effect(move || {
        let slug = section_memo.read().to_slug().to_string();
        // Use setTimeout(0) to defer until after DOM paints
        let js = format!(
            "setTimeout(() => {{ \
                const el = document.getElementById('settings-section-{slug}'); \
                const c = el && el.closest('.settings-content'); \
                if (el && c) c.scrollTo({{ top: el.offsetTop - 16, behavior: 'smooth' }}); \
            }}, 0)"
        );
        let _ = document::eval(&js);
    });

    // When the search query changes to non-empty, scroll to the first matching section.
    use_effect(move || {
        let q = search_text.read().to_lowercase();
        if q.is_empty() {
            return;
        }
        if let Some((_, first)) = NAV_SECTIONS.iter().find(|(_, s)| section_has_search_match(*s, &q)) {
            scroll_to_section_anchor(first.to_slug());
            app_state.write().settings_section = *first;
        }
    });

    let section = *section_memo.read();
    let query = search_text.read().clone();

    rsx! {
        div { class: "settings-page",
            SettingsNavigation {
                current: section,
                search_text,
                on_select: move |next: SettingsSection| {
                    *search_text.write() = String::new();
                    app_state.write().settings_section = next;
                    nav.push(Route::SettingsSectionRoute { section: next.to_slug().to_string() });
                },
            }
            div { class: "settings-content",
                SettingsAllSections { search_query: query }
            }
        }
    }
}
