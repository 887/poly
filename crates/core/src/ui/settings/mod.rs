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
//! | `general` | `LayoutSettings`, `GeneralSettings` |
//! | `voice_video` | `VoiceVideoSettings` |

mod accounts;
mod backup;
pub(crate) mod common;
mod diagnostics;
mod general;
mod identity;
mod language;
mod media;
mod plugin_settings;
pub(crate) mod scroll_spy;
// Re-export the demo render function so ui/demo.rs can register it at runtime
// via ClientManager::register_plugin_settings without knowing UI module internals.
#[cfg(feature = "demo")]
pub(crate) use plugin_settings::demo_settings_render_fn;
#[cfg(feature = "stoat")]
pub(crate) use plugin_settings::stoat_settings_render_fn;
// Re-export the poly server render function so ui/mod.rs can register it at startup.
#[cfg(feature = "server")]
pub(crate) use plugin_settings::poly_settings_render_fn;
mod plugins;
mod theme;
mod voice_video;

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::routes::Route;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use scroll_spy::scroll_to_settings_section;
#[cfg(target_arch = "wasm32")]
use scroll_spy::{
    SettingsScrollSpyConfig, install_settings_scroll_spy as install_shared_settings_scroll_spy,
};

use accounts::AccountsSettings;
use backup::BackupSettings;
use diagnostics::DiagnosticsPage;
use general::{GeneralSettings, LayoutSettings};
use identity::IdentitySettings;
use language::LanguageSettings;
use media::MediaSettings;
use plugins::PluginsSettings;
use theme::ThemeSettings;
use voice_video::VoiceVideoSettings;

// plugin_settings is used via the dynamic registry — no compile-time import
// of specific plugin components into the host.

const NAV_SECTIONS: [(&str, SettingsSection); 11] = [
    ("settings-accounts", SettingsSection::Accounts),
    ("settings-voice-video", SettingsSection::VoiceVideo),
    ("settings-backup", SettingsSection::Backup),
    ("settings-identity", SettingsSection::Identity),
    ("settings-theme", SettingsSection::Theme),
    ("settings-media", SettingsSection::Media),
    ("settings-language", SettingsSection::Language),
    ("settings-layout", SettingsSection::Layout),
    ("settings-general", SettingsSection::General),
    ("settings-plugins", SettingsSection::Plugins),
    ("settings-diagnostics", SettingsSection::Diagnostics),
    // Plugin-provided settings pages are NOT in this static array.
    // They are registered at runtime via ClientManager::register_plugin_settings.
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
    // Layout
    ("settings-layout", SettingsSection::Layout),
    ("settings-layout-mode", SettingsSection::Layout),
    ("settings-mirror-menu-layout", SettingsSection::Layout),
    ("settings-mirror-chat-messages", SettingsSection::Layout),
    // General
    ("settings-general", SettingsSection::General),
    ("settings-reset-app", SettingsSection::General),
    ("settings-nuke-app", SettingsSection::General),
    // Plugins
    ("settings-plugins", SettingsSection::Plugins),
    // Diagnostics
    ("settings-diagnostics", SettingsSection::Diagnostics),
    // Plugin-provided settings pages are not listed here; they have no static
    // search nodes. Search coverage for plugin pages is a future TODO.
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

/// Count matching searchable nodes for a section. Returns 0 when `q` is empty
/// (caller should treat empty query as "show all, no badges").
fn section_match_count(section: SettingsSection, q: &str) -> usize {
    if q.is_empty() {
        return 0;
    }
    SETTINGS_NODES
        .iter()
        .filter(|(key, s)| *s == section && t(key).to_lowercase().contains(q))
        .count()
}

/// Total number of matching nodes across all sections.
fn total_match_count(q: &str) -> usize {
    if q.is_empty() {
        return 0;
    }
    SETTINGS_NODES
        .iter()
        .filter(|(key, _)| t(key).to_lowercase().contains(q))
        .count()
}

/// Fire-and-forget JS: smooth-scroll the `.settings-content` container so that
/// the section with id `settings-section-{slug}` is near the top of the viewport.
fn scroll_to_section_anchor(slug: &str) {
    scroll_to_settings_section("settings-section-", slug);
}

#[rustfmt::skip]
#[component]
fn SettingsSearchBar(search_text: Signal<String>) -> Element {
    let current = search_text.read().clone();
    let total = total_match_count(&current.to_lowercase());

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
                span { class: "settings-search-count",
                    "{total} {t(\"settings-search-found\")}"
                }
                button {
                    class: "settings-search-clear",
                    onclick: move |_| search_text.set(String::new()),
                    "×"
                }
            }
        }
    }
}
fn install_settings_scroll_spy(
    _app_state: Signal<AppState>,
    _plugin_section_ids: Vec<String>,
    _active_plugin_slug: Signal<Option<String>>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut app_state = _app_state;
        let mut active_plugin_slug = _active_plugin_slug;
        let config = SettingsScrollSpyConfig {
            runtime_flag: "__polySettingsScrollSpyInstalled",
            scroll_root_selectors: vec![
                ".poly-split-content.settings-content > .poly-split-content-stage",
                ".settings-content",
            ],
            section_prefix: "settings-section-",
            section_ids: [
                "settings-section-accounts",
                "settings-section-voice-video",
                "settings-section-backup",
                "settings-section-identity",
                "settings-section-theme",
                "settings-section-media",
                "settings-section-language",
                "settings-section-layout",
                "settings-section-general",
                "settings-section-plugins",
                "settings-section-diagnostics",
            ]
            .into_iter()
            .map(ToString::to_string)
            .chain(_plugin_section_ids.into_iter())
            .collect(),
            plugin_section_prefix: Some("settings-section-plugin-"),
        };
        install_shared_settings_scroll_spy(config, move |slug| {
            if let Some(plugin_slug) = slug.strip_prefix("plugin-") {
                if active_plugin_slug.read().as_deref() != Some(plugin_slug) {
                    active_plugin_slug.set(Some(plugin_slug.to_string()));
                }
                if app_state.read().settings_section != SettingsSection::Plugins {
                    app_state.write().settings_section = SettingsSection::Plugins;
                }
            } else {
                active_plugin_slug.set(None);
                let next = SettingsSection::from_slug(&slug);
                if app_state.read().settings_section != next {
                    app_state.write().settings_section = next;
                }
            }
        });
    }
}

#[rustfmt::skip]
#[component]
fn SettingsContentHeader(search_text: Signal<String>) -> Element {
    rsx! {
        div { class: "special-page-header settings-page-header",
            h2 { class: "special-page-title", "{t(\"settings-title\")}" }
            SettingsSearchBar { search_text }
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
    active_plugin_slug: Signal<Option<String>>,
    on_select: EventHandler<SettingsSection>,
) -> Element {
    let filter = search_text.read().to_lowercase();
    let active_plugin = active_plugin_slug.read().clone();
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    // Snapshot the registered plugin settings pages so we don't hold the read
    // guard across the RSX macro expansion.
    let plugin_entries: Vec<_> = client_manager.read().plugin_settings.to_vec();

    rsx! {
        nav { class: "settings-nav",
            div { class: "settings-nav-header",
                h3 { class: "settings-nav-title", "{t(\"settings-title\")}" }
            }
            for (label_key, section) in NAV_SECTIONS {
                {
                    let label = t(label_key);
                    let has_match = section_has_search_match(section, &filter);
                    let count = section_match_count(section, &filter);
                    let active = current == section;
                    let searching = !filter.is_empty();
                    // Always render so scroll spy can find data-settings-slug; hide via CSS
                    // Hide takes priority over active — if searching and no match, always hide.
                    let class = match (searching, has_match, active) {
                        (true, false, _) => "settings-nav-item settings-nav-item-hidden",
                        (_, _, true) => "settings-nav-item active",
                        _ => "settings-nav-item",
                    };
                    rsx! {
                        div {
                            class,
                            onclick: move |_| {
                                active_plugin_slug.set(None);
                                app_state.write().settings_section = section;
                                on_select.call(section);
                                close_mobile_drawer();
                            },
                            "data-settings-slug": "{section.to_slug()}",
                            "{label}"
                            if searching && count > 0 {
                                span { class: "settings-nav-match-count", "({count})" }
                            }
                        }
                    }
                }
            }
            // Plugin-provided settings pages — registered dynamically by active backends.
            // A group header separates them visually from the built-in sections.
            if !plugin_entries.is_empty() {
                {
                    let hide_class = if filter.is_empty() { "settings-nav-group-header" } else { "settings-nav-group-header settings-nav-group-hidden" };
                    rsx! {
                        div { class: hide_class,
                            "{t(\"settings-plugin-settings-nav-header\")}"
                        }
                    }
                }
            }
            for entry in &plugin_entries {
                {
                    let entry = *entry;
                    let label = t(entry.nav_label_key);
                    let slug = entry.slug;
                    let is_active = active_plugin.as_deref() == Some(slug);
                    let class = match (filter.is_empty(), is_active) {
                        (false, _) => "settings-nav-item settings-nav-item-hidden",
                        (_, true) => "settings-nav-item active",
                        _ => "settings-nav-item",
                    };
                    rsx! {
                        div {
                            class,
                            "data-settings-slug": "plugin-{slug}",
                            onclick: move |_| {
                                active_plugin_slug.set(Some(slug.to_string()));
                                app_state.write().settings_section = SettingsSection::Plugins;
                                scroll_to_section_anchor(&format!("plugin-{slug}"));
                                close_mobile_drawer();
                            },
                            span { class: "settings-nav-plugin-icon", "{entry.nav_icon}" }
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
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    // Snapshot plugin settings pages so the read lock is released before RSX.
    let plugin_entries: Vec<_> = client_manager.read().plugin_settings.to_vec();
    rsx! {
        for (_label_key, section) in NAV_SECTIONS {
            {
                let slug = section.to_slug();
                let id = format!("settings-section-{slug}");
                let has_match = section_has_search_match(section, &q);
                let searching = !q.is_empty();
                // Always render so scroll spy IDs remain in the DOM; hide via CSS
                let class = if searching && !has_match {
                    "settings-section-block settings-section-hidden"
                } else {
                    "settings-section-block"
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
                            SettingsSection::Layout => rsx! {
                                LayoutSettings {}
                            },
                            SettingsSection::General => rsx! {
                                GeneralSettings {}
                            },
                            SettingsSection::Plugins => rsx! {
                                PluginsSettings {}
                            },
                            SettingsSection::Diagnostics => rsx! {
                                DiagnosticsPage {}
                            },
                        }
                    }
                }
            }
        }
        // Dynamic plugin settings pages — appended after the last built-in section.
        // Divider is shown only when search is not active.
        if !plugin_entries.is_empty() && q.is_empty() {
            div { class: "settings-plugin-divider",
                span { class: "settings-plugin-divider-label",
                    "{t(\"settings-plugins-section-divider\")}"
                }
                span { class: "settings-plugin-divider-badge",
                    "{t(\"settings-plugins-badge\")}"
                }
            }
        }
        for entry in &plugin_entries {
            {
                let entry = *entry;
                let slug = entry.slug;
                let id = format!("settings-section-plugin-{slug}");
                let render_fn = entry.render;
                // Always render plugin sections; hide via CSS during search
                let class = if q.is_empty() {
                    "settings-section-block"
                } else {
                    "settings-section-block settings-section-hidden"
                };
                rsx! {
                    div { id, class,
                        { render_fn() }
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
    let locale_key = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    let active_plugin_slug = use_signal(|| None::<String>);
    let nav = use_navigator();
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();

    // Memo: stabilize plugin_section_ids so scroll spy effect doesn't re-run on every render.
    // Effect only re-runs when plugins actually change, not on unrelated state updates.
    let plugin_section_ids = use_memo(move || {
        client_manager
            .read()
            .plugin_settings
            .iter()
            .map(|entry| format!("settings-section-plugin-{}", entry.slug))
            .collect::<Vec<String>>()
    });

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
                if (el) el.scrollIntoView({{ block: 'start', behavior: 'smooth' }}); \
            }}, 0)"
        );
        let _ = document::eval(&js);
    });

    // Install scroll spy when plugins change; memoized plugin_section_ids prevents
    // spurious re-runs on unrelated state updates (e.g., search, section changes).
    use_effect(move || {
        install_settings_scroll_spy(app_state, (*plugin_section_ids.read()).clone(), active_plugin_slug);
    });

    // When the search query changes to non-empty, scroll the content area to the
    // top so the user sees filtered results from the beginning.
    use_effect(move || {
        let q = search_text.read().to_lowercase();
        if q.is_empty() {
            return;
        }
        let _ = document::eval(
            "{ const c = document.querySelector('.settings-content'); if (c) c.scrollTop = 0; }"
        );
    });

    let section = *section_memo.read();
    let query = search_text.read().clone();

    rsx! {
        SplitMenuShell {
            root_class: "settings-page".to_string(),
            sidebar_class: "settings-page-sidebar".to_string(),
            content_class: "settings-content".to_string(),
            sidebar: rsx! {
                SettingsNavigation {
                    key: "settings-nav-{locale_key}",
                    current: section,
                    search_text,
                    active_plugin_slug,
                    on_select: move |next: SettingsSection| {
                        *search_text.write() = String::new();
                        app_state.write().settings_section = next;
                        nav.push(Route::SettingsSectionRoute { section: next.to_slug().to_string() });
                    },
                }
            },
            content: rsx! {
                div { class: "settings-page-panel", key: "settings-panel-{locale_key}",
                    SettingsContentHeader { search_text }
                    div { class: "settings-sections-stack",
                        SettingsAllSections { search_query: query }
                    }
                }
            },
        }
    }
}
