//! Harness helpers for sidebar surface testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 4.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Verify that the sidebar declaration returned by the backend is structurally well-formed.
#[allow(dead_code)]
pub async fn sidebar_declaration_well_formed(backend: &PluginBackend) {
    todo!("WP 4: implement per plan")
}

/// Verify that the sidebar layout items are consistent with the backend's declared capabilities.
#[allow(dead_code)]
pub async fn sidebar_layout_matches_capabilities(backend: &PluginBackend) {
    todo!("WP 4: implement per plan")
}

/// Emit a `SidebarInvalidated` event and verify the host re-fetches the declaration.
#[allow(dead_code)]
pub async fn sidebar_invalidated_event_refetches(backend: &mut PluginBackend) {
    todo!("WP 4: implement per plan")
}

/// Invoke a known sidebar action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_sidebar_action_roundtrip(backend: &PluginBackend, action_id: &str) {
    todo!("WP 4: implement per plan")
}
