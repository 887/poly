//! WASM Component Model guest implementation for the Poly Server client plugin.
//!
//! Stub implementation — the real WASM version will route HTTP/WebSocket
//! calls through host-api imports. For now, returns minimal-behavior defaults
//! for all six Pack-E WIT surfaces.
//!
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use crate::wit_bindings::{
    ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, MessengerClientGuest, PluginManifest, PluginMetadataGuest, SettingsScope,
    export, poly::messenger::host_api, wit,
};

// ─── Plugin struct ─────────────────────────────────────────────────

struct PolyServerPlugin;

// ─── messenger-client ──────────────────────────────────────────────

impl MessengerClientGuest for PolyServerPlugin {
    fn authenticate(_credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Poly Server WASM impl not yet complete".to_string(),
        ))
    }

    fn logout() -> Result<(), wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Poly Server WASM impl not yet complete".to_string(),
        ))
    }

    fn is_authenticated() -> bool {
        false
    }

    fn get_servers() -> Result<Vec<wit::Server>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_server(id: String) -> Result<wit::Server, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!("Server {id}")))
    }

    fn get_channels(_server_id: String) -> Result<Vec<wit::Channel>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_channel(id: String) -> Result<wit::Channel, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!("Channel {id}")))
    }

    fn send_message(
        _channel_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Poly Server WASM impl not yet complete".to_string(),
        ))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Poly Server WASM impl not yet complete".to_string(),
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

    fn get_pinned_messages(_channel_id: String) -> Result<Vec<wit::Message>, wit::ClientError> {
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
            "pin mutation not yet implemented".to_string(),
        ))
    }

    fn get_user(id: String) -> Result<wit::User, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!("User {id}")))
    }

    fn get_friends() -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_channel_members(_channel_id: String) -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_groups() -> Result<Vec<wit::Group>, wit::ClientError> {
        Ok(vec![])
    }

    fn remove_group_member(_group_id: String, _user_id: String) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn add_group_member(_group_id: String, _user_id: String) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(vec![])
    }

    fn open_direct_message_channel(_user_id: String) -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "open DM not yet implemented".to_string(),
        ))
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "saved messages not yet implemented".to_string(),
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
        Ok(())
    }

    fn handle_ws_data(_handle: u64, _data: Vec<u8>) {
        // TODO: Parse Poly server WebSocket events and call host emit-event.
    }

    fn get_backend_type() -> String {
        "poly".to_string()
    }

    fn get_backend_name() -> String {
        "Poly Server".to_string()
    }

    fn get_backend_capabilities() -> wit::BackendCapabilities {
        wit::BackendCapabilities {
            supports_voice: false,
            supports_video: false,
            supports_dms: true,
            supports_groups: true,
            supports_send_messages: true,
            supports_presence: true,
            supports_search: true,
            supports_reactions: true,
            supports_typing_indicators: true,
            supports_file_upload: true,
            landing: wit::LandingPage::FirstServer,
        }
    }

    fn list_files(
        _channel_id: String,
        _path: String,
    ) -> Result<Vec<wit::FileEntry>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "poly-server has no code channels".to_string(),
        ))
    }

    fn read_file(
        _channel_id: String,
        _path: String,
    ) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "poly-server has no code channels".to_string(),
        ))
    }

    fn get_forum_posts(
        _forum_channel_id: String,
        _sort: wit::ForumSortOrder,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ForumPost>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_forum_posts not implemented".to_string(),
        ))
    }

    fn get_active_threads(
        _server_id: String,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_active_threads not implemented".to_string(),
        ))
    }

    fn get_archived_threads(
        _parent_channel_id: String,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_archived_threads not implemented".to_string(),
        ))
    }

    fn create_forum_post(
        _forum_channel_id: String,
        _title: String,
        _body: String,
        _tags: Vec<String>,
    ) -> Result<wit::ForumPost, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "create_forum_post not implemented".to_string(),
        ))
    }
}

// ─── client-menus ─────────────────────────────────────────────────

use crate::wit_bindings::exports::poly::messenger::client_menus as menus;

impl ClientMenusGuest for PolyServerPlugin {
    fn get_context_menu_items(
        _target: menus::MenuTargetKind,
        _target_id: String,
    ) -> Result<Vec<menus::MenuItem>, menus::ClientError> {
        Ok(vec![])
    }

    fn invoke_context_action(
        action_id: String,
        _target: menus::MenuTargetKind,
        _target_id: String,
    ) -> Result<menus::ActionOutcome, menus::ClientError> {
        Err(menus::ClientError::NotFound(action_id))
    }

    fn poll_action(
        _handle: menus::PendingHandle,
    ) -> Result<menus::ActionOutcome, menus::ClientError> {
        Err(menus::ClientError::NotSupported(
            "no pending actions".to_string(),
        ))
    }
}

// ─── client-settings ──────────────────────────────────────────────

use crate::wit_bindings::exports::poly::messenger::client_settings as settings;

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

impl ClientSettingsGuest for PolyServerPlugin {
    fn get_settings_sections(
    ) -> Result<Vec<settings::SettingsSection>, settings::ClientError> {
        Ok(vec![])
    }

    fn get_setting_value(
        scope: settings::SettingsScope,
        scope_id: String,
        key: String,
    ) -> Result<String, wit::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        Ok(host_api::storage_get(&storage_key)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| "null".to_string()))
    }

    fn set_setting_value(
        scope: settings::SettingsScope,
        scope_id: String,
        key: String,
        value: String,
    ) -> Result<(), wit::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        host_api::storage_set(&storage_key, value.as_bytes())
            .map_err(wit::ClientError::Internal)
    }
}

// ─── client-sidebar ───────────────────────────────────────────────

use crate::wit_bindings::exports::poly::messenger::client_sidebar as sidebar;

impl ClientSidebarGuest for PolyServerPlugin {
    fn get_sidebar_declaration(
    ) -> Result<sidebar::SidebarDeclaration, sidebar::ClientError> {
        Ok(sidebar::SidebarDeclaration {
            layout: sidebar::SidebarLayoutKind::ChannelList,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: String,
    ) -> Result<sidebar::ActionOutcome, sidebar::ClientError> {
        Err(sidebar::ClientError::NotFound(action_id))
    }
}

// ─── client-views ─────────────────────────────────────────────────

use crate::wit_bindings::exports::poly::messenger::client_views as views;

impl ClientViewsGuest for PolyServerPlugin {
    fn get_channel_view(
        channel_id: String,
    ) -> Result<views::ViewDescriptor, views::ClientError> {
        Err(views::ClientError::NotSupported(format!(
            "channel {channel_id} has no custom view"
        )))
    }

    fn get_view_rows(
        channel_id: String,
        _cursor: Option<views::Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<views::ViewRowsPage, views::ClientError> {
        Err(views::ClientError::NotSupported(format!(
            "channel {channel_id} has no custom view"
        )))
    }

    fn get_view_detail(
        channel_id: String,
        row_id: String,
    ) -> Result<views::ViewDetail, views::ClientError> {
        Err(views::ClientError::NotFound(format!(
            "{channel_id}/{row_id}"
        )))
    }
}

// ─── client-composer ──────────────────────────────────────────────

use crate::wit_bindings::exports::poly::messenger::client_composer as composer;

impl ClientComposerGuest for PolyServerPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<Vec<composer::ComposerButton>, composer::ClientError> {
        Ok(vec![])
    }

    fn get_message_actions(
        _channel_id: String,
        _message_id: String,
    ) -> Result<Vec<composer::MenuItem>, composer::ClientError> {
        Ok(vec![])
    }

    fn invoke_composer_action(
        action_id: String,
        _channel_id: String,
    ) -> Result<composer::ActionOutcome, composer::ClientError> {
        Err(composer::ClientError::NotFound(action_id))
    }

    fn invoke_message_action(
        action_id: String,
        _channel_id: String,
        _message_id: String,
    ) -> Result<composer::ActionOutcome, composer::ClientError> {
        Err(composer::ClientError::NotFound(action_id))
    }
}

// ─── plugin-metadata ──────────────────────────────────────────────

impl PluginMetadataGuest for PolyServerPlugin {
    fn get_translations(locale: String) -> String {
        match locale.as_str() {
            "de" => include_str!("../locales/de/plugin.ftl").to_string(),
            "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
            "es" => include_str!("../locales/es/plugin.ftl").to_string(),
            // en or any unknown locale → English (same contract as WIT)
            _ => include_str!("../locales/en/plugin.ftl").to_string(),
        }
    }

    fn get_display_name_key() -> String {
        "plugin-poly-server-title".to_string()
    }

    fn get_icon() -> String {
        "\u{1F510}".to_string()
    }

    fn get_plugin_manifest() -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![],
            description: "Poly Server backend — HTTP + WebSocket client for poly-server instances."
                .to_string(),
            homepage: None,
        }
    }
}

// ─── Component export registration ────────────────────────────────

export!(PolyServerPlugin with_types_in crate::wit_bindings);
