//! WASM Component Model guest implementation for the Stoat messenger plugin.
//!
//! Stub implementation — all methods return "not yet implemented" errors.
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use crate::wit_bindings::{Guest, PluginMetadataGuest, SettingDescriptor, export, wit};

const FTL_EN: &str = "plugin-stoat-title = Stoat\n";
const FTL_DE: &str = "plugin-stoat-title = Stoat\n";
const FTL_FR: &str = "plugin-stoat-title = Stoat\n";
const FTL_ES: &str = "plugin-stoat-title = Stoat\n";

struct StoatPlugin;

impl Guest for StoatPlugin {
    fn authenticate(_credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Stoat client not yet implemented".into(),
        ))
    }

    fn logout() -> Result<(), wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Stoat client not yet implemented".into(),
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
            "Stoat client not yet implemented".into(),
        ))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Stoat reply sending not yet implemented".into(),
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
            "Stoat pin mutation not yet implemented".to_string(),
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

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(vec![])
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

    fn poll_event() -> Option<wit::ClientEvent> {
        None
    }

    fn get_backend_type() -> wit::BackendType {
        wit::BackendType::Stoat
    }

    fn get_backend_name() -> String {
        "Stoat".to_string()
    }
}

impl PluginMetadataGuest for StoatPlugin {
    fn get_translations(locale: String) -> String {
        match locale.as_str() {
            "en" => FTL_EN.to_string(),
            "de" => FTL_DE.to_string(),
            "fr" => FTL_FR.to_string(),
            "es" => FTL_ES.to_string(),
            _ => FTL_EN.to_string(),
        }
    }

    fn get_settings_schema() -> Vec<SettingDescriptor> {
        vec![]
    }

    fn get_display_name_key() -> String {
        "plugin-stoat-title".to_string()
    }

    fn get_icon() -> String {
        "S".to_string()
    }
}

// Register the component export.
export!(StoatPlugin with_types_in crate::wit_bindings);
