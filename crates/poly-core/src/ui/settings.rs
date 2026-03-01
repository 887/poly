//! Settings page — accounts, backup, identity, theme, language, appearance.

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
use crate::theme::{ThemeConfig, ThemePreset};
use dioxus::prelude::*;

/// Settings page component.
///
/// Two-column layout: navigation sidebar + content area.
#[component]
pub fn SettingsPage(app_state: Signal<AppState>) -> Element {
    let section = app_state.read().settings_section;

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
    let mut theme_config = use_context::<Signal<ThemeConfig>>();

    // Derive the current preset string for the select's selected option.
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
            // Theme preset selector
            div { class: "theme-presets",
                label { "{t(\"settings-theme-preset\")}" }
                select {
                    class: "theme-select",
                    value: "{current_preset}",
                    onchange: move |evt| {
                        let preset = match evt.value().as_str() {
                            "purple" => ThemePreset::Purple,
                            "red" => ThemePreset::Red,
                            "custom" => ThemePreset::Custom,
                            _ => ThemePreset::NeutralDark,
                        };
                        // Update context signal → App re-renders <style> with new CSS.
                        let mut new_config = theme_config.read().clone();
                        new_config.preset = preset;
                        theme_config.set(new_config.clone());
                        // Persist async (fire-and-forget).
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
                    option { value: "neutral-dark", "{t(\"theme-neutral-dark\")}" }
                    option { value: "purple", "{t(\"theme-purple\")}" }
                    option { value: "red", "{t(\"theme-red\")}" }
                    option { value: "custom", "{t(\"theme-custom\")}" }
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
/// Uses [`crate::i18n::use_locale`] for reactive locale switching:
/// changing the language immediately re-renders all translated strings
/// across the app (because all components share the locale `Signal`).
/// The new locale is also persisted to `AppSettings` in storage.
#[component]
fn LanguageSettings() -> Element {
    let (locale_sig, mut set_locale_fn) = crate::i18n::use_locale();
    // Reading the signal here subscribes this component to locale changes.
    let current_locale = locale_sig.read().clone();

    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-language\")}" }
            p { class: "settings-description", "{t(\"settings-language-description\")}" }
            select {
                class: "language-select",
                value: "{current_locale}",
                onchange: move |evt| {
                    let new_locale = evt.value();
                    // Update global state + trigger re-render via signal.
                    set_locale_fn(&new_locale);
                    // Persist the new locale to AppSettings (fire-and-forget).
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
                                    );
                                }
                            }
                        }
                    });
                },
                option { value: "en", "English" }
                option { value: "de", "Deutsch" }
                option { value: "fr", "Français" }
                option { value: "es", "Español" }
            }
        }
    }
}

/// Appearance settings section.
#[component]
fn AppearanceSettings() -> Element {
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
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-general\")}" }
            p { class: "settings-description", "{t(\"settings-general-description\")}" }
                // TODO(phase-2.7.9.10): Notification preferences, startup behavior
        }
    }
}
