//! Harness helpers for settings surface testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 3.
//! WP 1 will replace `&str` placeholders with typed enums once
//! `SettingsScope` exists in the WIT-generated bindings.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Verify that all settings sections declared by the backend are structurally well-formed.
#[allow(dead_code)]
pub async fn settings_sections_well_formed(backend: &PluginBackend) {
    todo!("WP 3: implement per plan")
}

/// Write a value to a setting key, read it back, and assert round-trip equality.
///
/// # WP note
/// WP 1: replace `scope: &str` with `scope: SettingsScope` enum.
#[allow(dead_code)]
pub async fn setting_roundtrip(
    backend: &PluginBackend,
    // WP 1: replace &str with SettingsScope enum
    scope: &str,
    key: &str,
    value: serde_json::Value,
) {
    todo!("WP 3: implement per plan")
}

/// Write a setting, simulate a backend reload, and verify the value survives.
///
/// # WP note
/// WP 1: replace `scope: &str` with `scope: SettingsScope` enum.
#[allow(dead_code)]
pub async fn setting_persists_across_reload(
    backend: &mut PluginBackend,
    // WP 1: replace &str with SettingsScope enum
    scope: &str,
    key: &str,
    value: serde_json::Value,
) {
    todo!("WP 3: implement per plan")
}
