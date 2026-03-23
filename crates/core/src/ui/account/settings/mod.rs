//! Account-scoped settings page.
//!
//! A focused settings view that shows only account-specific preferences.
//! Unlike the app-level `SettingsPage`, this page scopes everything to
//! one account and omits global concerns (theme, language, identity, etc.).
//!
//! Voice & Audio settings are in the app-level `SettingsPage` (Voice & Video section).
//!
//! ## Sections
//! | Section | Component |
//! |---|---|
//! | Notifications | `NotificationsSettings` — per-account notification toggles |
//!
//! ## 150-line component rule
//! Every `#[component]` fn body in this module MUST stay under **150 lines**
//! of RSX + logic. Extract sub-components rather than growing any file.

mod content_social;
mod notifications;

#[cfg(feature = "server")]
mod profile;

use crate::i18n::t;
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::settings::scroll_spy::scroll_to_settings_section;
#[cfg(target_arch = "wasm32")]
use crate::ui::settings::scroll_spy::{
    SettingsScrollSpyConfig, install_settings_scroll_spy as install_shared_settings_scroll_spy,
};
use crate::ui::split_shell::SplitMenuShell;
use content_social::ContentSocialSettings;
use dioxus::prelude::*;
use notifications::NotificationsSettings;

#[cfg(feature = "server")]
use profile::PolyProfileSettings;

/// Render the profile nav item when server feature is active and backend is "poly".
#[cfg(feature = "server")]
fn profile_nav_element(
    show: bool,
    is_active: bool,
    mut search_text: Signal<String>,
    mut active_section: Signal<String>,
) -> Element {
    if !show {
        return rsx! {};
    }
    let class = if is_active {
        "settings-nav-item active"
    } else {
        "settings-nav-item"
    };
    rsx! {
        div {
            class,
            onclick: move |_| {
                *search_text.write() = String::new();
                active_section.set("profile".to_string());
                close_mobile_drawer();
            },
            {t("plugin-poly-profile-title")}
        }
    }
}

/// No-op when server feature is disabled.
#[cfg(not(feature = "server"))]
fn profile_nav_element(
    _show: bool,
    _is_active: bool,
    _search_text: Signal<String>,
    _active_section: Signal<String>,
) -> Element {
    rsx! {}
}

/// Render the profile settings section when server feature is active and backend is "poly".
#[cfg(feature = "server")]
fn profile_section_element(show: bool, account_id: String) -> Element {
    if !show {
        return rsx! {};
    }
    rsx! {
        div { id: "acct-section-profile", class: "settings-section-block",
            PolyProfileSettings { account_id }
        }
    }
}

/// No-op when server feature is disabled.
#[cfg(not(feature = "server"))]
fn profile_section_element(_show: bool, _account_id: String) -> Element {
    rsx! {}
}

/// Account-specific searchable settings nodes.
/// Format: (i18n key, section slug).
const ACCT_NAV_SECTIONS: &[(&str, &str)] = &[
    ("settings-notifications", "notifications"),
    ("settings-content-social", "content-social"),
];

/// Returns true if any node for this account section matches the query.
fn acct_section_has_match(section_slug: &str, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    ACCT_NAV_SECTIONS
        .iter()
        .any(|(key, slug)| *slug == section_slug && t(key).to_lowercase().contains(q))
}

/// Fire-and-forget JS: smooth-scroll the account settings content area to a section.
fn scroll_to_acct_section(slug: &str) {
    scroll_to_settings_section("acct-section-", slug);
}

fn install_account_settings_scroll_spy(_active_section: Signal<String>, _show_profile: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let mut active_section = _active_section;
        let show_profile = _show_profile;
        let mut section_ids = vec![
            "acct-section-notifications".to_string(),
            "acct-section-content-social".to_string(),
        ];
        if show_profile {
            section_ids.insert(0, "acct-section-profile".to_string());
        }
        let config = SettingsScrollSpyConfig {
            runtime_flag: "__polyAccountSettingsScrollSpyInstalled",
            content_selector: ".settings-content",
            section_prefix: "acct-section-",
            section_ids,
            plugin_section_prefix: None,
        };
        install_shared_settings_scroll_spy(config, move |slug| {
            if active_section.read().as_str() != slug {
                active_section.set(slug);
            }
        });
    }
}

#[rustfmt::skip]
#[component]
fn AccountSettingsSearchBar(search_text: Signal<String>) -> Element {
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
fn AccountSettingsContentHeader(account_id: String, search_text: Signal<String>) -> Element {
    let account_id_upper = account_id.to_uppercase();

    rsx! {
        div { class: "special-page-header settings-page-header",
            h2 { class: "special-page-title", "{t(\"account-settings-title\")}" }
            p { class: "settings-description", "{account_id_upper}" }
            AccountSettingsSearchBar { search_text }
        }
    }
}

/// Account-scoped settings page.
///
/// VS Code-style single-scroll layout: account-specific sections are rendered
/// in a scrollable column. The nav sidebar shows a header (account name) and
/// section items that scroll the content on click. Search dims non-matching
/// sections.
///
/// Global settings (theme, language, voice/video, identity, backup) are handled
/// by the app-level `SettingsPage`.
#[rustfmt::skip]
#[component]
pub fn AccountSettingsPage(backend: String, account_id: String) -> Element {
    // Subscribe to locale so labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();
    let mut search_text = use_signal(String::new);
    let mut active_section = use_signal(|| "notifications".to_string());

    // Whether to show the poly-server Profile tab.
    // Compile-time: only available with the "server" feature.
    // Runtime: only when the active backend is "poly".
    #[cfg(feature = "server")]
    let show_profile = backend == "poly";
    #[cfg(not(feature = "server"))]
    let show_profile = false;

    // Scroll to active section when it changes (inc. initial render).
    use_effect(move || {
        let slug = active_section.read().clone();
        scroll_to_acct_section(&slug);
    });

    use_effect(move || {
        install_account_settings_scroll_spy(active_section, show_profile);
    });

    // When search changes, scroll to first matching section.
    use_effect(move || {
        let q = search_text.read().to_lowercase();
        if q.is_empty() {
            return;
        }
        if let Some((_, slug)) = ACCT_NAV_SECTIONS.iter().find(|(_, slug)| acct_section_has_match(slug, &q)) {
            scroll_to_acct_section(slug);
            active_section.set(slug.to_string());
        }
    });

    let sf = search_text.read().to_lowercase();
    let active = active_section.read().clone();
    let is_profile_active = active == "profile";

    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: "settings-content".to_string(),
            sidebar: rsx! {
                nav { class: "settings-nav",
                    div { class: "settings-nav-header",
                        h3 { class: "settings-nav-title", "{t(\"account-settings-title\")}" }
                    }
                    { profile_nav_element(show_profile, is_profile_active, search_text, active_section) }
                    for (label_key, slug) in ACCT_NAV_SECTIONS {
                        {
                            let label = t(label_key);
                            let has_match = acct_section_has_match(slug, &sf);
                            let is_active = active == *slug;
                            let class = match (is_active, has_match) {
                                (true, _) => "settings-nav-item active",
                                (false, true) => "settings-nav-item",
                                (false, false) => "settings-nav-item settings-nav-item-dimmed",
                            };
                            let slug_s = slug.to_string();
                            rsx! {
                                div {
                                    class,
                                    onclick: move |_| {
                                        *search_text.write() = String::new();
                                        active_section.set(slug_s.clone());
                                        close_mobile_drawer();
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                VoiceAccountFooter {}
            },
            content: rsx! {
                div { class: "settings-page-panel",
                    AccountSettingsContentHeader {
                        account_id: account_id.clone(),
                        search_text,
                    }
                    div { class: "settings-sections-stack",
                        { profile_section_element(show_profile, account_id.clone()) }
                        div {
                            id: "acct-section-notifications",
                            class: if acct_section_has_match("notifications", &sf) { "settings-section-block" } else { "settings-section-block settings-section-dimmed" },
                            NotificationsSettings { account_id: account_id.clone() }
                        }
                        div {
                            id: "acct-section-content-social",
                            class: if acct_section_has_match("content-social", &sf) { "settings-section-block" } else { "settings-section-block settings-section-dimmed" },
                            ContentSocialSettings { _account_id: account_id.clone() }
                        }
                    }
                }
            },
        }
    }
}
