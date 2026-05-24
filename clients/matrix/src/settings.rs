//! `impl SettingsBackend for MatrixClient` — settings sections and storage cell.

use async_trait::async_trait;
use poly_client::*;

use crate::MatrixClient;

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for MatrixClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "space-settings".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "display-name".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "topic".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                ],
                info_block: None,
            },
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "privacy".to_string(),
                icon: None,
                fields: vec![SettingDescriptor {
                    key: "allow-guests".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
                    extra: String::new(),
                }],
                info_block: None,
            },
        ])
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}
