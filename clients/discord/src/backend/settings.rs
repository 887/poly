//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::DiscordClient;
use async_trait::async_trait;
use poly_client::{ClientResult, SettingsSection, SettingsScope, SettingDescriptor, SettingKind, SettingsStorageCell};


#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::SettingsBackend for DiscordClient {
    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        Ok(vec![
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "profile".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "nickname".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "server-avatar-url".to_string(),
                        kind: SettingKind::TextInput,
                        default_value: "\"\"".to_string(),
                        extra: String::new(),
                    },
                ],
                info_block: None,
            },
            SettingsSection {
                scope: SettingsScope::PerServer,
                section_key: "notification-rules".to_string(),
                icon: None,
                fields: vec![
                    SettingDescriptor {
                        key: "mentions-only".to_string(),
                        kind: SettingKind::Toggle,
                        default_value: "false".to_string(),
                        extra: String::new(),
                    },
                    SettingDescriptor {
                        key: "mute-category".to_string(),
                        kind: SettingKind::Toggle,
                        default_value: "false".to_string(),
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
                    key: "allow-dms-from-server-members".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "true".to_string(),
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
