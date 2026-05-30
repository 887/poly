//! Theme engine for Poly.
//!
//! Manages CSS custom properties for all colors, supports preset themes
//! with both dark and light variants, per-color customization via
//! color pickers, a togglable CSS editor with variable reference, and
//! theme import/export.
//!
//! ## Built-in Presets
//! - **Blue** (default): Blue accents, modern slate tones
//! - **Purple** (Discord-inspired): Blurple accents
//! - **Red** (Stoat-inspired): Red/coral accents
//! - **Green**: Nature-inspired green accents
//! - **Monotone**: Pure black/white, no accent colors

// DECISION(D9): Every color configurable, import/export themes, full CSS editor.
// DECISION(D9b): Each preset has dark+light variant; Monotone replaces Custom.

use std::fmt::Write as _;
use std::fmt::Write as _;
use serde::{Deserialize, Serialize};

/// Available theme presets.
///
/// Every preset provides both a dark and a light variant.
/// `Monotone` is a colorless black-or-white base for fully custom themes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePreset {
    /// Blue accents with modern slate tones (default).
    #[default]
    Blue,
    /// Discord-inspired with blurple accents.
    Purple,
    /// Stoat-inspired with red/coral accents.
    Red,
    /// Nature-inspired green accents.
    Green,
    /// Neutral black/white — no accent colors.
    Monotone,

    // Legacy variants kept for deserialization compatibility.
    // They map to the same themes at runtime.
    /// Legacy alias for [`Blue`].
    #[serde(alias = "NeutralDark")]
    #[doc(hidden)]
    NeutralDark,
    /// Legacy alias — maps to [`Blue`] at runtime.
    #[serde(alias = "Bright")]
    #[doc(hidden)]
    Bright,
    /// Legacy alias — maps to [`Monotone`] at runtime.
    #[serde(alias = "Custom")]
    #[doc(hidden)]
    Custom,
}

impl ThemePreset {
    /// Normalize legacy variants to their canonical form.
    #[must_use]
    pub fn canonical(self) -> Self {
        match self {
            Self::NeutralDark | Self::Bright => Self::Blue,
            Self::Custom => Self::Monotone,
            other @ (Self::Blue | Self::Purple | Self::Red | Self::Green | Self::Monotone) => other,
        }
    }
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
    /// Whether custom CSS overrides are applied.
    pub custom_css_enabled: bool,
    /// Custom CSS overrides (applied after preset when enabled).
    pub custom_css: String,
    /// Whether per-color overrides are applied.
    pub color_overrides_enabled: bool,
    /// Per-color overrides (CSS variable name -> value).
    pub color_overrides: std::collections::HashMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: ThemePreset::Blue,
            color_mode: ColorMode::Dark,
            custom_css_enabled: false,
            custom_css: String::new(),
            color_overrides_enabled: false,
            color_overrides: std::collections::HashMap::new(),
        }
    }
}

/// Initialize the theme engine with the default theme.
pub fn init() {
    tracing::info!(
        "Theme engine initialized with {:?} preset",
        ThemePreset::Blue
    );
}

/// All CSS variable names that themes expose, grouped with descriptions.
///
/// Used by the CSS editor template and by export/import.
pub const ALL_CSS_VARS: &[(&str, &str)] = &[
    // Backgrounds
    ("--bg-primary", "Main background"),
    ("--bg-secondary", "Sidebar / secondary panels"),
    ("--bg-tertiary", "Tertiary panels"),
    ("--bg-surface", "Cards, elevated surfaces"),
    ("--bg-hover", "Hover state backgrounds"),
    ("--bg-active", "Active / selected backgrounds"),
    ("--bg-input", "Input field backgrounds"),
    // Text
    ("--text-primary", "Main text color"),
    ("--text-secondary", "Secondary / muted text"),
    ("--text-muted", "Faint helper text"),
    ("--text-inverse", "Text on accent backgrounds"),
    ("--text-link", "Hyperlink color"),
    // Borders
    ("--border-primary", "Dividers, borders"),
    ("--border-secondary", "Subtle secondary borders"),
    ("--border-focus", "Focused-element ring"),
    // Accents
    ("--accent-primary", "Buttons, links, active highlights"),
    ("--accent-primary-hover", "Hovered accent elements"),
    ("--accent-secondary", "Secondary accent color"),
    ("--accent-success", "Success / online indicators"),
    ("--accent-warning", "Warning indicators"),
    ("--accent-danger", "Danger / error indicators"),
    // Sidebar
    ("--sidebar-bg", "Sidebar background"),
    ("--sidebar-icon-bg", "Sidebar icon background"),
    ("--sidebar-icon-hover", "Sidebar icon hover"),
    ("--sidebar-icon-active", "Sidebar active icon"),
    ("--sidebar-separator", "Sidebar divider line"),
    ("--favorites-bar-bg", "Favorites bar (Bar 1) background"),
    ("--account-bar-bg", "Account server bar (Bar 2) background"),
    // Chat
    ("--chat-bg", "Chat area background"),
    ("--chat-message-hover", "Hovered message row"),
    ("--chat-input-bg", "Chat input background"),
    ("--chat-input-border", "Chat input border"),
    // Scrollbar
    ("--scrollbar-thumb", "Scrollbar thumb color"),
    ("--scrollbar-track", "Scrollbar track color"),
    // Badge
    ("--badge-bg", "Notification badge background"),
    ("--badge-text", "Notification badge text"),
];

/// Get the raw CSS for a preset in a given color mode.
///
/// For [`ColorMode::FollowDevice`] this returns **both** variants wrapped
/// in `@media (prefers-color-scheme: …)` blocks so the browser can choose.
#[must_use] 
pub fn preset_css(preset: ThemePreset, mode: ColorMode) -> String {
    let preset = preset.canonical();
    match mode {
        ColorMode::Dark => dark_css(preset).to_string(),
        ColorMode::Light => light_css(preset).to_string(),
        ColorMode::FollowDevice => {
            // Wrap each variant in a media query so the OS picks the right one.
            let dark = dark_css(preset);
            let light = light_css(preset);
            format!(
                "@media (prefers-color-scheme: dark) {{\n{dark}\n}}\n\
                 @media (prefers-color-scheme: light) {{\n{light}\n}}\n"
            )
        }
    }
}

/// Dark-mode CSS for a (canonical) preset.
fn dark_css(preset: ThemePreset) -> &'static str {
    match preset {
        ThemePreset::Blue | ThemePreset::NeutralDark | ThemePreset::Bright => {
            include_str!("../assets/styling/themes/blue-dark.css")
        }
        ThemePreset::Purple => include_str!("../assets/styling/themes/purple-dark.css"),
        ThemePreset::Red => include_str!("../assets/styling/themes/red-dark.css"),
        ThemePreset::Green => include_str!("../assets/styling/themes/green-dark.css"),
        ThemePreset::Monotone | ThemePreset::Custom => {
            include_str!("../assets/styling/themes/monotone-dark.css")
        }
    }
}

/// Light-mode CSS for a (canonical) preset.
fn light_css(preset: ThemePreset) -> &'static str {
    match preset {
        ThemePreset::Blue | ThemePreset::NeutralDark | ThemePreset::Bright => {
            include_str!("../assets/styling/themes/blue-light.css")
        }
        ThemePreset::Purple => include_str!("../assets/styling/themes/purple-light.css"),
        ThemePreset::Red => include_str!("../assets/styling/themes/red-light.css"),
        ThemePreset::Green => include_str!("../assets/styling/themes/green-light.css"),
        ThemePreset::Monotone | ThemePreset::Custom => {
            include_str!("../assets/styling/themes/monotone-light.css")
        }
    }
}

/// Extract the current value of a CSS variable from the active preset CSS.
///
/// Scans the dark-mode base CSS for `--var-name: <value>;` and returns the
/// value (trimmed). Returns `None` if not found.
#[must_use] 
pub fn extract_var_value(preset: ThemePreset, mode: ColorMode, var_name: &str) -> Option<String> {
    let css = match mode {
        ColorMode::Dark | ColorMode::FollowDevice => dark_css(preset.canonical()),
        ColorMode::Light => light_css(preset.canonical()),
    };
    let needle = format!("{var_name}:");
    for line in css.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&needle) {
            let val = rest.trim().trim_end_matches(';').trim();
            return Some(val.to_string());
        }
    }
    None
}

/// Generate the complete CSS string for a theme config.
///
/// Layers: preset CSS → color overrides → custom CSS (if enabled).
#[must_use] 
pub fn generate_css(config: &ThemeConfig) -> String {
    let mut css = String::new();

    // 1. Base preset (dark / light / follow-device)
    css.push_str(&preset_css(config.preset, config.color_mode));
    css.push('\n');

    // 2. Color overrides as CSS variables (only if toggle is on)
    if config.color_overrides_enabled && !config.color_overrides.is_empty() {
        css.push_str(":root {\n");
        for (var, value) in &config.color_overrides {
            css.push_str(&format!("  {var}: {value};\n"));
        }
        css.push_str("}\n");
    }

    // 3. Custom CSS (only if toggle is on)
    if config.custom_css_enabled && !config.custom_css.is_empty() {
        css.push('\n');
        css.push_str(&config.custom_css);
    }

    css
}

/// Build the CSS editor template for a given theme.
///
/// All variables from the current preset are listed (commented out),
/// each with a description. The user can uncomment lines to override.
/// Color-picker overrides appear as uncommented lines.
#[must_use] 
pub fn build_css_template(config: &ThemeConfig) -> String {
    let mut out = String::from(
        "/* ═══════════════════════════════════════\n\
         *  Poly Theme CSS Editor\n\
         *\n\
         *  All available CSS variables are listed\n\
         *  below. Uncomment a line to override it.\n\
         *  Color pickers sync with these variables.\n\
         *\n\
         *  Variables use CSS custom properties:\n\
         *    var(--accent-primary)  in your rules.\n\
         * ═══════════════════════════════════════ */\n\n\
         :root {\n",
    );

    for &(var, desc) in ALL_CSS_VARS {
        let preset_val = extract_var_value(config.preset, config.color_mode, var)
            .unwrap_or_else(|| "#808080".into());
        if let Some(override_val) = config.color_overrides.get(var) {
            // Active override — uncommented
            out.push_str(&format!("  {var}: {override_val};  /* {desc} */\n"));
        } else {
            // Default — commented out so user can see the value
            out.push_str(&format!("  /* {var}: {preset_val};  {desc} */\n"));
        }
    }

    out.push_str(
        "}\n\n\
         /* ═══ Element Override Examples ═══\n\
         *  .btn {{ border-radius: 20px; }}\n\
         *  .server-sidebar {{ width: 80px; }}\n\
         *  .chat-input {{ font-size: 16px; }}\n\
         *  .settings-nav {{ background: var(--bg-secondary); }}\n\
         */\n",
    );

    out
}

/// Export marker prefix used inside comments for variable overrides.
const EXPORT_PREFIX: &str = "@poly-var";

/// Export a theme to a shareable string.
///
/// Overrides are written as `/* @poly-var --name: value */` comments so
/// import can reconstruct them without touching the CSS body.
#[must_use] 
pub fn export_theme(config: &ThemeConfig) -> String {
    let mut output = format!(
        "/* Poly Theme Export */\n\
         /* @poly-preset: {:?} */\n\
         /* @poly-mode: {:?} */\n\
         /* @poly-css-enabled: {} */\n\
         /* @poly-color-overrides-enabled: {} */\n\n",
        config.preset.canonical(),
        config.color_mode,
        config.custom_css_enabled,
        config.color_overrides_enabled,
    );

    // Write overrides as magic comments
    for (var, value) in &config.color_overrides {
        output.push_str(&format!("/* {EXPORT_PREFIX} {var}: {value} */\n"));
    }

    // Include custom CSS body (always, even if toggled off — so it survives round-trip)
    if !config.custom_css.is_empty() {
        output.push_str("\n/* @poly-custom-css-start */\n");
        output.push_str(&config.custom_css);
        output.push_str("\n/* @poly-custom-css-end */\n");
    }

    output
}

/// Import a theme from a previously-exported string.
///
/// Parses `@poly-var`, `@poly-preset`, `@poly-mode` comments and the
/// custom CSS block. Returns an updated [`ThemeConfig`].
#[must_use] 
pub fn import_theme(exported: &str) -> ThemeConfig {
    let mut config = ThemeConfig::default();
    let mut in_css_block = false;
    let mut custom_css = String::new();

    for line in exported.lines() {
        let trimmed = line.trim();

        // Preset
        if let Some(rest) = trimmed.strip_prefix("/* @poly-preset:") {
            let val = rest.trim().trim_end_matches("*/").trim();
            config.preset = match val {
                "Purple" => ThemePreset::Purple,
                "Red" => ThemePreset::Red,
                "Green" => ThemePreset::Green,
                "Monotone" => ThemePreset::Monotone,
                // "Blue" + unknown both fall through to Blue (the default preset).
                _ => ThemePreset::Blue,
            };
            continue;
        }

        // Color mode
        if let Some(rest) = trimmed.strip_prefix("/* @poly-mode:") {
            let val = rest.trim().trim_end_matches("*/").trim();
            config.color_mode = match val {
                "Light" => ColorMode::Light,
                "FollowDevice" => ColorMode::FollowDevice,
                // "Dark" + unknown both fall through to Dark (the default mode).
                _ => ColorMode::Dark,
            };
            continue;
        }

        // CSS enabled
        if let Some(rest) = trimmed.strip_prefix("/* @poly-css-enabled:") {
            let val = rest.trim().trim_end_matches("*/").trim();
            config.custom_css_enabled = val == "true";
            continue;
        }

        // Color overrides enabled
        if let Some(rest) = trimmed.strip_prefix("/* @poly-color-overrides-enabled:") {
            let val = rest.trim().trim_end_matches("*/").trim();
            config.color_overrides_enabled = val == "true";
            continue;
        }

        // Variable overrides
        if let Some(rest) = trimmed.strip_prefix("/* @poly-var") {
            let inner = rest.trim().trim_end_matches("*/").trim();
            if let Some((var, value)) = inner.split_once(':') {
                config
                    .color_overrides
                    .insert(var.trim().to_string(), value.trim().to_string());
            }
            continue;
        }

        // Custom CSS block
        if trimmed == "/* @poly-custom-css-start */" {
            in_css_block = true;
            continue;
        }
        if trimmed == "/* @poly-custom-css-end */" {
            in_css_block = false;
            continue;
        }
        if in_css_block {
            custom_css.push_str(line);
            custom_css.push('\n');
        }
    }

    config.custom_css = custom_css.trim_end().to_string();
    config
}
