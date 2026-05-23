//! `SettingsBackend` capability sub-trait (Phase C.1 — ISP split).
//!
//! Carved out of [`IsBackend`] in Phase C.1 of
//! `docs/plans/plan-solid-audit-core-state.md`.  Groups the
//! per-backend settings declaration & storage methods (`D11 / D15 / D18`).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(sb) = backend.as_settings() {
//!     let sections = sb.get_settings_sections().await?;
//! }
//! ```
//!
//! The legacy [`IsBackend`] methods (`get_settings_sections`,
//! `settings_storage`, `get_setting_value`, `set_setting_value`)
//! remain as default-delegating shims so existing call sites in
//! `crates/core/` continue to compile.  When `as_settings()` returns
//! `Some`, IsBackend forwards to the sub-trait impl; otherwise the
//! historic "empty cell" defaults apply.
//!
//! [`IsBackend`]: crate::IsBackend
//! [`IsBackend::as_settings`]: crate::IsBackend::as_settings

use async_trait::async_trait;

use crate::{ClientError, ClientResult, SettingsScope, SettingsSection, SettingsStorageCell};

/// Capability sub-trait for plugin-declared settings.
///
/// `get_setting_value` / `set_setting_value` have default impls that read
/// through to [`Self::settings_storage`] — backends only need to override
/// when they back settings with a non-`SettingsStorageCell` store (none
/// do today, but the override slot exists).
///
/// [`IsBackend::as_settings`]: crate::IsBackend::as_settings
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SettingsBackend: Send + Sync {
    /// D11 / D18 — every settings section this plugin contributes.
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>>;

    /// Storage cell for backend-local settings.
    fn settings_storage(&self) -> &SettingsStorageCell;

    /// D15 — read a JSON-encoded setting value.
    ///
    /// Default reads from `self.settings_storage()` then falls back to the
    /// declared default value from `get_settings_sections()`.
    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        if let Some(v) = self.settings_storage().get(scope, scope_id, key) {
            return Ok(v);
        }
        for section in self.get_settings_sections().await? {
            for field in section.fields {
                if field.key == key {
                    return Ok(field.default_value);
                }
            }
        }
        Err(ClientError::NotFound(format!("setting: {key}")))
    }

    /// D15 — write a JSON-encoded setting value.
    ///
    /// Default writes through to `self.settings_storage()`.
    async fn set_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()> {
        self.settings_storage().set(scope, scope_id, key, value)
    }
}
