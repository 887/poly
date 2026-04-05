//! WASM Component Model guest implementation for the Matrix messenger plugin.
//!
//! Partial real implementation using host-mediated HTTP requests.
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;

use crate::wit_bindings::{Guest, PluginMetadataGuest, SettingDescriptor, export, wit};
use serde::{Deserialize, Serialize};

const DEFAULT_HOMESERVER: &str = "https://matrix.org";

#[derive(Debug, Clone)]
struct StoredSession {
    access_token: String,
    device_id: String,
    user_id: String,
}

thread_local! {
    static STATE: RefCell<Option<StoredSession>> = const { RefCell::new(None) };
}

#[derive(Debug, Serialize)]
struct MatrixLoginRequest {
    #[serde(rename = "type")]
    login_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    identifier: Option<MatrixLoginIdentifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_device_display_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct MatrixLoginIdentifier {
    #[serde(rename = "type")]
    id_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MatrixLoginResponse {
    user_id: String,
    access_token: String,
    device_id: String,
}

#[derive(Debug, Deserialize)]
struct MatrixWhoAmIResponse {
    user_id: String,
    #[serde(default)]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MatrixProfileResponse {
    #[serde(default)]
    displayname: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
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
        .map_err(|err| wit::ClientError::Internal(format!("invalid Matrix guest JSON: {err}")))
}

fn current_session() -> Result<StoredSession, wit::ClientError> {
    STATE.with(|state| {
        state.borrow().clone().ok_or_else(|| {
            wit::ClientError::AuthFailed("Matrix guest is not authenticated".to_string())
        })
    })
}

fn matrix_auth_headers(token: &str) -> Vec<(String, String)> {
    vec![("authorization".to_string(), format!("Bearer {token}"))]
}

fn instance_id_for_homeserver(homeserver: &str) -> String {
    homeserver
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .replace('/', "~")
}

fn fetch_profile(
    homeserver: &str,
    token: &str,
    user_id: &str,
) -> Result<MatrixProfileResponse, wit::ClientError> {
    let response = host_http_request(
        "GET",
        &format!("{homeserver}/_matrix/client/v3/profile/{user_id}"),
        matrix_auth_headers(token),
        None,
    )?;

    if !matches!(response.status, 200..=299) {
        return Err(match response.status {
            401 => wit::ClientError::AuthFailed("Matrix token rejected".to_string()),
            404 => wit::ClientError::NotFound(format!("Matrix user {user_id} not found")),
            status => wit::ClientError::Network(format!(
                "Matrix /profile/{user_id} returned HTTP {status}"
            )),
        });
    }

    parse_json(&response)
}

struct MatrixPlugin;

impl Guest for MatrixPlugin {
    fn authenticate(credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        match credentials {
            wit::AuthCredentials::Token(token) => {
                // Validate the token by calling whoami
                let response = host_http_request(
                    "GET",
                    &format!("{DEFAULT_HOMESERVER}/_matrix/client/v3/account/whoami"),
                    matrix_auth_headers(&token),
                    None,
                )?;

                if !matches!(response.status, 200..=299) {
                    return Err(match response.status {
                        401 => {
                            wit::ClientError::AuthFailed("Matrix token rejected".to_string())
                        }
                        status => wit::ClientError::Network(format!(
                            "Matrix /account/whoami returned HTTP {status}"
                        )),
                    });
                }

                let whoami: MatrixWhoAmIResponse = parse_json(&response)?;
                let device_id = whoami.device_id.unwrap_or_default();

                // Fetch profile for display name
                let profile = fetch_profile(DEFAULT_HOMESERVER, &token, &whoami.user_id)?;
                let display_name = profile
                    .displayname
                    .unwrap_or_else(|| whoami.user_id.clone());

                let instance_id = instance_id_for_homeserver(DEFAULT_HOMESERVER);

                STATE.with(|state| {
                    state.replace(Some(StoredSession {
                        access_token: token.clone(),
                        device_id: device_id.clone(),
                        user_id: whoami.user_id.clone(),
                    }));
                });

                Ok(wit::Session {
                    id: format!("{}-{device_id}", whoami.user_id),
                    user: wit::User {
                        id: whoami.user_id.clone(),
                        display_name,
                        avatar_url: profile.avatar_url,
                        presence: wit::PresenceStatus::Online,
                        backend: wit::BackendType::from("matrix"),
                    },
                    token,
                    backend: wit::BackendType::from("matrix"),
                    icon_emoji: Some("\u{1f7e6}".to_string()),
                    instance_id,
                    backend_url: Some(DEFAULT_HOMESERVER.to_string()),
                })
            }
            wit::AuthCredentials::EmailPassword(creds) => {
                // Matrix uses the "email" field as the Matrix user ID / username
                let login_body = MatrixLoginRequest {
                    login_type: "m.login.password".to_string(),
                    identifier: Some(MatrixLoginIdentifier {
                        id_type: "m.id.user".to_string(),
                        user: Some(creds.email),
                    }),
                    password: Some(creds.password),
                    token: None,
                    initial_device_display_name: Some("Poly".to_string()),
                };

                let response = host_http_request(
                    "POST",
                    &format!("{DEFAULT_HOMESERVER}/_matrix/client/v3/login"),
                    vec![("content-type".to_string(), "application/json".to_string())],
                    Some(serde_json::to_vec(&login_body).map_err(|err| {
                        wit::ClientError::Internal(format!(
                            "failed to encode Matrix login body: {err}"
                        ))
                    })?),
                )?;

                if !matches!(response.status, 200..=299) {
                    return Err(match response.status {
                        401 | 403 => wit::ClientError::AuthFailed(
                            "Matrix username/password rejected".to_string(),
                        ),
                        status => wit::ClientError::Network(format!(
                            "Matrix login returned HTTP {status}"
                        )),
                    });
                }

                let login: MatrixLoginResponse = parse_json(&response)?;

                // Fetch profile for display name
                let profile =
                    fetch_profile(DEFAULT_HOMESERVER, &login.access_token, &login.user_id)?;
                let display_name = profile
                    .displayname
                    .unwrap_or_else(|| login.user_id.clone());

                let instance_id = instance_id_for_homeserver(DEFAULT_HOMESERVER);

                STATE.with(|state| {
                    state.replace(Some(StoredSession {
                        access_token: login.access_token.clone(),
                        device_id: login.device_id.clone(),
                        user_id: login.user_id.clone(),
                    }));
                });

                Ok(wit::Session {
                    id: format!("{}-{}", login.user_id, login.device_id),
                    user: wit::User {
                        id: login.user_id.clone(),
                        display_name,
                        avatar_url: profile.avatar_url,
                        presence: wit::PresenceStatus::Online,
                        backend: wit::BackendType::from("matrix"),
                    },
                    token: login.access_token,
                    backend: wit::BackendType::from("matrix"),
                    icon_emoji: Some("\u{1f7e6}".to_string()),
                    instance_id,
                    backend_url: Some(DEFAULT_HOMESERVER.to_string()),
                })
            }
            _ => Err(wit::ClientError::NotSupported(
                "Matrix guest currently supports token and email/password auth only".into(),
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
            "Matrix client not yet implemented".into(),
        ))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::Internal(
            "Matrix reply sending not yet implemented".into(),
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
            "Matrix pin mutation not yet implemented".to_string(),
        ))
    }

    fn get_user(id: String) -> Result<wit::User, wit::ClientError> {
        let session = current_session()?;
        let profile = fetch_profile(DEFAULT_HOMESERVER, &session.access_token, &id)?;
        let display_name = profile.displayname.unwrap_or_else(|| id.clone());

        Ok(wit::User {
            id,
            display_name,
            avatar_url: profile.avatar_url,
            presence: wit::PresenceStatus::Offline,
            backend: wit::BackendType::from("matrix"),
        })
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
            "Matrix WASM open DM not yet implemented".to_string(),
        ))
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Matrix WASM saved messages not yet implemented".to_string(),
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
        // Matrix uses HTTP long-poll (/sync), not WebSocket.
        // Events are emitted during sync HTTP response processing.
    }

    fn get_backend_type() -> wit::BackendType {
        wit::BackendType::from("matrix")
    }

    fn get_backend_name() -> String {
        "Matrix".to_string()
    }
}

impl PluginMetadataGuest for MatrixPlugin {
    fn get_translations(locale: String) -> String {
        match locale.as_str() {
            "de" => include_str!("../locales/de/plugin.ftl").to_string(),
            "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
            "es" => include_str!("../locales/es/plugin.ftl").to_string(),
            _ => include_str!("../locales/en/plugin.ftl").to_string(),
        }
    }

    fn get_settings_schema() -> Vec<SettingDescriptor> {
        vec![]
    }

    fn get_display_name_key() -> String {
        "plugin-matrix-title".to_string()
    }

    fn get_icon() -> String {
        "🟦".to_string()
    }
}

export!(MatrixPlugin with_types_in crate::wit_bindings);
