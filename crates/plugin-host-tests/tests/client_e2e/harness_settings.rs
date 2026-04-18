//! Harness helpers for settings surface testing.
//!
//! Pack A.3 — bodies filled.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_client::{ClientBackend, SettingsScope};
use poly_plugin_host::PluginBackend;

/// Verify that all settings sections declared by the backend are structurally well-formed.
///
/// Checks:
/// - Every section has a non-empty `section_key`.
/// - Every field within the section has a non-empty `key`.
/// - Scope is a recognised variant (guaranteed by enum deserialization).
pub async fn settings_sections_well_formed(backend: &PluginBackend) {
    let sections = backend
        .get_settings_sections()
        .await
        .expect("get_settings_sections should not fail");

    for section in &sections {
        assert!(
            !section.section_key.is_empty(),
            "SettingsSection.section_key must not be empty"
        );
        for field in &section.fields {
            assert!(
                !field.key.is_empty(),
                "SettingDescriptor.key must not be empty in section {:?}",
                section.section_key
            );
        }
    }
}

/// Write a value to a setting key, read it back, and assert round-trip equality.
///
/// Uses `SettingsScope::AccountGlobal` and empty scope_id as the simplest
/// well-defined scope available across all plugins.
#[allow(dead_code)]
pub async fn setting_roundtrip(
    backend: &PluginBackend,
    scope: &str,
    key: &str,
    value: serde_json::Value,
) {
    let scope_enum = SettingsScope::AccountGlobal;
    let scope_id = "";
    let json = serde_json::to_string(&value).expect("value must be JSON-serializable");

    backend
        .set_setting_value(scope_enum, scope_id, key, &json)
        .await
        .expect("set_setting_value should succeed");

    let read_back = backend
        .get_setting_value(scope_enum, scope_id, key)
        .await
        .expect("get_setting_value should succeed");

    let read_val: serde_json::Value =
        serde_json::from_str(&read_back).expect("read-back value must be valid JSON");
    assert_eq!(value, read_val, "setting round-trip mismatch for key {key:?}");
}

/// Write a setting, simulate a backend reload, and verify the value survives.
///
/// Note: true cross-restart persistence requires a storage backend that
/// survives the plugin being dropped and re-instantiated.  This helper
/// is a placeholder that tests the write/read contract within one instance
/// lifetime; full persistence is covered by integration tests.
#[allow(dead_code)]
pub async fn setting_persists_across_reload(
    backend: &mut PluginBackend,
    scope: &str,
    key: &str,
    value: serde_json::Value,
) {
    // Write
    let json = serde_json::to_string(&value).expect("value must be JSON-serializable");
    backend
        .set_setting_value(SettingsScope::AccountGlobal, "", key, &json)
        .await
        .expect("set_setting_value should succeed");

    // Read back within same instance (full cross-reload is an integration test)
    let read_back = backend
        .get_setting_value(SettingsScope::AccountGlobal, "", key)
        .await
        .expect("get_setting_value should succeed after write");

    let read_val: serde_json::Value =
        serde_json::from_str(&read_back).expect("read-back value must be valid JSON");
    assert_eq!(value, read_val, "setting value should match after same-instance reload for key {key:?}");
}
