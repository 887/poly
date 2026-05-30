//! Theme settings — presets, color mode, color overrides, and CSS editor.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::theme::{ThemeConfig, ThemePreset};
use crate::ui::actions::{ActionCx, UiAction};
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the theme settings section.
pub enum ThemeSettingsAction {
    /// Switch to a different built-in theme preset.
    SetPreset(ThemePreset),
    /// Change the color mode (dark / light / follow device).
    SetColorMode(crate::theme::ColorMode),
    /// Toggle color overrides on or off.
    SetColorOverridesEnabled(bool),
    /// Override a specific CSS color variable.
    SetColorOverride(String, String),
    /// Reset all color overrides to preset defaults.
    ResetColors,
    /// Toggle custom CSS injection on or off.
    SetCustomCssEnabled(bool),
    /// Apply (save) the current custom CSS.
    ApplyCss(String),
    /// Import a theme from a clipboard-formatted export string.
    ImportTheme(String),
    /// Export the current theme to the clipboard.
    ExportTheme,
}

impl UiAction for ThemeSettingsAction {
    fn apply(self, _cx: ActionCx<'_>) {
        match self {
            Self::SetPreset(_preset) => todo!("phase-E: persist theme preset"),
            Self::SetColorMode(_mode) => todo!("phase-E: persist color mode"),
            Self::SetColorOverridesEnabled(_enabled) => {
                todo!("phase-E: toggle color overrides")
            }
            Self::SetColorOverride(_var, _value) => todo!("phase-E: persist color override"),
            Self::ResetColors => todo!("phase-E: reset color overrides"),
            Self::SetCustomCssEnabled(_enabled) => todo!("phase-E: toggle custom CSS"),
            Self::ApplyCss(_css) => todo!("phase-E: apply custom CSS"),
            Self::ImportTheme(_exported) => todo!("phase-E: import theme from string"),
            Self::ExportTheme => todo!("phase-E: copy theme export to clipboard"),
        }
    }
}

/// Persist the theme config to storage (fire-and-forget).
async fn persist_theme(config: ThemeConfig) {
    if let Some(s) = crate::STORAGE.get() {
        if let Err(e) = s.set_theme_config(&config).await {
            tracing::error!("Failed to persist theme config: {e}");
        } else {
            tracing::info!("Theme config persisted ✓");
        }
    }
}

fn update_theme_config(
    theme_config: BatchedSignal<ThemeConfig>,
    update: impl FnOnce(&mut ThemeConfig),
) {
    let mut cfg = theme_config.read().clone();
    update(&mut cfg);
    let cfg_clone = cfg.clone();
    theme_config.batch(move |v| *v = cfg_clone);
    spawn(async move {
        persist_theme(cfg).await;
    });
}

fn resolved_color_value(config: &ThemeConfig, var_name: &str) -> String {
    config
        .color_overrides
        .get(var_name)
        .cloned()
        .unwrap_or_else(|| {
            crate::theme::extract_var_value(config.preset, config.color_mode, var_name)
                .unwrap_or_else(|| "#808080".to_string())
        })
}

fn initial_editor_css(config: &ThemeConfig) -> String {
    if config.custom_css.is_empty() {
        crate::theme::build_css_template(config)
    } else {
        config.custom_css.clone()
    }
}

/// Visual preset picker — colored buttons for each built-in theme.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ThemePresetPicker(theme_config: BatchedSignal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let current = theme_config.read().preset.canonical();
    const PRESETS: &[(ThemePreset, &str, &str)] = &[
        (ThemePreset::Blue, "blue", "theme-blue"),
        (ThemePreset::Purple, "purple", "theme-purple"),
        (ThemePreset::Red, "red", "theme-red"),
        (ThemePreset::Green, "green", "theme-green"),
        (ThemePreset::Monotone, "monotone", "theme-monotone"),
    ];
    rsx! {
        div { class: "theme-section",
            label { class: "settings-label", "{t(\"settings-theme-preset\")}" }
            div { class: "theme-preset-row",
                for (preset , data_name , i18n_key) in PRESETS {
                    {
                        let preset = *preset;
                        let data_name = *data_name;
                        let i18n_key = *i18n_key;
                        let is_active = current == preset;
                        rsx! {
                            button {
                                class: if is_active { "theme-preset-btn active" } else { "theme-preset-btn" },
                                "data-preset": data_name,
                                onclick: move |_| {
                                    let mut cfg = theme_config.read().clone();
                                    cfg.preset = preset;
                                    let cfg_clone = cfg.clone();
                                    theme_config.batch(move |v| *v = cfg_clone);
                                    spawn(async move {
                                        persist_theme(cfg).await;
                                    });
                                },
                                "{t(i18n_key)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Dark / Light / Follow Device toggle.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ThemeColorModeSelector(theme_config: BatchedSignal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let current = theme_config.read().color_mode;
    const MODES: &[(crate::theme::ColorMode, &str)] = &[
        (crate::theme::ColorMode::Dark, "settings-dark-mode"),
        (crate::theme::ColorMode::Light, "settings-light-mode"),
        (
            crate::theme::ColorMode::FollowDevice,
            "settings-follow-device",
        ),
    ];
    rsx! {
        div { class: "theme-section",
            label { class: "settings-label", "{t(\"settings-color-mode\")}" }
            div { class: "color-mode-row",
                for (mode , key) in MODES {
                    {
                        let mode = *mode;
                        let key = *key;
                        let is_active = current == mode;
                        rsx! {
                            button {
                                class: if is_active { "btn btn-sm color-mode-btn active" } else { "btn btn-sm color-mode-btn" },
                                onclick: move |_| {
                                    let mut cfg = theme_config.read().clone();
                                    cfg.color_mode = mode;
                                    let cfg_clone = cfg.clone();
                                    theme_config.batch(move |v| *v = cfg_clone);
                                    spawn(async move {
                                        persist_theme(cfg).await;
                                    });
                                },
                                "{t(key)}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Color pickers for the six most impactful CSS variables.
///
/// When disabled (default), the color pickers are greyed out and no
/// color overrides are applied. When enabled, users can customize
/// individual colors which then override the preset.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ThemeColorCustomizer(theme_config: BatchedSignal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let vars: Vec<(&str, String)> = vec![
        ("--accent-primary", t("color-accent")),
        ("--bg-primary", t("color-background")),
        ("--bg-surface", t("color-surface")),
        ("--text-primary", t("color-text")),
        ("--text-secondary", t("color-secondary-text")),
        ("--border-primary", t("color-border")),
        ("--favorites-bar-bg", t("color-favorites-bar")),
        ("--account-bar-bg", t("color-account-bar")),
    ];
    let config = theme_config.read().clone();
    let colors_enabled = config.color_overrides_enabled;
    let color_entries: Vec<(String, String, String)> = vars
        .iter()
        .map(|(var_name, label)| {
            (
                (*var_name).to_string(),
                label.clone(),
                resolved_color_value(&config, var_name),
            )
        })
        .collect();

    rsx! {
        div { class: "theme-section",
            ColorOverridesToggleRow {
                colors_enabled,
                on_toggle: move |enabled| {
                    update_theme_config(theme_config, |cfg| cfg.color_overrides_enabled = enabled);
                },
            }
            p { class: "colors-hint", "{t(\"settings-color-hint\")}" }
            ColorOverridesGrid {
                entries: color_entries,
                colors_enabled,
                theme_config,
            }
            ResetColorsButton {
                on_reset: move |_| {
                    update_theme_config(theme_config, |cfg| cfg.color_overrides.clear());
                },
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ColorOverridesToggleRow(colors_enabled: bool, on_toggle: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "colors-toggle-row",
            label { class: "settings-label", "{t(\"settings-color-overrides\")}" }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: colors_enabled,
                    onchange: move |e| on_toggle.call(e.checked()),
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ColorOverridesGrid(
    entries: Vec<(String, String, String)>,
    colors_enabled: bool,
    theme_config: BatchedSignal<ThemeConfig>,
) -> Element {
    rsx! {
        div { class: "color-overrides-grid",
            for (var_name , display_label , cur) in &entries {
                {
                    let var_name = var_name.clone();
                    let display_label = display_label.clone();
                    let cur = cur.clone();
                    rsx! {
                        div { class: "color-override-item",
                            label { class: "color-override-label", "{display_label}" }
                            input {
                                r#type: "color",
                                class: if colors_enabled { "color-picker" } else { "color-picker color-picker-disabled" },
                                disabled: !colors_enabled,
                                value: cur,
                                oninput: move |e| {
                                    if colors_enabled {
                                        let color = e.value();
                                        update_theme_config(
                                            theme_config,
                                            |cfg| {
                                                cfg.color_overrides.insert(var_name.clone(), color);
                                            },
                                        );
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn ResetColorsButton(on_reset: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "theme-actions",
            button {
                class: "btn btn-secondary",
                onclick: move |evt| on_reset.call(evt),
                {t("settings-reset-colors")}
            }
        }
    }
}

/// CSS editor with enable toggle, pre-populated variable template, and
/// import/export controls.
///
/// When disabled (default), the editor is visible but greyed out and
/// the CSS is not injected. The template lists every CSS variable
/// (commented out) so users can see what is available.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ThemeCssEditor(theme_config: BatchedSignal<ThemeConfig>) -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let config = theme_config.read().clone();
    let local_css = use_signal(|| initial_editor_css(&config));
    let css_enabled = config.custom_css_enabled;

    rsx! {
        div { class: "theme-section",
            CssEditorToggleRow {
                css_enabled,
                on_toggle: move |enabled| {
                    update_theme_config(theme_config, |cfg| cfg.custom_css_enabled = enabled);
                },
            }
            p { class: "css-hint", "{t(\"settings-css-hint\")}" }
            CssEditorArea { css_enabled, local_css, theme_config }
            CssEditorActions { local_css, theme_config }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn CssEditorToggleRow(css_enabled: bool, on_toggle: EventHandler<bool>) -> Element {
    rsx! {
        div { class: "css-toggle-row",
            label { class: "settings-label", "{t(\"settings-theme-custom-css\")}" }
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    checked: css_enabled,
                    onchange: move |e| on_toggle.call(e.checked()),
                }
                span { class: "toggle-slider" }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(allow_default)]
#[component]
fn CssEditorArea(
    css_enabled: bool,
    local_css: Signal<String>,
    theme_config: BatchedSignal<ThemeConfig>,
) -> Element {
    rsx! {
        textarea {
            class: if css_enabled { "css-editor" } else { "css-editor css-editor-disabled" },
            rows: 14,
            value: local_css.read().clone(),
            oninput: move |e| local_css.set(e.value()),
            onblur: move |_| {
                let css = local_css.read().clone();
                update_theme_config(theme_config, |cfg| cfg.custom_css = css);
            },
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn CssEditorActions(local_css: Signal<String>, theme_config: BatchedSignal<ThemeConfig>) -> Element {
    rsx! {
        div { class: "theme-actions",
            button {
                class: "btn btn-secondary",
                onclick: move |_| {
                    let css = local_css.read().clone();
                    update_theme_config(theme_config, |cfg| cfg.custom_css = css);
                },
                "{t(\"settings-theme-apply-css\")}"
            }
            button {
                class: "btn btn-secondary",
                onclick: move |_| {
                    let exported = crate::theme::export_theme(&theme_config.read());
                    let js = format!(
                        "navigator.clipboard.writeText({exported:?}).catch(()=>{{}})",
                    );
                    let _ = document::eval(&js);
                },
                "{t(\"settings-theme-export\")}"
            }
            button {
                class: "btn btn-secondary",
                onclick: move |_| {
                    spawn(async move {
                        let mut eval = document::eval(
                            "navigator.clipboard.readText().then(t=>dioxus.send(t)).catch(()=>dioxus.send(''))",
                        );
                        if let Ok(val) = eval.recv::<serde_json::Value>().await
                            && let Some(s) = val.as_str()
                        {
                            let imported = crate::theme::import_theme(s);
                            local_css.set(initial_editor_css(&imported));
                            let imp_clone = imported.clone();
                            theme_config.batch(move |v| *v = imp_clone);
                            persist_theme(imported).await;
                        }
                    });
                },
                "{t(\"settings-theme-import\")}"
            }
            button {
                class: "btn btn-secondary",
                onclick: move |_| {
                    let template = crate::theme::build_css_template(&theme_config.read());
                    local_css.set(template);
                },
                "{t(\"settings-css-reset-template\")}"
            }
        }
    }
}

/// Theme settings page — presets, color mode, color overrides, and CSS editor.
///
/// Replaces the separate Appearance page: everything color/theme related
/// is now in one place.
#[rustfmt::skip]
#[ui_action(ThemeSettingsAction)]
#[context_menu(none)]
#[component]
pub(super) fn ThemeSettings() -> Element {
    let _locale = crate::i18n::use_locale().read().clone();
    let theme_config = use_context::<BatchedSignal<ThemeConfig>>();
    rsx! {
        div { class: "settings-section theme-settings",
            h2 { "{t(\"settings-theme\")}" }
            p { class: "settings-description", "{t(\"settings-theme-description\")}" }
            ThemePresetPicker { theme_config }
            ThemeColorModeSelector { theme_config }
            ThemeColorCustomizer { theme_config }
            ThemeCssEditor { theme_config }
        }
    }
}
