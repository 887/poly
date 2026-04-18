//! Harness helpers for context-menu surface testing.
//!
//! Skeletons only — bodies are `todo!()`. Filled in WP 2.
//! WP 1 will replace `&str` placeholders with typed enums once
//! `MenuTargetKind` exists in the WIT-generated bindings.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, unused_variables)]

use poly_client::{ActionOutcome, ClientBackend, MenuTargetKind};
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
///
/// Pack B layer (d): a known action must resolve to a non-panicking
/// `ActionOutcome`. Every current backend returns `Noop` / `Completed` /
/// `Toast` / `Navigate` / `RefreshTarget` etc. — all of which are valid.
#[allow(dead_code)]
pub async fn invoke_action_roundtrip(
    backend: &PluginBackend,
    target: MenuTargetKind,
    target_id: &str,
    known_id: &str,
) {
    let outcome = backend
        .invoke_context_action(known_id, target, target_id)
        .await;
    let outcome = outcome.unwrap_or_else(|err| {
        panic!("invoke_context_action({known_id}) should succeed for a known id: {err:?}")
    });
    match outcome {
        ActionOutcome::Noop
        | ActionOutcome::Completed
        | ActionOutcome::Pending(_)
        | ActionOutcome::RefreshTarget
        | ActionOutcome::RefreshSidebar
        | ActionOutcome::Navigate(_)
        | ActionOutcome::Toast(_)
        | ActionOutcome::OpenSettings(_)
        | ActionOutcome::OpenModal(_) => {
            // All variants are acceptable — just assert the match is total.
        }
    }
}

/// Verify that a menu action that returns a pending state can be polled to
/// completion. Pack B / P12 — `poll_action` must accept a plugin-opaque
/// handle without panicking and return a well-formed `ActionOutcome`.
#[allow(dead_code)]
pub async fn menu_pending_action_polls(
    backend: &PluginBackend,
    target: MenuTargetKind,
    target_id: &str,
    known_id: &str,
) {
    let outcome = backend
        .invoke_context_action(known_id, target, target_id)
        .await;
    let Ok(outcome) = outcome else {
        return; // Backend doesn't support this id — nothing to poll.
    };
    let ActionOutcome::Pending(handle) = outcome else {
        return; // Plugin resolved synchronously — nothing to poll.
    };
    // Loop at most 10 times with a minimal delay; stop when non-pending.
    let mut current = handle;
    for _ in 0..10 {
        match backend.poll_action(current.clone()).await {
            Ok(ActionOutcome::Pending(next)) => current = next,
            Ok(_) => return,
            Err(err) => panic!("poll_action should not fail for valid handle: {err:?}"),
        }
    }
    panic!("poll_action did not resolve within 10 iterations");
}
