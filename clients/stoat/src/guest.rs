//! WASM Component Model guest implementation for the Stoat messenger plugin.
//!
//! Partial real implementation using host-mediated HTTP requests.
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;

use crate::wit_bindings::{Guest, PluginMetadataGuest, SettingDescriptor, export, wit};
use serde::{Deserialize, Serialize};

const OFFICIAL_STOAT_BASE_URL: &str = "https://api.stoat.chat";
const STOAT_SESSION_TOKEN_HEADER: &str = "x-session-token";

#[derive(Debug, Clone)]
struct StoredSession {
    session_id: String,
    token: String,
    user_id: String,
}

thread_local! {
    static STATE: RefCell<Option<StoredSession>> = const { RefCell::new(None) };
}

#[derive(Debug, Deserialize)]
#[serde(tag = "result")]
enum StoatLoginResponse {
    Success {
        #[serde(rename = "_id")]
        id: String,
        user_id: String,
        token: String,
        name: String,
    },
    #[serde(rename = "MFA")]
    Mfa {
        allowed_methods: Vec<String>,
    },
    Disabled {
        user_id: String,
    },
}

#[derive(Debug, Deserialize)]
struct StoatGuestUser {
    #[serde(rename = "_id")]
    id: String,
    username: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    online: bool,
    #[serde(default)]
    status: Option<StoatGuestStatus>,
}

#[derive(Debug, Deserialize)]
struct StoatGuestChannel {
    channel_type: String,
    #[serde(rename = "_id")]
    id: String,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    recipients: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct StoatGuestStatus {
    #[serde(default)]
    presence: Option<StoatGuestPresence>,
}

#[derive(Debug, Deserialize)]
enum StoatGuestPresence {
    Online,
    Idle,
    Focus,
    Busy,
    Invisible,
}

#[derive(Debug, Serialize)]
struct StoatGuestPasswordLoginRequest {
    email: String,
    password: String,
    friendly_name: Option<String>,
}

fn map_presence(user: &StoatGuestUser) -> wit::PresenceStatus {
    match user
        .status
        .as_ref()
        .and_then(|status| status.presence.as_ref())
    {
        Some(StoatGuestPresence::Online) => wit::PresenceStatus::Online,
        Some(StoatGuestPresence::Idle) => wit::PresenceStatus::Idle,
        Some(StoatGuestPresence::Focus) | Some(StoatGuestPresence::Busy) => {
            wit::PresenceStatus::DoNotDisturb
        }
        Some(StoatGuestPresence::Invisible) => wit::PresenceStatus::Invisible,
        None => {
            if user.online {
                wit::PresenceStatus::Online
            } else {
                wit::PresenceStatus::Offline
            }
        }
    }
}

fn instance_id_for_base_url(base_url: &str) -> String {
    base_url
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .replace('/', "~")
}

fn to_session(
    user: StoatGuestUser,
    token: String,
    session_id: String,
    base_url: &str,
) -> wit::Session {
    let display_name = user
        .display_name
        .clone()
        .unwrap_or_else(|| user.username.clone());
    let wit_user = wit::User {
        id: user.id.clone(),
        display_name,
        avatar_url: None,
        presence: map_presence(&user),
        backend: wit::BackendType::Stoat,
    };

    let instance_id = instance_id_for_base_url(base_url);
    STATE.with(|state| {
        state.replace(Some(StoredSession {
            session_id: session_id.clone(),
            token: token.clone(),
            user_id: user.id.clone(),
        }));
    });

    wit::Session {
        id: session_id,
        user: wit_user,
        token,
        backend: wit::BackendType::Stoat,
        icon_emoji: Some("🦦".to_string()),
        instance_id,
        backend_url: Some(base_url.to_string()),
    }
}

fn host_http_request(
    method: &str,
    url: &str,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
) -> Result<crate::wit_bindings::poly::messenger::types::HttpResponse, wit::ClientError> {
    Ok(
        crate::wit_bindings::poly::messenger::host_api::http_request(
            method,
            url,
            &headers,
            body.as_deref(),
        )
        .map_err(wit::ClientError::Internal)?,
    )
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    response: &crate::wit_bindings::poly::messenger::types::HttpResponse,
) -> Result<T, wit::ClientError> {
    serde_json::from_slice(&response.body)
        .map_err(|err| wit::ClientError::Internal(format!("invalid Stoat guest JSON: {err}")))
}

fn fetch_self(base_url: &str, token: &str) -> Result<StoatGuestUser, wit::ClientError> {
    let response = host_http_request(
        "GET",
        &format!("{base_url}/users/@me"),
        vec![(STOAT_SESSION_TOKEN_HEADER.to_string(), token.to_string())],
        None,
    )?;

    if !matches!(response.status, 200..=299) {
        return Err(match response.status {
            401 => wit::ClientError::AuthFailed("Stoat token rejected".to_string()),
            status => wit::ClientError::Network(format!("Stoat /users/@me returned HTTP {status}")),
        });
    }

    parse_json(&response)
}

fn current_session() -> Result<StoredSession, wit::ClientError> {
    STATE.with(|state| {
        state
            .borrow()
            .clone()
            .ok_or_else(|| wit::ClientError::AuthFailed("Stoat guest is not authenticated".to_string()))
    })
}

fn stoat_auth_headers(token: &str) -> Vec<(String, String)> {
    vec![(STOAT_SESSION_TOKEN_HEADER.to_string(), token.to_string())]
}

fn fetch_user(base_url: &str, token: &str, user_id: &str) -> Result<StoatGuestUser, wit::ClientError> {
    let response = host_http_request(
        "GET",
        &format!("{base_url}/users/{user_id}"),
        stoat_auth_headers(token),
        None,
    )?;

    if !matches!(response.status, 200..=299) {
        return Err(match response.status {
            401 => wit::ClientError::AuthFailed("Stoat token rejected".to_string()),
            404 => wit::ClientError::NotFound(format!("Stoat user {user_id} not found")),
            status => wit::ClientError::Network(format!("Stoat /users/{user_id} returned HTTP {status}")),
        });
    }

    parse_json(&response)
}

fn fetch_open_dm_channel(
    base_url: &str,
    token: &str,
    user_id: &str,
) -> Result<StoatGuestChannel, wit::ClientError> {
    let response = host_http_request(
        "GET",
        &format!("{base_url}/users/{user_id}/dm"),
        stoat_auth_headers(token),
        None,
    )?;

    if !matches!(response.status, 200..=299) {
        return Err(match response.status {
            401 => wit::ClientError::AuthFailed("Stoat token rejected".to_string()),
            404 => wit::ClientError::NotFound(format!("Stoat DM target {user_id} not found")),
            status => wit::ClientError::Network(format!("Stoat /users/{user_id}/dm returned HTTP {status}")),
        });
    }

    parse_json(&response)
}

fn mutate_group_member(
    method: &str,
    group_id: &str,
    user_id: &str,
) -> Result<(), wit::ClientError> {
    let session = current_session()?;
    let response = host_http_request(
        method,
        &format!(
            "{OFFICIAL_STOAT_BASE_URL}/channels/{group_id}/recipients/{user_id}"
        ),
        stoat_auth_headers(&session.token),
        None,
    )?;

    if !matches!(response.status, 200..=299) {
        return Err(match response.status {
            401 => wit::ClientError::AuthFailed("Stoat token rejected".to_string()),
            404 => wit::ClientError::NotFound(format!(
                "Stoat group/member path {group_id}/{user_id} not found"
            )),
            status => wit::ClientError::Network(format!(
                "Stoat {method} /channels/{group_id}/recipients/{user_id} returned HTTP {status}"
            )),
        });
    }

    Ok(())
}

fn to_wit_user(user: &StoatGuestUser) -> wit::User {
    wit::User {
        id: user.id.clone(),
        display_name: user
            .display_name
            .clone()
            .unwrap_or_else(|| user.username.clone()),
        avatar_url: None,
        presence: map_presence(user),
        backend: wit::BackendType::Stoat,
    }
}

fn open_dm_like_channel(user_id: &str) -> Result<wit::DmChannel, wit::ClientError> {
    let session = current_session()?;
    let channel = fetch_open_dm_channel(OFFICIAL_STOAT_BASE_URL, &session.token, user_id)?;

    let dm_user = if channel.channel_type == "SavedMessages" {
        fetch_self(OFFICIAL_STOAT_BASE_URL, &session.token)?
    } else {
        let other_user_id = channel
            .recipients
            .clone()
            .unwrap_or_default()
            .into_iter()
            .find(|candidate| candidate != &session.user_id)
            .or(channel.user.clone())
            .ok_or_else(|| {
                wit::ClientError::Internal(format!(
                    "Stoat DM channel {} is missing the other participant",
                    channel.id
                ))
            })?;
        fetch_user(OFFICIAL_STOAT_BASE_URL, &session.token, &other_user_id)?
    };

    Ok(wit::DmChannel {
        id: channel.id,
        user: to_wit_user(&dm_user),
        last_message: None,
        unread_count: 0,
        backend: wit::BackendType::Stoat,
        account_id: session.user_id,
    })
}

const FTL_EN: &str = "plugin-stoat-title = Stoat\n";
const FTL_DE: &str = "plugin-stoat-title = Stoat\n";
const FTL_FR: &str = "plugin-stoat-title = Stoat\n";
const FTL_ES: &str = "plugin-stoat-title = Stoat\n";

struct StoatPlugin;

impl Guest for StoatPlugin {
    fn authenticate(credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        match credentials {
            wit::AuthCredentials::Token(token) => {
                let user = fetch_self(OFFICIAL_STOAT_BASE_URL, &token)?;
                Ok(to_session(
                    user,
                    token,
                    STATE.with(|state| {
                        state.borrow().as_ref().map_or_else(
                            || "stoat-token-session".to_string(),
                            |stored| stored.session_id.clone(),
                        )
                    }),
                    OFFICIAL_STOAT_BASE_URL,
                ))
            }
            wit::AuthCredentials::EmailPassword(creds) => {
                let response = host_http_request(
                    "POST",
                    &format!("{OFFICIAL_STOAT_BASE_URL}/auth/session/login"),
                    vec![("content-type".to_string(), "application/json".to_string())],
                    Some(
                        serde_json::to_vec(&StoatGuestPasswordLoginRequest {
                            email: creds.email,
                            password: creds.password,
                            friendly_name: Some("Poly".to_string()),
                        })
                        .map_err(|err| {
                            wit::ClientError::Internal(format!(
                                "failed to encode Stoat login body: {err}"
                            ))
                        })?,
                    ),
                )?;

                if !matches!(response.status, 200..=299) {
                    return Err(match response.status {
                        401 => wit::ClientError::AuthFailed(
                            "Stoat email/password rejected".to_string(),
                        ),
                        status => {
                            wit::ClientError::Network(format!("Stoat login returned HTTP {status}"))
                        }
                    });
                }

                let login: StoatLoginResponse = parse_json(&response)?;
                match login {
                    StoatLoginResponse::Success {
                        id,
                        user_id: _user_id,
                        token,
                        name: _name,
                    } => {
                        let user = fetch_self(OFFICIAL_STOAT_BASE_URL, &token)?;
                        Ok(to_session(user, token, id, OFFICIAL_STOAT_BASE_URL))
                    }
                    StoatLoginResponse::Mfa { allowed_methods } => {
                        Err(wit::ClientError::AuthFailed(format!(
                            "Stoat requires MFA before login can continue (allowed methods: {})",
                            allowed_methods.join(", ")
                        )))
                    }
                    StoatLoginResponse::Disabled { user_id } => Err(wit::ClientError::AuthFailed(
                        format!("Stoat account is disabled for user {user_id}"),
                    )),
                }
            }
            _ => Err(wit::ClientError::NotSupported(
                "Stoat guest currently supports token and email/password auth only".into(),
            )),
        }
    }

    fn logout() -> Result<(), wit::ClientError> {
        STATE.with(|state| state.replace(None));
        Ok(())
    }

    fn is_authenticated() -> bool {
        STATE.with(|state| state.borrow().is_some())
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

    fn remove_group_member(group_id: String, user_id: String) -> Result<(), wit::ClientError> {
        mutate_group_member("DELETE", &group_id, &user_id)
    }

    fn add_group_member(group_id: String, user_id: String) -> Result<(), wit::ClientError> {
        mutate_group_member("PUT", &group_id, &user_id)
    }

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(vec![])
    }

    fn open_direct_message_channel(user_id: String) -> Result<wit::DmChannel, wit::ClientError> {
        open_dm_like_channel(&user_id)
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        let session = current_session()?;
        open_dm_like_channel(&session.user_id)
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
