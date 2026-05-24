//! WASM Component Model guest implementation for the Stoat messenger plugin.
//!
//! Partial real implementation using host-mediated HTTP requests.
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashSet;

use crate::wit_bindings::{
    ActionOutcome, ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, Cursor, Guest, MenuItem, MenuTargetKind, PendingHandle, PluginMetadataGuest,
    SidebarDeclaration, SidebarLayoutKind, SettingsScope, export,
    poly::messenger::host_api,
    wit,
};
use serde::{Deserialize, Serialize};

const OFFICIAL_STOAT_BASE_URL: &str = "https://api.stoat.chat";
const STOAT_SESSION_TOKEN_HEADER: &str = "x-session-token";

#[derive(Debug, Clone)]
struct StoredSession {
    session_id: String,
    token: String,
    user_id: String,
}

/// In-memory state for context-menu toggle actions (F10).
/// Persistent storage is F9 — out of scope here.
struct StoatMenuState {
    muted_channels: HashSet<String>,
    muted_servers: HashSet<String>,
    blocked_users: HashSet<String>,
    friends: HashSet<String>,
    closed_dms: HashSet<String>,
    muted_dms: HashSet<String>,
}

thread_local! {
    static STATE: RefCell<Option<StoredSession>> = const { RefCell::new(None) };
    static MENU_STATE: RefCell<StoatMenuState> = RefCell::new(StoatMenuState {
        muted_channels: HashSet::new(),
        muted_servers: HashSet::new(),
        blocked_users: HashSet::new(),
        friends: HashSet::new(),
        closed_dms: HashSet::new(),
        muted_dms: HashSet::new(),
    });
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
        backend: "stoat".to_string(),
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
        backend: "stoat".to_string(),
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
        state.borrow().clone().ok_or_else(|| {
            wit::ClientError::AuthFailed("Stoat guest is not authenticated".to_string())
        })
    })
}

fn stoat_auth_headers(token: &str) -> Vec<(String, String)> {
    vec![(STOAT_SESSION_TOKEN_HEADER.to_string(), token.to_string())]
}

fn fetch_user(
    base_url: &str,
    token: &str,
    user_id: &str,
) -> Result<StoatGuestUser, wit::ClientError> {
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
            status => {
                wit::ClientError::Network(format!("Stoat /users/{user_id} returned HTTP {status}"))
            }
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
            status => wit::ClientError::Network(format!(
                "Stoat /users/{user_id}/dm returned HTTP {status}"
            )),
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
        &format!("{OFFICIAL_STOAT_BASE_URL}/channels/{group_id}/recipients/{user_id}"),
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
        backend: "stoat".to_string(),
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
        backend: "stoat".to_string(),
        account_id: session.user_id,
    })
}

const FTL_EN: &str = include_str!("../locales/en/plugin.ftl");
const FTL_DE: &str = include_str!("../locales/de/plugin.ftl");
const FTL_FR: &str = include_str!("../locales/fr/plugin.ftl");
const FTL_ES: &str = include_str!("../locales/es/plugin.ftl");

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

    // G.5 — voice-transport WIT stubs. Real Stoat voice work lives in
    // native `lib.rs` (Vortex WS + PATCH /channels/{id}/voice_state); this
    // sandboxed WIT-plugin variant cannot reach real transport, so it
    // returns the documented default-impl outcomes from
    // `VoiceTransportBackend`.
    fn join_voice_channel_transport(
        _server_id: String,
        _channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn start_dm_call_transport(
        _dm_channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "start_dm_call_transport (WIT stub)".into(),
        ))
    }

    fn set_voice_mute(
        _server_id: String,
        _channel_id: String,
        _self_mute: bool,
        _self_deaf: bool,
    ) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn get_presence(_user_id: String) -> Result<wit::PresenceStatus, wit::ClientError> {
        Ok(wit::PresenceStatus::Offline)
    }

    fn set_presence(_status: wit::PresenceStatus) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn handle_ws_data(_handle: u64, data: Vec<u8>) {
        // SOLID-audit-stoat D.1 — Bonfire WebSocket event parser for the WIT
        // guest. Mirrors the native parser in `is_backend.rs::parse_bonfire_event`
        // (same event variants, same field shapes) but maps into `wit::ClientEvent`
        // instead of `poly_client::ClientEvent` and pushes each event back to the
        // host through `host_api::emit_event`. Bonfire frames are JSON text; we
        // accept either single-event payloads or arrays (the test-stoat mock
        // sometimes batches events for replay scenarios).
        let Ok(text) = std::str::from_utf8(&data) else {
            host_api::log(
                wit::LogLevel::Warn,
                "stoat handle_ws_data: payload is not valid UTF-8",
            );
            return;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            host_api::log(
                wit::LogLevel::Warn,
                "stoat handle_ws_data: payload is not valid JSON",
            );
            return;
        };
        if let Some(arr) = json.as_array() {
            for ev in arr {
                if let Some(client_event) = bonfire_event_to_wit(ev) {
                    host_api::emit_event(&client_event);
                }
            }
        } else if let Some(client_event) = bonfire_event_to_wit(&json) {
            host_api::emit_event(&client_event);
        }
    }

    fn get_backend_type() -> String {
        "stoat".to_string()
    }

    fn get_backend_name() -> String {
        "Stoat".to_string()
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
            "stoat has no code channels".to_string(),
        ))
    }

    fn read_file(_channel_id: String, _path: String) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "stoat has no code channels".to_string(),
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

    fn get_display_name_key() -> String {
        "plugin-stoat-title".to_string()
    }

    fn get_icon() -> String {
        "S".to_string()
    }

    fn get_plugin_manifest() -> crate::wit_bindings::PluginManifest {
        crate::wit_bindings::PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["stoat.chat".to_string()],
            description: "Connect to Stoat, a self-hosted instant messaging platform. \
                          Supports text channels, group DMs, and presence status."
                .to_string(),
            homepage: Some("https://stoat.chat".to_string()),
        }
    }
}

fn guest_menu_item(
    id: &str,
    label_key: &str,
    slot: crate::wit_bindings::exports::poly::messenger::client_menus::MenuSlot,
) -> MenuItem {
    MenuItem {
        id: id.to_string(),
        parent_id: None,
        slot,
        label_key: label_key.to_string(),
        icon: None,
        item_variant: crate::wit_bindings::exports::poly::messenger::client_menus::MenuItemVariant::Normal,
        shortcut: None,
        block: None,
    }
}

fn guest_menu_item_destructive(
    id: &str,
    label_key: &str,
    slot: crate::wit_bindings::exports::poly::messenger::client_menus::MenuSlot,
) -> MenuItem {
    MenuItem {
        id: id.to_string(),
        parent_id: None,
        slot,
        label_key: label_key.to_string(),
        icon: None,
        item_variant: crate::wit_bindings::exports::poly::messenger::client_menus::MenuItemVariant::Destructive,
        shortcut: None,
        block: None,
    }
}

impl ClientMenusGuest for StoatPlugin {
    fn get_context_menu_items(
        target: MenuTargetKind,
        target_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        use crate::wit_bindings::exports::poly::messenger::client_menus::MenuSlot;

        match target {
            MenuTargetKind::Channel => {
                let is_muted =
                    MENU_STATE.with(|s| s.borrow().muted_channels.contains(&target_id));
                let mute_item = if is_muted {
                    guest_menu_item(
                        "unmute-channel",
                        "plugin-stoat-menu-unmute-channel-label",
                        MenuSlot::AfterFavorites,
                    )
                } else {
                    guest_menu_item(
                        "mute-channel",
                        "plugin-stoat-menu-mute-channel-label",
                        MenuSlot::AfterFavorites,
                    )
                };
                Ok(vec![
                    mute_item,
                    guest_menu_item(
                        "mark-channel-read",
                        "plugin-stoat-menu-mark-channel-read-label",
                        MenuSlot::AfterFavorites,
                    ),
                ])
            }
            MenuTargetKind::Server => {
                let is_muted =
                    MENU_STATE.with(|s| s.borrow().muted_servers.contains(&target_id));
                let mute_item = if is_muted {
                    guest_menu_item(
                        "unmute-server",
                        "plugin-stoat-menu-unmute-server-label",
                        MenuSlot::AfterFavorites,
                    )
                } else {
                    guest_menu_item(
                        "mute-server",
                        "plugin-stoat-menu-mute-server-label",
                        MenuSlot::AfterFavorites,
                    )
                };
                Ok(vec![
                    guest_menu_item(
                        "invite-people",
                        "plugin-stoat-menu-invite-people-label",
                        MenuSlot::AfterFavorites,
                    ),
                    guest_menu_item(
                        "privacy-settings",
                        "plugin-stoat-menu-privacy-settings-label",
                        MenuSlot::AfterFavorites,
                    ),
                    guest_menu_item(
                        "edit-per-server-profile",
                        "plugin-stoat-menu-edit-per-server-profile-label",
                        MenuSlot::AfterFavorites,
                    ),
                    guest_menu_item(
                        "manage-bots",
                        "plugin-stoat-menu-manage-bots-label",
                        MenuSlot::AfterFavorites,
                    ),
                    mute_item,
                    guest_menu_item_destructive(
                        "leave-server",
                        "plugin-stoat-menu-leave-server-label",
                        MenuSlot::BeforeLeave,
                    ),
                ])
            }
            MenuTargetKind::User => {
                let is_blocked =
                    MENU_STATE.with(|s| s.borrow().blocked_users.contains(&target_id));
                let is_friend = MENU_STATE.with(|s| s.borrow().friends.contains(&target_id));
                let block_item = if is_blocked {
                    guest_menu_item(
                        "unblock-user",
                        "plugin-stoat-menu-unblock-user-label",
                        MenuSlot::BeforeLeave,
                    )
                } else {
                    guest_menu_item_destructive(
                        "block-user",
                        "plugin-stoat-menu-block-user-label",
                        MenuSlot::BeforeLeave,
                    )
                };
                let friend_item = if is_friend {
                    guest_menu_item(
                        "remove-friend",
                        "plugin-stoat-menu-remove-friend-label",
                        MenuSlot::AfterFavorites,
                    )
                } else {
                    guest_menu_item(
                        "add-friend",
                        "plugin-stoat-menu-add-friend-label",
                        MenuSlot::AfterFavorites,
                    )
                };
                Ok(vec![
                    guest_menu_item(
                        "open-dm",
                        "plugin-stoat-menu-open-dm-label",
                        MenuSlot::AfterFavorites,
                    ),
                    friend_item,
                    block_item,
                ])
            }
            MenuTargetKind::Message => Ok(vec![
                guest_menu_item(
                    "react-message",
                    "plugin-stoat-menu-react-message-label",
                    MenuSlot::Top,
                ),
                guest_menu_item(
                    "copy-message-link",
                    "plugin-stoat-menu-copy-message-link-label",
                    MenuSlot::AfterFavorites,
                ),
                guest_menu_item_destructive(
                    "delete-message",
                    "plugin-stoat-menu-delete-message-label",
                    MenuSlot::BeforeLeave,
                ),
            ]),
            MenuTargetKind::Dm => {
                let is_muted = MENU_STATE.with(|s| s.borrow().muted_dms.contains(&target_id));
                let mute_item = if is_muted {
                    guest_menu_item(
                        "unmute-dm",
                        "plugin-stoat-menu-unmute-dm-label",
                        MenuSlot::AfterFavorites,
                    )
                } else {
                    guest_menu_item(
                        "mute-dm",
                        "plugin-stoat-menu-mute-dm-label",
                        MenuSlot::AfterFavorites,
                    )
                };
                Ok(vec![
                    guest_menu_item_destructive(
                        "close-dm",
                        "plugin-stoat-menu-close-dm-label",
                        MenuSlot::BeforeLeave,
                    ),
                    mute_item,
                ])
            }
            MenuTargetKind::Category => Ok(vec![]),
        }
    }

    fn invoke_context_action(
        action_id: String,
        _target: MenuTargetKind,
        target_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        match action_id.as_str() {
            "mute-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_channels.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unmute-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_channels.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "mark-channel-read" => Ok(ActionOutcome::Completed),
            "mute-server" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_servers.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unmute-server" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_servers.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "invite-people"
            | "privacy-settings"
            | "edit-per-server-profile"
            | "manage-bots" => Ok(ActionOutcome::Noop),
            "leave-server" => Ok(ActionOutcome::Completed),
            "block-user" => {
                MENU_STATE.with(|s| s.borrow_mut().blocked_users.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unblock-user" => {
                MENU_STATE.with(|s| s.borrow_mut().blocked_users.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "add-friend" => {
                MENU_STATE.with(|s| s.borrow_mut().friends.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "remove-friend" => {
                MENU_STATE.with(|s| s.borrow_mut().friends.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "open-dm" => Ok(ActionOutcome::Noop),
            "react-message" => Ok(ActionOutcome::Noop),
            "copy-message-link" => Ok(ActionOutcome::Noop),
            "delete-message" => Ok(ActionOutcome::Completed),
            "close-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().closed_dms.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "mute-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_dms.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unmute-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_dms.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            other => Err(wit::ClientError::NotFound(format!(
                "unknown stoat action: {other}"
            ))),
        }
    }

    fn poll_action(_handle: PendingHandle) -> Result<ActionOutcome, wit::ClientError> {
        Ok(ActionOutcome::Completed)
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

impl ClientSettingsGuest for StoatPlugin {
    fn get_settings_sections(
    ) -> Result<Vec<crate::wit_bindings::SettingsSection>, wit::ClientError> {
        Ok(vec![])
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

impl ClientSidebarGuest for StoatPlugin {
    fn get_sidebar_declaration() -> Result<SidebarDeclaration, wit::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::ChannelList,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }
}

impl ClientViewsGuest for StoatPlugin {
    fn get_channel_view(
        _channel_id: String,
    ) -> Result<crate::wit_bindings::exports::poly::messenger::client_views::ViewDescriptor, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "stoat does not support non-chat views".to_string(),
        ))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<crate::wit_bindings::exports::poly::messenger::client_views::ViewRowsPage, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "stoat does not support view rows".to_string(),
        ))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<crate::wit_bindings::exports::poly::messenger::client_views::ViewDetail, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "stoat does not support view detail".to_string(),
        ))
    }
}

impl ClientComposerGuest for StoatPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<Vec<crate::wit_bindings::exports::poly::messenger::client_composer::ComposerButton>, wit::ClientError> {
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

// ─── Bonfire WebSocket event parser ─────────────────────────────────
//
// SOLID-audit-stoat D.1 — WIT-guest counterpart to the native
// `is_backend.rs::parse_bonfire_event`. The native parser maps Bonfire JSON
// into `poly_client::ClientEvent`; this one maps the same shapes into
// `wit::ClientEvent` so the WASM plugin can call `host_api::emit_event` and
// reach the host's event_stream consumers. Variant coverage is intentionally
// identical to the native parser — keep them in sync when adding new Bonfire
// frame types (e.g. `MessageUpdate`, `MessageDelete` once Stoat ships them
// reliably across instances).

fn bonfire_event_to_wit(json: &serde_json::Value) -> Option<wit::ClientEvent> {
    let ty = json.get("type")?.as_str()?;
    match ty {
        "Message" => {
            let channel_id = json.get("channel")?.as_str()?.to_string();
            let msg_json = json.get("message")?;
            let id = msg_json.get("_id")?.as_str()?.to_string();
            let content = msg_json.get("content")?.as_str()?.to_string();
            let author_id = msg_json.get("author")?.as_str()?.to_string();
            // Bonfire `Message` frames don't carry the author's display name or
            // avatar inline; the UI resolves these from the channel member
            // cache. Match the native parser's choice to ship empty strings
            // here rather than block on an extra `/users/{id}` round-trip.
            let author = wit::User {
                id: author_id,
                display_name: String::new(),
                avatar_url: None,
                presence: wit::PresenceStatus::Online,
                backend: "stoat".to_string(),
            };
            let message = wit::Message {
                id,
                author,
                content: wit::MessageContent::Text(content),
                // host-api uses RFC3339 strings; `get-current-time` is the
                // host's monotonic clock — wall-clock now is a best-effort
                // stand-in matching native parser semantics.
                timestamp: host_api::get_current_time(),
                attachments: vec![],
                reactions: vec![],
                reply_to: None,
                edited: false,
                thread: None,
            };
            Some(wit::ClientEvent::MessageReceived(
                wit::MessageReceivedEvent { channel_id, message },
            ))
        }
        "ChannelStartTyping" => {
            let channel_id = json.get("id")?.as_str()?.to_string();
            let user_id = json.get("user")?.as_str()?.to_string();
            Some(wit::ClientEvent::TypingStarted(wit::TypingStartedEvent {
                channel_id,
                user_id,
                timestamp: host_api::get_current_time(),
            }))
        }
        // F.6 — Bonfire voice events (emitted by test-stoat mock; piggy-back on
        // the Bonfire WS instead of needing a separate Vortex subscription).
        "VoiceUserJoined" => {
            let channel_id = json.get("channel_id")?.as_str()?.to_string();
            let user_id = json.get("user_id")?.as_str()?.to_string();
            let display_name = json
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(&user_id)
                .to_string();
            let avatar_url = json
                .get("avatar_url")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let participant = wit::VoiceParticipant {
                user: wit::User {
                    id: user_id,
                    display_name,
                    avatar_url,
                    presence: wit::PresenceStatus::Online,
                    backend: "stoat".to_string(),
                },
                is_muted: json
                    .get("is_muted")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                is_deafened: false,
                is_streaming: false,
                is_video_on: false,
                is_speaking: false,
            };
            Some(wit::ClientEvent::VoiceUserJoined(wit::VoiceChannelEvent {
                channel_id,
                participant,
            }))
        }
        "VoiceUserLeft" => {
            let channel_id = json.get("channel_id")?.as_str()?.to_string();
            let user_id = json.get("user_id")?.as_str()?.to_string();
            Some(wit::ClientEvent::VoiceUserLeft(wit::VoiceUserLeftEvent {
                channel_id,
                user_id,
            }))
        }
        // Bonfire's first message after token-validation success.
        "Authenticated" => Some(wit::ClientEvent::ConnectionStateChanged(
            wit::ConnectionStateChangedEvent {
                backend: "stoat".to_string(),
                connected: true,
            },
        )),
        _ => None,
    }
}

// Register the component export.
export!(StoatPlugin with_types_in crate::wit_bindings);
