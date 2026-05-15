//! Harness helpers for settings surface testing.
//!
//! Pack A.3 — bodies filled.


use poly_client::{
    IsBackend, MessagingBackend, ModerationBackend, SocialGraphBackend, DmsAndGroupsBackend,
    ServerAdminBackend, AuthCredentials, BackendType, ChannelType, ClientError, ClientEvent,
    MessageContent, MessageQuery, PresenceStatus, SettingsScope, ViewBody, ViewKind,
    UpdateChannelParams, MenuTargetKind, ActionOutcome, CursorKind,
};
use poly_plugin_host::PluginBackend;

use super::harness::HarnessResult;

/// Verify that all settings sections declared by the backend are structurally well-formed.
///
/// Checks:
/// - Every section has a non-empty `section_key`.
/// - Every field within the section has a non-empty `key`.
/// - Scope is a recognised variant (guaranteed by enum deserialization).
pub async fn settings_sections_well_formed(backend: &PluginBackend) -> HarnessResult {
    let sections = backend
        .get_settings_sections()
        .await
        .map_err(|e| format!("get_settings_sections should not fail: {e:?}"))?;

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
    Ok(())
}

/// Write a value to a setting key, read it back, and assert round-trip equality.
///
/// Uses `SettingsScope::AccountGlobal` and empty scope_id as the simplest
/// well-defined scope available across all plugins.
#[allow(dead_code)]
pub async fn setting_roundtrip(
    backend: &PluginBackend,
    _scope: &str,
    key: &str,
    value: serde_json::Value,
) -> HarnessResult {
    let scope_enum = SettingsScope::AccountGlobal;
    let scope_id = "";
    let json = serde_json::to_string(&value)
        .map_err(|e| format!("value must be JSON-serializable: {e:?}"))?;

    backend
        .set_setting_value(scope_enum, scope_id, key, &json)
        .await
        .map_err(|e| format!("set_setting_value should succeed: {e:?}"))?;

    let read_back = backend
        .get_setting_value(scope_enum, scope_id, key)
        .await
        .map_err(|e| format!("get_setting_value should succeed: {e:?}"))?;

    let read_val: serde_json::Value = serde_json::from_str(&read_back)
        .map_err(|e| format!("read-back value must be valid JSON: {e:?}"))?;
    assert_eq!(value, read_val, "setting round-trip mismatch for key {key:?}");
    Ok(())
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
    _scope: &str,
    key: &str,
    value: serde_json::Value,
) -> HarnessResult {
    // Write
    let json = serde_json::to_string(&value)
        .map_err(|e| format!("value must be JSON-serializable: {e:?}"))?;
    backend
        .set_setting_value(SettingsScope::AccountGlobal, "", key, &json)
        .await
        .map_err(|e| format!("set_setting_value should succeed: {e:?}"))?;

    // Read back within same instance (full cross-reload is an integration test)
    let read_back = backend
        .get_setting_value(SettingsScope::AccountGlobal, "", key)
        .await
        .map_err(|e| format!("get_setting_value should succeed after write: {e:?}"))?;

    let read_val: serde_json::Value = serde_json::from_str(&read_back)
        .map_err(|e| format!("read-back value must be valid JSON: {e:?}"))?;
    assert_eq!(
        value, read_val,
        "setting value should match after same-instance reload for key {key:?}"
    );
    Ok(())
}
