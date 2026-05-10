//! Harness helpers for composer toolbar and per-message action surface testing.
//!
//! Pack A.3 — bodies filled.

use poly_client::IsBackend;
use poly_plugin_host::PluginBackend;

use super::harness::HarnessResult;

/// Verify that the composer toolbar buttons declared for the given channel are well-formed.
///
/// Checks:
/// - Every button has a non-empty `id` (kebab-case action id).
/// - Every button has a non-empty `label_key`.
/// - `position` is a valid `ComposerSlot` variant.
pub async fn composer_buttons_well_formed(
    backend: &PluginBackend,
    ch_id: &str,
) -> HarnessResult {
    let buttons = backend
        .get_composer_buttons(ch_id)
        .await
        .map_err(|e| format!("get_composer_buttons should not fail: {e:?}"))?;

    // Shape assertions — no items is valid (plugin may declare none).
    for button in &buttons {
        assert!(!button.id.is_empty(), "ComposerButton.id must not be empty");
        assert!(
            !button.label_key.is_empty(),
            "ComposerButton.label_key must not be empty for button {:?}",
            button.id
        );
        // position is an enum so it is always valid if deserialized.
        let _ = button.position;
    }
    Ok(())
}

/// Verify that the per-message action items declared for a given message are well-formed.
#[allow(dead_code)]
pub async fn message_actions_well_formed(
    backend: &PluginBackend,
    ch_id: &str,
    msg_id: &str,
) -> HarnessResult {
    let items = backend
        .get_message_actions(ch_id, msg_id)
        .await
        .map_err(|e| format!("get_message_actions should not fail: {e:?}"))?;

    for item in &items {
        assert!(!item.id.is_empty(), "MenuItem.id must not be empty");
        assert!(
            !item.label_key.is_empty(),
            "MenuItem.label_key must not be empty for item {:?}",
            item.id
        );
    }
    Ok(())
}

/// Invoke a known composer action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_composer_action_roundtrip(
    backend: &PluginBackend,
    ch_id: &str,
    action_id: &str,
) -> HarnessResult {
    // invoke_composer_action returns Result<ActionOutcome, ClientError>.
    // For a known action id the plugin should return Ok(...).
    let outcome = backend.invoke_composer_action(action_id, ch_id).await;
    assert!(
        outcome.is_ok(),
        "invoke_composer_action({action_id}) should succeed for known action id"
    );
    Ok(())
}

/// Invoke a known per-message action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_message_action_roundtrip(
    backend: &PluginBackend,
    ch_id: &str,
    msg_id: &str,
    action_id: &str,
) -> HarnessResult {
    let outcome = backend.invoke_message_action(action_id, ch_id, msg_id).await;
    assert!(
        outcome.is_ok(),
        "invoke_message_action({action_id}) should succeed for known action id"
    );
    Ok(())
}
