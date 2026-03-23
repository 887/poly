//! Shared JS-backed scroll spy helpers for settings pages.
//!
//! This module centralizes the settings-page scroll tracking logic so the app
//! level settings, account settings, and server settings all use the same
//! runtime script instead of three separate inline JS blobs.

#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use serde::Serialize;

#[cfg(target_arch = "wasm32")]
const SETTINGS_SCROLL_SPY_RUNTIME_JS: &str =
    include_str!("../../../assets/scripts/settings_scroll_spy_runtime.js");

/// JS install-time config for a settings scroll spy.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SettingsScrollSpyConfig {
    pub(crate) runtime_flag: &'static str,
    pub(crate) content_selector: &'static str,
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
pub(crate) fn install_settings_scroll_spy<F>(config: SettingsScrollSpyConfig, on_slug: F)
where
    F: FnMut(String) + 'static,
{
    spawn(async move {
        let _ = document::eval(SETTINGS_SCROLL_SPY_RUNTIME_JS);
        let config_json = match serde_json::to_string(&config) {
            Ok(json) => json,
            Err(err) => {
                tracing::warn!("Failed to serialize settings scroll spy config: {err}");
                return;
            }
        };

        let js = format!("window.__polyInstallSettingsScrollSpy?.({config_json})");
        let mut eval = document::eval(&js);
        let Ok(status) = eval.recv::<String>().await else {
            return;
        };
        if status != "ready" {
            return;
        }

        let mut on_slug = on_slug;
        loop {
            let Ok(slug) = eval.recv::<String>().await else {
                break;
            };
            on_slug(slug);
        }

        let _ = document::eval("window.__polySettingsScrollSpyCleanup?.();");
    });
}
