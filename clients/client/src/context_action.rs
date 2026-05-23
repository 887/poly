//! `ContextActionBackend` capability sub-trait (Phase C.1 — ISP split).
//!
//! Carved out of [`IsBackend`] in Phase C.1 of
//! `docs/plans/plan-solid-audit-core-state.md`.  Groups context-menu,
//! composer-button, and message-action declaration & dispatch
//! (`D11 / D14 / D16 / D22 / D25`).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(ca) = backend.as_context_action() {
//!     let items = ca.get_context_menu_items(target, &target_id).await?;
//! }
//! ```
//!
//! The legacy [`IsBackend`] methods (`get_context_menu_items`,
//! `invoke_context_action`, `get_message_actions`, `invoke_message_action`,
//! `get_composer_buttons`, `invoke_composer_action`, `poll_action`)
//! remain as default-delegating shims so existing call sites in
//! `crates/core/` continue to compile.
//!
//! [`IsBackend`]: crate::IsBackend
//! [`IsBackend::as_context_action`]: crate::IsBackend::as_context_action

use async_trait::async_trait;

use crate::{
    ActionOutcome, ClientError, ClientResult, ComposerButton, MenuItem, MenuTargetKind,
    PendingHandle,
};

/// Capability sub-trait for plugin-declared context menu / composer / message actions.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ContextActionBackend: Send + Sync {
    /// D11 — return plugin-declared context menu items for `target`.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_context_menu_items(
        &self,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![])
    }

    /// D14 / D22 — dispatch a plugin action.
    ///
    /// Default: `Err(NotFound(action_id))`.
    async fn invoke_context_action(
        &self,
        action_id: &str,
        _target: MenuTargetKind,
        _target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(action_id.to_string()))
    }

    /// D8 — per-message actions, merged into the message hover/overflow menu.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_message_actions(
        &self,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        Ok(vec![])
    }

    /// D14 / D25 — dispatch a per-message action.
    ///
    /// Default: `Err(NotFound(action_id))`.
    async fn invoke_message_action(
        &self,
        action_id: &str,
        _channel_id: &str,
        _message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(action_id.to_string()))
    }

    /// D8 — composer-toolbar buttons for the given channel.
    ///
    /// Default: `Ok(vec![])`.
    async fn get_composer_buttons(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<ComposerButton>> {
        Ok(vec![])
    }

    /// D14 / D25 — dispatch a composer button action.
    ///
    /// Default: `Err(NotFound(action_id))`.
    async fn invoke_composer_action(
        &self,
        action_id: &str,
        _channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotFound(action_id.to_string()))
    }

    /// D16 — poll a pending async action for its final outcome.
    ///
    /// Default: `Err(NotSupported)`.
    async fn poll_action(&self, _handle: PendingHandle) -> ClientResult<ActionOutcome> {
        Err(ClientError::NotSupported("poll_action".to_string()))
    }
}
