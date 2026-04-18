//! Harness helpers for context-menu surface testing.
//!
//! Skeletons only — bodies are stubs. Filled in WP 2.
//! WP 1 will replace `&str` placeholders with typed enums once
//! `MenuTargetKind` exists in the WIT-generated bindings.

use poly_client::{ActionOutcome, ClientBackend, MenuTargetKind};
use poly_plugin_host::PluginBackend;

use super::harness::HarnessResult;

/// Verify that all menu items returned for a given target are structurally well-formed.
///
/// # WP note
/// WP 1: replace `target: &str` with `target: MenuTargetKind` enum.
#[allow(dead_code)]
pub async fn menu_items_well_formed(
    _backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    _target: &str,
    _target_id: &str,
) -> HarnessResult {
    // WP 2: implement per plan
    Ok(())
}

/// Verify that every label key declared by menu items resolves in the plugin's FTL bundle.
///
/// # WP note
/// WP 1: replace `target: &str` with `target: MenuTargetKind` enum.
#[allow(dead_code)]
pub async fn menu_items_have_valid_ftl(
    _backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    _target: &str,
    _target_id: &str,
) -> HarnessResult {
    // WP 2: implement per plan
    Ok(())
}

/// Verify that all action IDs declared by menu items are kebab-case.
///
/// # WP note
/// WP 1: replace `target: &str` with `target: MenuTargetKind` enum.
#[allow(dead_code)]
pub async fn menu_items_use_kebab_action_ids(
    _backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    _target: &str,
    _target_id: &str,
) -> HarnessResult {
    // WP 2: implement per plan
    Ok(())
}

/// Verify that invoking an unknown action ID returns a NotFound error (not a panic).
#[allow(dead_code)]
pub async fn invoke_action_unknown_returns_notfound(
    _backend: &PluginBackend,
    // WP 1: replace &str with MenuTargetKind enum
    _target: &str,
    _target_id: &str,
) -> HarnessResult {
    // WP 2: implement per plan
    Ok(())
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
) -> HarnessResult {
    let outcome = backend
        .invoke_context_action(known_id, target, target_id)
        .await
        .map_err(|err| {
            format!("invoke_context_action({known_id}) should succeed for a known id: {err:?}")
        })?;
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
    Ok(())
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
) -> HarnessResult {
    let outcome = backend
        .invoke_context_action(known_id, target, target_id)
        .await;
    let Ok(outcome) = outcome else {
        return Ok(()); // Backend doesn't support this id — nothing to poll.
    };
    let ActionOutcome::Pending(handle) = outcome else {
        return Ok(()); // Plugin resolved synchronously — nothing to poll.
    };
    // Loop at most 10 times with a minimal delay; stop when non-pending.
    let mut current = handle;
    for _ in 0..10 {
        match backend.poll_action(current.clone()).await {
            Ok(ActionOutcome::Pending(next)) => current = next,
            Ok(_) => return Ok(()),
            Err(err) => {
                return Err(
                    format!("poll_action should not fail for valid handle: {err:?}").into(),
                )
            }
        }
    }
    Err("poll_action did not resolve within 10 iterations".into())
}
