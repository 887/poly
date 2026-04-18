//! Harness helpers for context-menu surface testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 2.
//! WP 1 will replace `&str` placeholders with typed enums once
//! `MenuTargetKind` exists in the WIT-generated bindings.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_plugin_host::PluginBackend;

/// Verify that all menu items returned for a given target are structurally well-formed.
///
/// # WP note
/// WP 1: replace `target: &str` with `target: MenuTargetKind` enum.
#[allow(dead_code)]
pub async fn menu_items_well_formed(
    backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    target: &str,
    target_id: &str,
) {
    todo!("WP 2: implement per plan")
}

/// Verify that every label key declared by menu items resolves in the plugin's FTL bundle.
///
/// # WP note
/// WP 1: replace `target: &str` with `target: MenuTargetKind` enum.
#[allow(dead_code)]
pub async fn menu_items_have_valid_ftl(
    backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    target: &str,
    target_id: &str,
) {
    todo!("WP 2: implement per plan")
}

/// Verify that all action IDs declared by menu items are kebab-case.
///
/// # WP note
/// WP 1: replace `target: &str` with `target: MenuTargetKind` enum.
#[allow(dead_code)]
pub async fn menu_items_use_kebab_action_ids(
    backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    target: &str,
    target_id: &str,
) {
    todo!("WP 2: implement per plan")
}

/// Verify that invoking an unknown action ID returns a NotFound error (not a panic).
#[allow(dead_code)]
pub async fn invoke_action_unknown_returns_notfound(
    backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    target: &str,
    target_id: &str,
) {
    todo!("WP 2: implement per plan")
}

/// Invoke a known action ID and verify the returned outcome is well-formed.
#[allow(dead_code)]
pub async fn invoke_action_roundtrip(
    backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    target: &str,
    target_id: &str,
    known_id: &str,
) {
    todo!("WP 2: implement per plan")
}

/// Verify that a menu action that returns a pending state can be polled to completion.
#[allow(dead_code)]
pub async fn menu_pending_action_polls(
    backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    target: &str,
    target_id: &str,
) {
    todo!("WP 2: implement per plan")
}
