//! Settings page — accounts, backup, identity, theme, language, appearance.

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use crate::theme::{ThemeConfig, ThemePreset};
use dioxus::prelude::*;

// ── Custom select component ───────────────────────────────────────────────────

/// A (value, display-label) pair for [`PolySelect`].
#[derive(Clone, PartialEq)]
struct SelectOption {
    value: &'static str,
    label: &'static str,
}

/// Fully themed dropdown select — replaces the ugly native `<select>`.
///
/// The native OS select popup ignores CSS custom properties; this component
/// renders entirely in the webview so it respects the active theme.
#[component]
fn PolySelect(
    options: Vec<SelectOption>,
    /// Currently selected value.
    value: String,
    /// Called with the new value string when the user picks an option.
    onchange: EventHandler<String>,
) -> Element {
    let mut open = use_signal(|| false);
    let current_label = options
        .iter()
        .find(|o| o.value == value)
        .map(|o| o.label)
        .unwrap_or(&value);

    rsx! {
        div { class: "poly-select",
            // Trigger button
            div {
                class: if *open.read() { "poly-select-trigger open" } else { "poly-select-trigger" },
                onclick: move |_| {
                    let v = *open.read();
                    open.set(!v);
                },
                span { class: "poly-select-current", "{current_label}" }
                span { class: "poly-select-chevron", "▾" }
            }
            // Options panel
            if *open.read() {
                div { class: "poly-select-menu",
                    for opt in &options {
                        {
                            let opt_value = opt.value;
                            let is_active = opt.value == value;
                            rsx! {
                                div {
                                    class: if is_active { "poly-select-option active" } else { "poly-select-option" },
                                    onclick: move |_| {
                                        open.set(false);
                                        onchange.call(opt_value.to_string());
                                    },
                                    "{opt.label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Settings page component.
///
/// Two-column layout: navigation sidebar + content area.
#[component]
pub fn SettingsPage(app_state: Signal<AppState>) -> Element {
    let section = app_state.read().settings_section;
    // Subscribe to locale signal so nav labels re-render on language change.
    let _locale = crate::i18n::use_locale().read().clone();

    rsx! {
        div { class: "settings-page",
            // Settings navigation
            nav { class: "settings-nav",
                SettingsNavItem {
                    label: t("settings-accounts"),
                    active: section == SettingsSection::Accounts,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Accounts,
                }
                SettingsNavItem {
                    label: t("settings-backup"),
                    active: section == SettingsSection::Backup,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Backup,
                }
                SettingsNavItem {
                    label: t("settings-identity"),
                    active: section == SettingsSection::Identity,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Identity,
                }
                SettingsNavItem {
                    label: t("settings-theme"),
                    active: section == SettingsSection::Theme,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Theme,
                }
                SettingsNavItem {
                    label: t("settings-language"),
                    active: section == SettingsSection::Language,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Language,
                }
                SettingsNavItem {
                    label: t("settings-appearance"),
                    active: section == SettingsSection::Appearance,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::Appearance,
                }
                SettingsNavItem {
                    label: t("settings-general"),
                    active: section == SettingsSection::General,
                    onclick: move |_| app_state.write().settings_section = SettingsSection::General,
                }
            }

            // Settings content
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
                    SettingsSection::Language => rsx! {
                        LanguageSettings {}
                    },
                    SettingsSection::Appearance => rsx! {
                        AppearanceSettings {}
                    },
                    SettingsSection::General => rsx! {
                        GeneralSettings {}
                    },
                }
            }
        }
    }
}

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

/// Accounts settings section.
#[component]
fn AccountsSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-accounts\")}" }
            p { class: "settings-description", "{t(\"settings-accounts-description\")}" }
            // TODO(phase-2.7.9.2): Account list grouped by backend
            button { class: "btn btn-primary", "{t(\"settings-add-account\")}" }
        }
    }
}

/// Backup servers settings section.
#[component]
fn BackupSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-backup\")}" }
            p { class: "settings-description", "{t(\"settings-backup-description\")}" }
            // TODO(phase-2.7.9.5): Backup server list
            button { class: "btn btn-primary", "{t(\"settings-add-backup\")}" }
        }
    }
}

/// Identity settings section.
#[component]
fn IdentitySettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-identity\")}" }
            p { class: "settings-description", "{t(\"settings-identity-description\")}" }
            // TODO(phase-2.7.9.6): Show public key, export recovery phrase
            div { class: "identity-info",
                label { "{t(\"settings-your-id\")}" }
                code { class: "account-id", "Loading..." }
            }
            button { class: "btn btn-secondary", "{t(\"settings-export-recovery\")}" }
        }
    }
}

/// Theme settings section.
///
/// Reads/writes the `Signal<ThemeConfig>` provided by [`crate::ui::App`].
/// Changing the preset updates the signal immediately (re-renders the
/// `<style id="poly-theme">` in App) and persists to storage.
#[component]
fn ThemeSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let mut theme_config = use_context::<Signal<ThemeConfig>>();

    let current_preset = match theme_config.read().preset {
        ThemePreset::NeutralDark => "neutral-dark",
        ThemePreset::Purple => "purple",
        ThemePreset::Red => "red",
        ThemePreset::Custom => "custom",
    };

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-theme\")}" }
            p { class: "settings-description", "{t(\"settings-theme-description\")}" }
            div { class: "theme-presets",
                label { class: "settings-label", "{t(\"settings-theme-preset\")}" }
                PolySelect {
                    options: vec![
                        SelectOption {
                            value: "neutral-dark",
                            label: "Neutral Dark",
                        },
                        SelectOption {
                            value: "purple",
                            label: "Purple",
                        },
                        SelectOption {
                            value: "red",
                            label: "Red",
                        },
                        SelectOption {
                            value: "custom",
                            label: "Custom",
                        },
                    ],
                    value: current_preset.to_string(),
                    onchange: move |new_val: String| {
                        let preset = match new_val.as_str() {
                            "purple" => ThemePreset::Purple,
                            "red" => ThemePreset::Red,
                            "custom" => ThemePreset::Custom,
                            _ => ThemePreset::NeutralDark,
                        };
                        let mut new_config = theme_config.read().clone();
                        new_config.preset = preset;
                        theme_config.set(new_config.clone());
                        spawn(async move {
                            if let Some(s) = crate::STORAGE.get() {
                                if let Err(e) = s.set_theme_config(&new_config).await {
                                    tracing::error!("Failed to persist theme config: {e}");
                                } else {
                                    tracing::info!("Theme config persisted ✓");
                                }
                            }
                        });
                    },
                }
            }
            div { class: "theme-actions",
                button { class: "btn btn-secondary", "{t(\"settings-theme-import\")}" }
                button { class: "btn btn-secondary", "{t(\"settings-theme-export\")}" }
            }
        }
    }
}

/// Language settings section.
///
/// The dropdown pre-selects the OS/browser-detected language (set during
/// [`crate::i18n::init`]) and switches the entire app's strings reactively
/// on change. Works identically on desktop (Wry) and web (WASM).
#[component]
fn LanguageSettings() -> Element {
    // Reads the locale Signal from context — subscribes to changes so the
    // selected option updates immediately when another part of the app
    // changes the locale.
    let mut locale_sig = crate::i18n::use_locale();
    let current_locale = locale_sig.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-language\")}" }
            p { class: "settings-description", "{t(\"settings-language-description\")}" }
            PolySelect {
                options: vec![
                    SelectOption {
                        value: "en",
                        label: "English",
                    },
                    SelectOption {
                        value: "de",
                        label: "Deutsch",
                    },
                    SelectOption {
                        value: "fr",
                        label: "Français",
                    },
                    SelectOption {
                        value: "es",
                        label: "Español",
                    },
                ],
                value: current_locale.clone(),
                onchange: move |new_locale: String| {
                    // Update global i18n state and re-render all subscribed
                    // components via the shared Signal.
                    crate::i18n::set_locale(&new_locale);
                    *locale_sig.write() = new_locale.clone();
                    // Persist (fire-and-forget).
                    spawn(async move {
                        if let Some(s) = crate::STORAGE.get() {
                            match s.get_app_settings().await {
                                Ok(mut settings) => {
                                    settings.locale = new_locale;
                                    if let Err(e) = s.set_app_settings(&settings).await {
                                        tracing::error!("Failed to persist locale: {e}");
                                    } else {
                                        tracing::info!("Locale persisted to storage ✓");
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to read settings for locale persist: {e}"
                                    )
                                }
                            }
                        }
                    });
                },
            }
        }
    }
}

/// Appearance settings section.
#[component]
fn AppearanceSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-appearance\")}" }
            p { class: "settings-description", "{t(\"settings-appearance-description\")}" }
            // TODO(phase-2.7.9.9): Dark/light mode toggle
            div { class: "appearance-options",
                label {
                    input {
                        r#type: "radio",
                        name: "color-mode",
                        value: "dark",
                        checked: true,
                    }
                    " {t(\"settings-dark-mode\")}"
                }
                label {
                    input { r#type: "radio", name: "color-mode", value: "light" }
                    " {t(\"settings-light-mode\")}"
                }
                label {
                    input {
                        r#type: "radio",
                        name: "color-mode",
                        value: "follow",
                    }
                    " {t(\"settings-follow-device\")}"
                }
            }
        }
    }
}

/// General settings section.
#[component]
fn GeneralSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-general\")}" }
            p { class: "settings-description", "{t(\"settings-general-description\")}" }
                // TODO(phase-2.7.9.10): Notification preferences, startup behavior
        }
    }
}
