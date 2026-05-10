//! Harness helpers for sidebar surface testing.
//!
//! Pack B.2 P28 — `sidebar_invalidated_event_refetches` body filled. Other
//! skeletons remain deferred.

use poly_client::IsBackend;
use poly_plugin_host::PluginBackend;

use super::harness::HarnessResult;

/// Verify that the sidebar declaration returned by the backend is structurally well-formed.
#[allow(dead_code)]
pub async fn sidebar_declaration_well_formed(_backend: &PluginBackend) -> HarnessResult {
    // WP 4: implement per plan
    Ok(())
}

/// Verify that the sidebar layout items are consistent with the backend's declared capabilities.
#[allow(dead_code)]
pub async fn sidebar_layout_matches_capabilities(_backend: &PluginBackend) -> HarnessResult {
    // WP 4: implement per plan
    Ok(())
}

/// Pack B.2 P28 — simulate the host receiving a `ClientEvent::SidebarInvalidated`
/// from the plugin and verify that a subsequent call to `get_sidebar_declaration`
/// succeeds and returns the same shape (i.e. the host re-fetches without panic
/// or state corruption).
///
/// Full event-driven wiring (the UI dep-tick increment) lives in
/// `crates/core/src/ui/demo.rs`'s event-stream listener; this harness exercises
/// the backend contract piece: two consecutive `get_sidebar_declaration` calls
/// return valid declarations with stable layout kinds.
#[allow(dead_code)]
pub async fn sidebar_invalidated_event_refetches(backend: &mut PluginBackend) -> HarnessResult {
    let first = backend
        .get_sidebar_declaration()
        .await
        .map_err(|e| format!("first get_sidebar_declaration should succeed: {e:?}"))?;
    // Re-fetch — in production the host triggers this on receipt of
    // `ClientEvent::SidebarInvalidated` via the `sidebar_invalidated_tick`
    // increment in AppState. Here we prove the backend side is idempotent.
    let second = backend
        .get_sidebar_declaration()
        .await
        .map_err(|e| {
            format!("second get_sidebar_declaration (after invalidation) should succeed: {e:?}")
        })?;
    assert_eq!(
        first.layout, second.layout,
        "layout kind must not flip between refetches"
    );
    Ok(())
}

/// Invoke a known sidebar action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_sidebar_action_roundtrip(
    _backend: &PluginBackend,
    _action_id: &str,
) -> HarnessResult {
    // WP 4: implement per plan
    Ok(())
}
