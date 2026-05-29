//! `SettingsBackend` implementation for `HackerNewsClient`.
//!
//! Exposes two account-global settings: `default-feed` (select) and
//! `items-per-page` (slider). Storage is in-memory via `SettingsStorageCell`.

use async_trait::async_trait;
use poly_client::{ClientResult, SettingsSection, SettingsScope, SettingDescriptor, SettingKind, SettingsStorageCell};

use crate::HackerNewsClient;

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for HackerNewsClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::AccountGlobal,
            section_key: "preferences".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "default-feed".to_string(),
                    kind: SettingKind::Select,
                    default_value: "\"top\"".to_string(),
                    extra: "[\"top\",\"new\",\"best\",\"ask\",\"show\",\"jobs\"]".to_string(),
                },
                SettingDescriptor {
                    key: "items-per-page".to_string(),
                    kind: SettingKind::Slider,
                    default_value: "30".to_string(),
                    extra: "{\"min\":10,\"max\":100,\"step\":5}".to_string(),
                },
            ],
            info_block: None,
        }])
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}
