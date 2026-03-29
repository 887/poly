//! Shared JS-backed scroll spy helpers for settings pages.
//!
//! This module centralizes the settings-page scroll tracking logic so the app
//! level settings, account settings, and server settings all use the same
//! runtime script instead of three separate inline JS blobs.
//!
//! NOTE(DX-ASSET-JS-1): keep this runtime loaded via `asset!` so Dioxus hot
//! reload sees script edits instead of freezing them into `include_str!`.

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use manganis::asset;
#[cfg(target_arch = "wasm32")]
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
const SETTINGS_SCROLL_SPY_RUNTIME_JS: Asset = asset!(
    "assets/scripts/settings_scroll_spy_runtime.js",
    AssetOptions::js()
);

/// JS install-time config for a settings scroll spy.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsScrollSpyConfig {
    pub(crate) runtime_flag: &'static str,
    pub(crate) scroll_root_selectors: Vec<&'static str>,
    pub(crate) section_prefix: &'static str,
    pub(crate) section_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) plugin_section_prefix: Option<&'static str>,
}

/// Smooth-scroll the shared settings content area to a section anchor.
#[cfg(target_arch = "wasm32")]
pub(crate) fn scroll_to_settings_section(section_prefix: &str, slug: &str) {
    let id = format!("{section_prefix}{slug}");
    let js = format!("window.__polyScrollSettingsSectionById?.('{}')", id);
    let _ = document::eval(&js);
}

/// No-op on non-wasm targets.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn scroll_to_settings_section(_section_prefix: &str, _slug: &str) {}

/// Install the shared settings scroll spy and forward active slugs to Rust.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_settings_scroll_spy<F>(config: SettingsScrollSpyConfig, _on_slug: F)
where
    F: FnMut(String) + 'static,
{
    spawn(async move {
        let config_json = match serde_json::to_string(&config) {
            Ok(json) => json,
            Err(err) => {
                tracing::warn!("Failed to serialize settings scroll spy config: {err}");
                return;
            }
        };

        let js = format!(
            "window.__polySettingsScrollSpyCleanup?.(); window.__polySettingsScrollSpyPendingConfig = {config_json}; 'ready'"
        );
        let _ = document::eval(&js);

        if !crate::ui::load_js_asset(SETTINGS_SCROLL_SPY_RUNTIME_JS).await {
            return;
        }

        // Small delay via JS to ensure scroll spy runtime is initialized before installing
        let _ = document::eval(
            "new Promise(r => setTimeout(r, 10)).then(() => window.__polyInstallSettingsScrollSpy?.(window.__polySettingsScrollSpyPendingConfig))",
        );
    });
}
