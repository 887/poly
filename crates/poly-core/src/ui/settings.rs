//! Settings page — accounts, backup, identity, theme, language, appearance.

use crate::i18n::t;
use crate::state::{AppState, SettingsSection};
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
#[component]
fn ThemeSettings() -> Element {
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-theme\")}" }
            p { class: "settings-description", "{t(\"settings-theme-description\")}" }
            // Theme preset selector
            div { class: "theme-presets",
                label { "{t(\"settings-theme-preset\")}" }
                // TODO(phase-2.7.9.7): Theme preset selector, color editor, CSS editor
                select { class: "theme-select",
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
#[component]
fn LanguageSettings() -> Element {
    rsx! {
        div { class: "settings-section",
            h2 { "{t(\"settings-language\")}" }
            p { class: "settings-description", "{t(\"settings-language-description\")}" }
            select {
                class: "language-select",
                onchange: move |evt| {
                    crate::i18n::set_locale(&evt.value());
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
