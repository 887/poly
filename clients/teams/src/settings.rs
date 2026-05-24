//! `impl SettingsBackend for TeamsClient` — settings sections and storage.
//! C.1: per-team profile settings surface.

use crate::TeamsClient;
#[cfg(feature = "native")]
use async_trait::async_trait;
#[cfg(feature = "native")]
use poly_client::*;

// ── C.1 — SettingsBackend ────────────────────────────────────────────────────

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for TeamsClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::PerServer,
            section_key: "team-profile".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "display-name".to_string(),
                    kind: SettingKind::TextInput,
                    default_value: "\"\"".to_string(),
                    extra: String::new(),
                },
                SettingDescriptor {
                    key: "description".to_string(),
                    kind: SettingKind::TextInput,
                    default_value: "\"\"".to_string(),
                    extra: String::new(),
                },
            ],
            info_block: None,
        }])
    }

    fn settings_storage(&self) -> &SettingsStorageCell {
        &self.settings_storage
    }
}
