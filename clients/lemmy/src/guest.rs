//! WASM Component Model guest implementation for the Lemmy messenger plugin.
//!
//! This module is only compiled when targeting `wasm32-wasip2` (gated in `lib.rs`).
//! It implements all exported WIT interfaces with minimal stubs sufficient for the
//! plugin to load and respond to the host.

#![allow(unsafe_code)]

use crate::wit_bindings::{
    ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, MessengerClientGuest, PluginManifest, PluginMetadataGuest, export,
    poly::messenger::host_api,
    wit,
};

use exports::poly::messenger::{
    client_menus::{
        ActionOutcome, MenuItem, MenuTargetKind, PendingHandle,
    },
    client_settings::{SettingDescriptor, SettingKind, SettingsScope, SettingsSection},
    client_sidebar::{SidebarDeclaration, SidebarLayoutKind},
    client_views::{ViewDescriptor, ViewDetail, ViewRowsPage},
    client_composer::{ComposerButton},
};

use crate::wit_bindings::exports;

/// Zero-sized marker struct for the Lemmy WASM plugin component.
pub struct LemmyPlugin;

// ─── MessengerClientGuest ──────────────────────────────────────────

impl MessengerClientGuest for LemmyPlugin {
    fn authenticate(
        _credentials: wit::AuthCredentials,
    ) -> Result<wit::Session, wit::ClientError> {
        Err(wit::ClientError::AuthFailed(
            "Lemmy WASM plugin: authenticate not yet implemented".to_string(),
        ))
    }

    fn logout() -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn is_authenticated() -> bool {
        false
    }

    fn get_servers() -> Result<Vec<wit::Server>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_server(_id: String) -> Result<wit::Server, wit::ClientError> {
        Err(wit::ClientError::NotFound("server not found".to_string()))
    }

    fn get_channels(_server_id: String) -> Result<Vec<wit::Channel>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_channel(_id: String) -> Result<wit::Channel, wit::ClientError> {
        Err(wit::ClientError::NotFound("channel not found".to_string()))
    }

    fn send_message(
        _channel_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "send_message not yet implemented in WASM plugin".to_string(),
        ))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "send_reply_message not yet implemented in WASM plugin".to_string(),
        ))
    }

    fn get_messages(
        _channel_id: String,
        _query: wit::MessageQuery,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        Ok(vec![])
    }

    fn search_messages(
        _query: wit::MessageSearchQuery,
    ) -> Result<Vec<wit::MessageSearchHit>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_pinned_messages(
        _channel_id: String,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_available_emojis(
        _channel_id: String,
    ) -> Result<Vec<wit::CustomEmoji>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_available_stickers(
        _channel_id: String,
    ) -> Result<Vec<wit::StickerItem>, wit::ClientError> {
        Ok(vec![])
    }

    fn set_message_pinned(
        _channel_id: String,
        _message_id: String,
        _pinned: bool,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy does not support pinning messages".to_string(),
        ))
    }

    fn get_user(_user_id: String) -> Result<wit::User, wit::ClientError> {
        Err(wit::ClientError::NotFound("user not found".to_string()))
    }

    fn get_friends() -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_channel_members(
        _channel_id: String,
    ) -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_groups() -> Result<Vec<wit::Group>, wit::ClientError> {
        Ok(vec![])
    }

    fn remove_group_member(
        _group_id: String,
        _user_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no group DMs".to_string(),
        ))
    }

    fn add_group_member(
        _group_id: String,
        _user_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no group DMs".to_string(),
        ))
    }

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(vec![])
    }

    fn open_direct_message_channel(
        _user_id: String,
    ) -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotFound(
            "DM channel not found".to_string(),
        ))
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy does not have a saved messages channel".to_string(),
        ))
    }

    fn get_notifications() -> Result<Vec<wit::Notification>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_voice_participants(
        _channel_id: String,
    ) -> Result<Vec<wit::VoiceParticipant>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_presence(_user_id: String) -> Result<wit::PresenceStatus, wit::ClientError> {
        Ok(wit::PresenceStatus::Offline)
    }

    fn set_presence(_status: wit::PresenceStatus) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no presence system".to_string(),
        ))
    }

    fn handle_ws_data(_handle: u64, _data: Vec<u8>) {
        // Lemmy v0.19+ removed WebSocket; no WS data expected.
    }

    fn get_backend_type() -> String {
        "lemmy".to_string()
    }

    fn get_backend_name() -> String {
        "Lemmy".to_string()
    }

    fn get_backend_capabilities() -> wit::BackendCapabilities {
        wit::BackendCapabilities {
            supports_voice: false,
            supports_video: false,
            supports_dms: true,
            supports_groups: false,
            supports_send_messages: true,
            supports_presence: false,
            supports_search: true,
            supports_reactions: true,
            supports_typing_indicators: false,
            supports_file_upload: true,
            landing: wit::LandingPage::FirstServer,
        }
    }

    fn list_files(
        _channel_id: String,
        _path: String,
    ) -> Result<Vec<wit::FileEntry>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no code channels".to_string(),
        ))
    }

    fn read_file(
        _channel_id: String,
        _path: String,
    ) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no code channels".to_string(),
        ))
    }
}

// ─── PluginMetadataGuest ───────────────────────────────────────────

impl PluginMetadataGuest for LemmyPlugin {
    fn get_translations(locale: String) -> String {
        crate::plugin_translations(&locale)
    }

    fn get_display_name_key() -> String {
        "plugin-lemmy-title".to_string()
    }

    fn get_icon() -> String {
        "🐀".to_string()
    }

    fn get_plugin_manifest() -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["*".to_string()],
            description: "Lemmy federated forum client".to_string(),
            homepage: None,
        }
    }
}

// ─── ClientMenusGuest ─────────────────────────────────────────────

impl ClientMenusGuest for LemmyPlugin {
    fn get_context_menu_items(
        _target: MenuTargetKind,
        _target_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        Ok(vec![])
    }

    fn invoke_context_action(
        action_id: String,
        _target: MenuTargetKind,
        _target_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!(
            "unknown action: {action_id}"
        )))
    }

    fn poll_action(
        _handle: PendingHandle,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(
            "no pending actions".to_string(),
        ))
    }
}

// ─── Settings helpers ─────────────────────────────────────────────

fn scope_label(scope: SettingsScope) -> &'static str {
    match scope {
        SettingsScope::AccountGlobal => "account-global",
        SettingsScope::PerServer => "per-server",
        SettingsScope::PerChannel => "per-channel",
        SettingsScope::PerUser => "per-user",
    }
}

fn composite_key(scope: SettingsScope, scope_id: &str, key: &str) -> String {
    format!("settings:{}:{}:{}", scope_label(scope), scope_id, key)
}

// ─── ClientSettingsGuest ──────────────────────────────────────────

impl ClientSettingsGuest for LemmyPlugin {
    fn get_settings_sections() -> Result<Vec<SettingsSection>, wit::ClientError> {
        Ok(vec![SettingsSection {
            scope: SettingsScope::PerServer,
            section_key: "community".to_string(),
            icon: None,
            fields: vec![
                SettingDescriptor {
                    key: "mute-community".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
                    extra: String::new(),
                },
                SettingDescriptor {
                    key: "show-nsfw".to_string(),
                    kind: SettingKind::Toggle,
                    default_value: "false".to_string(),
                    extra: String::new(),
                },
            ],
            info_block: None,
        }])
    }

    fn get_setting_value(
        scope: SettingsScope,
        scope_id: String,
        key: String,
    ) -> Result<String, wit::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        Ok(host_api::storage_get(&storage_key)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| "null".to_string()))
    }

    fn set_setting_value(
        scope: SettingsScope,
        scope_id: String,
        key: String,
        value: String,
    ) -> Result<(), wit::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        host_api::storage_set(&storage_key, value.as_bytes())
            .map_err(wit::ClientError::Internal)
    }
}

// ─── ClientSidebarGuest ───────────────────────────────────────────

impl ClientSidebarGuest for LemmyPlugin {
    fn get_sidebar_declaration() -> Result<SidebarDeclaration, wit::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Communities,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!(
            "unknown sidebar action: {action_id}"
        )))
    }
}

// ─── ClientViewsGuest ─────────────────────────────────────────────

impl ClientViewsGuest for LemmyPlugin {
    fn get_channel_view(
        _channel_id: String,
    ) -> Result<ViewDescriptor, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_channel_view not yet implemented in WASM plugin".to_string(),
        ))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<exports::poly::messenger::client_views::Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<ViewRowsPage, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_view_rows not yet implemented in WASM plugin".to_string(),
        ))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<ViewDetail, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_view_detail not yet implemented in WASM plugin".to_string(),
        ))
    }
}

// ─── ClientComposerGuest ──────────────────────────────────────────

impl ClientComposerGuest for LemmyPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<Vec<ComposerButton>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_message_actions(
        _channel_id: String,
        _message_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        Ok(vec![])
    }

    fn invoke_composer_action(
        action_id: String,
        _channel_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!(
            "unknown composer action: {action_id}"
        )))
    }

    fn invoke_message_action(
        action_id: String,
        _channel_id: String,
        _message_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!(
            "unknown message action: {action_id}"
        )))
    }
}

// ─── Component export registration ────────────────────────────────

export!(LemmyPlugin with_types_in crate::wit_bindings);
