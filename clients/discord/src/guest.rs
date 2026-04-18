//! WASM Component Model guest implementation for the Discord messenger plugin.
//!
//! Stub implementation — all methods return "not yet implemented" errors.
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use crate::wit_bindings::{
    ActionOutcome, ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, Guest, PluginManifest, PluginMetadataGuest, SidebarDeclaration,
    SidebarLayoutKind, export, wit,
};

struct DiscordPlugin;

impl Guest for DiscordPlugin {
    fn authenticate(_credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Discord client not yet implemented".into(),
        ))
    }

    fn logout() -> Result<(), wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Discord client not yet implemented".into(),
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
            "Discord client not yet implemented".into(),
        ))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Discord reply sending not yet implemented".into(),
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
            "Discord pin mutation not yet implemented".to_string(),
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
            "Discord WASM open DM not yet implemented".to_string(),
        ))
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Discord WASM saved messages not yet implemented".to_string(),
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
        // TODO(3.3.5): Parse Discord Gateway WebSocket events, call emit-event
    }

    fn get_backend_type() -> String {
        "discord".to_string()
    }

    fn get_backend_name() -> String {
        "Discord".to_string()
    }

    fn get_backend_capabilities() -> wit::BackendCapabilities {
        wit::BackendCapabilities {
            supports_voice: true,
            supports_video: true,
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

    fn list_files(_channel_id: String, _path: String) -> Result<Vec<wit::FileEntry>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "discord has no code channels".to_string(),
        ))
    }

    fn read_file(_channel_id: String, _path: String) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "discord has no code channels".to_string(),
        ))
    }
}

impl PluginMetadataGuest for DiscordPlugin {
    fn get_translations(_locale: String) -> String {
        String::new()
    }

    fn get_display_name_key() -> String {
        "plugin-discord-title".to_string()
    }

    fn get_icon() -> String {
        "💬".to_string()
    }

    fn get_plugin_manifest() -> PluginManifest {
        PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![
                "discord.com".to_string(),
                "cdn.discordapp.com".to_string(),
            ],
            description: "Discord chat backend. Connects to discord.com with a user token. \
                          Dev-only: not shipped in release builds because Discord's ToS \
                          forbids third-party clients on the app store."
                .to_string(),
            homepage: Some("https://discord.com".to_string()),
        }
    }
}

impl ClientMenusGuest for DiscordPlugin {
    fn get_context_menu_items(
        _target: crate::wit_bindings::exports::poly::messenger::client_menus::MenuTargetKind,
        _target_id: String,
    ) -> Result<
        Vec<crate::wit_bindings::exports::poly::messenger::client_menus::MenuItem>,
        wit::ClientError,
    > {
        Ok(Vec::new())
    }

    fn invoke_context_action(
        action_id: String,
        _target: crate::wit_bindings::exports::poly::messenger::client_menus::MenuTargetKind,
        _target_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }

    fn poll_action(
        _handle: crate::wit_bindings::exports::poly::messenger::client_menus::PendingHandle,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Ok(ActionOutcome::Completed)
    }
}

impl ClientSettingsGuest for DiscordPlugin {
    fn get_settings_sections() -> Result<
        Vec<crate::wit_bindings::exports::poly::messenger::client_settings::SettingsSection>,
        wit::ClientError,
    > {
        Ok(Vec::new())
    }

    fn get_setting_value(
        _scope: crate::wit_bindings::exports::poly::messenger::client_settings::SettingsScope,
        _scope_id: String,
        _key: String,
    ) -> Result<String, wit::ClientError> {
        Ok("null".to_string())
    }

    fn set_setting_value(
        _scope: crate::wit_bindings::exports::poly::messenger::client_settings::SettingsScope,
        _scope_id: String,
        _key: String,
        _value: String,
    ) -> Result<(), wit::ClientError> {
        Ok(())
    }
}

impl ClientSidebarGuest for DiscordPlugin {
    fn get_sidebar_declaration() -> Result<SidebarDeclaration, wit::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(action_id: String) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }
}

impl ClientViewsGuest for DiscordPlugin {
    fn get_channel_view(
        _channel_id: String,
    ) -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewDescriptor,
        wit::ClientError,
    > {
        Err(wit::ClientError::NotSupported(
            "discord has no custom views".to_string(),
        ))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<crate::wit_bindings::exports::poly::messenger::client_views::Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewRowsPage,
        wit::ClientError,
    > {
        Err(wit::ClientError::NotSupported(
            "discord has no custom views".to_string(),
        ))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewDetail,
        wit::ClientError,
    > {
        Err(wit::ClientError::NotSupported(
            "discord has no custom views".to_string(),
        ))
    }
}

impl ClientComposerGuest for DiscordPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<
        Vec<crate::wit_bindings::exports::poly::messenger::client_composer::ComposerButton>,
        wit::ClientError,
    > {
        Ok(Vec::new())
    }

    fn get_message_actions(
        _channel_id: String,
        _message_id: String,
    ) -> Result<
        Vec<crate::wit_bindings::exports::poly::messenger::client_menus::MenuItem>,
        wit::ClientError,
    > {
        Ok(Vec::new())
    }

    fn invoke_composer_action(
        action_id: String,
        _channel_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }

    fn invoke_message_action(
        action_id: String,
        _channel_id: String,
        _message_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }
}

export!(DiscordPlugin with_types_in crate::wit_bindings);
