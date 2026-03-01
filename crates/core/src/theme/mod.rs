//! Theme engine for Poly.
//!
//! Manages CSS custom properties for all colors, supports preset themes
//! (neutral-dark, purple, red), per-color customization, custom CSS editor,
//! and theme import/export.
//!
//! ## Built-in Presets
//! - **Neutral Dark** (default): Dark slate/charcoal, modern neutral tones
//! - **Purple** (Discord-inspired): Blurple accents, dark background
//! - **Red** (Stoat-inspired): Red/coral accents, dark background

// DECISION(D9): Every color configurable, import/export themes, full CSS editor.

use serde::{Deserialize, Serialize};

/// Available theme presets.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePreset {
    /// Dark slate/charcoal with neutral tones (default).
    #[default]
    NeutralDark,
    /// Discord-inspired with blurple accents.
    Purple,
    /// Stoat-inspired with red/coral accents.
    Red,
    /// User-defined custom theme.
    Custom,
}

/// Color mode preference.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorMode {
    /// Dark mode.
    #[default]
    Dark,
    /// Light mode.
    Light,
    /// Follow the device/OS preference.
    FollowDevice,
}

/// Complete theme configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Active theme preset.
    pub preset: ThemePreset,
    /// Color mode (dark/light/follow device).
    pub color_mode: ColorMode,
    /// Custom CSS overrides (applied after preset).
    pub custom_css: String,
    /// Per-color overrides (CSS variable name -> value).
    pub color_overrides: std::collections::HashMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: ThemePreset::NeutralDark,
            color_mode: ColorMode::Dark,
            custom_css: String::new(),
            color_overrides: std::collections::HashMap::new(),
        }
    }
}

/// Initialize the theme engine with the default theme.
pub fn init() {
    tracing::info!(
        "Theme engine initialized with {:?} preset",
        ThemePreset::NeutralDark
    );
}

/// Get the CSS for a given preset.
pub fn preset_css(preset: ThemePreset) -> &'static str {
    match preset {
        ThemePreset::NeutralDark => {
            include_str!("../../assets/styling/themes/neutral-dark.css")
        }
        ThemePreset::Purple => include_str!("../../assets/styling/themes/purple.css"),
        ThemePreset::Red => include_str!("../../assets/styling/themes/red.css"),
        ThemePreset::Custom => "",
    }
}

/// Generate the complete CSS string for a theme config.
///
/// Layers: preset CSS → color overrides → custom CSS.
pub fn generate_css(config: &ThemeConfig) -> String {
    let mut css = String::new();

    // 1. Base preset
    css.push_str(preset_css(config.preset));
    css.push('\n');

    // 2. Color overrides as CSS variables
    if !config.color_overrides.is_empty() {
        css.push_str(":root {\n");
        for (var, value) in &config.color_overrides {
            css.push_str(&format!("  {var}: {value};\n"));
        }
        css.push_str("}\n");
    }

    // 3. Custom CSS
    if !config.custom_css.is_empty() {
        css.push('\n');
        css.push_str(&config.custom_css);
    }

    css
}

/// Export a theme config to a CSS file string (for sharing).
pub fn export_theme(config: &ThemeConfig) -> String {
    let mut output = format!(
        "/* Poly Theme Export */\n/* Preset: {:?} */\n/* Color Mode: {:?} */\n\n",
        config.preset, config.color_mode
    );
    output.push_str(&generate_css(config));
    output
}
