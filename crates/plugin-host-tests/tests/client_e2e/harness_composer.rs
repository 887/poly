//! Harness helpers for composer toolbar and per-message action surface testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 6.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Verify that the composer toolbar buttons declared for the given channel are well-formed.
#[allow(dead_code)]
pub async fn composer_buttons_well_formed(backend: &PluginBackend, ch_id: &str) {
    todo!("WP 6: implement per plan")
}

/// Verify that the per-message action items declared for a given message are well-formed.
#[allow(dead_code)]
pub async fn message_actions_well_formed(
    backend: &PluginBackend,
    ch_id: &str,
    msg_id: &str,
) {
    todo!("WP 6: implement per plan")
}

/// Invoke a known composer action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_composer_action_roundtrip(
    backend: &PluginBackend,
    ch_id: &str,
    action_id: &str,
) {
    todo!("WP 6: implement per plan")
}

/// Invoke a known per-message action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_message_action_roundtrip(
    backend: &PluginBackend,
    ch_id: &str,
    msg_id: &str,
    action_id: &str,
) {
    todo!("WP 6: implement per plan")
}
