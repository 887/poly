//! WASM Component Model guest implementation for the Matrix messenger plugin.
//!
//! Partial real implementation using host-mediated HTTP requests.
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashSet;

use crate::wit_bindings::{
    ActionOutcome, ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest,
    ClientSidebarGuest, ClientViewsGuest, Guest, MenuItem, MenuItemVariant, MenuSlot,
    MenuTargetKind, PluginMetadataGuest, SidebarIconSource, SidebarItem, SidebarRouteKind,
    SidebarSection, export, wit,
};
use serde::{Deserialize, Serialize};

const DEFAULT_HOMESERVER: &str = "https://matrix.org";

#[derive(Debug, Clone)]
struct StoredSession {
    access_token: String,
    device_id: String,
    user_id: String,
}

// ─── F4: Space tree helpers (WASM guest) ──────────────────────────────────

/// Minimal joined-rooms response shape for WASM deserialization.
#[derive(Debug, Deserialize)]
struct WasmJoinedRoomsResponse {
    #[serde(default)]
    joined_rooms: Vec<String>,
}

/// Minimal room-state event shape for WASM deserialization.
#[derive(Debug, Deserialize)]
struct WasmRoomStateEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    content: serde_json::Value,
}

/// Minimal space-hierarchy room shape for WASM deserialization.
#[derive(Debug, Deserialize)]
struct WasmSpaceHierarchyRoom {
    room_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    room_type: Option<String>,
}

/// Minimal space-hierarchy response shape for WASM deserialization.
#[derive(Debug, Default, Deserialize)]
struct WasmSpaceHierarchyResponse {
    #[serde(default)]
    rooms: Vec<WasmSpaceHierarchyRoom>,
}

/// F4 — one entry in the flattened space/room tree (WASM variant).
#[derive(Debug, Clone)]
struct WasmSpaceTreeEntry {
    id: String,
    name: String,
    is_space: bool,
    parent_id: Option<String>,
}

/// F4 — convert flat `Vec<WasmSpaceTreeEntry>` into `Vec<SidebarItem>` (WASM).
///
/// FTL label note: same as the native version — `label_key` is the room's
/// display name.  The host's `t()` fallback renders it as-is when no FTL
/// match exists.
fn build_sidebar_items_wasm(entries: Vec<WasmSpaceTreeEntry>) -> Vec<SidebarItem> {
    let known_ids: std::collections::HashSet<String> =
        entries.iter().map(|e| e.id.clone()).collect();

    entries
        .into_iter()
        .map(|entry| {
            let parent_id = entry.parent_id.filter(|pid| known_ids.contains(pid.as_str()));
            let icon = if entry.is_space {
                Some(SidebarIconSource::Emoji("\u{1f30c}".to_string()))
            } else {
                Some(SidebarIconSource::Emoji("\u{1f4ac}".to_string()))
            };
            let route_kind = if entry.is_space {
                SidebarRouteKind::CustomView
            } else {
                SidebarRouteKind::Channel
            };
            SidebarItem {
                id: entry.id,
                parent_id,
                label_key: entry.name,
                icon,
                badge: None,
                route_kind,
            }
        })
        .collect()
}

/// F4 — fetch the space tree via host HTTP requests (WASM guest).
///
/// Returns a flat list of `WasmSpaceTreeEntry` items; errors on any step
/// produce an empty result for that step rather than propagating (best-effort
/// tree construction).
fn fetch_space_tree_wasm(
    homeserver: &str,
    token: &str,
) -> Result<Vec<WasmSpaceTreeEntry>, wit::ClientError> {
    use std::collections::HashSet;

    let headers = vec![("authorization".to_string(), format!("Bearer {token}"))];

    // 1. Fetch joined rooms.
    let resp = host_http_request(
        "GET",
        &format!("{homeserver}/_matrix/client/v3/joined_rooms"),
        headers.clone(),
        None,
    )?;
    if !matches!(resp.status, 200..=299) {
        return Err(wit::ClientError::Network(format!(
            "joined_rooms returned HTTP {}",
            resp.status
        )));
    }
    let joined: WasmJoinedRoomsResponse = parse_json(&resp)?;

    // 2. For each joined room, fetch its state to determine type and name.
    let mut room_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut room_is_space: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    let mut space_ids: Vec<String> = Vec::new();

    for room_id in &joined.joined_rooms {
        let state_resp = host_http_request(
            "GET",
            &format!("{homeserver}/_matrix/client/v3/rooms/{room_id}/state"),
            headers.clone(),
            None,
        );
        let state_events: Vec<WasmRoomStateEvent> = match state_resp {
            Ok(r) if matches!(r.status, 200..=299) => {
                parse_json(&r).unwrap_or_default()
            }
            _ => vec![],
        };

        let is_space = state_events.iter().any(|ev| {
            ev.event_type == "m.room.create"
                && ev.content.get("type").and_then(|v| v.as_str()) == Some("m.space")
        });
        let name = state_events
            .iter()
            .find(|ev| ev.event_type == "m.room.name")
            .and_then(|ev| ev.content.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(room_id)
            .to_string();

        room_names.insert(room_id.clone(), name);
        room_is_space.insert(room_id.clone(), is_space);
        if is_space {
            space_ids.push(room_id.clone());
        }
    }

    // 3. For each space, fetch hierarchy and collect child entries.
    let mut entries: Vec<WasmSpaceTreeEntry> = Vec::new();
    let mut child_ids: HashSet<String> = HashSet::new();
    let mut visited_spaces: HashSet<String> = HashSet::new();

    for space_id in &space_ids {
        if !visited_spaces.insert(space_id.clone()) {
            continue;
        }
        let hier_resp = host_http_request(
            "GET",
            &format!("{homeserver}/_matrix/client/v1/rooms/{space_id}/hierarchy"),
            headers.clone(),
            None,
        );
        let hierarchy: WasmSpaceHierarchyResponse = match hier_resp {
            Ok(r) if matches!(r.status, 200..=299) => parse_json(&r).unwrap_or_default(),
            _ => WasmSpaceHierarchyResponse { rooms: vec![] },
        };

        for room in &hierarchy.rooms {
            if room.room_id == *space_id {
                continue;
            }
            let is_space = room.room_type.as_deref() == Some("m.space");
            let name = room.name.clone().unwrap_or_else(|| room.room_id.clone());
            child_ids.insert(room.room_id.clone());
            entries.push(WasmSpaceTreeEntry {
                id: room.room_id.clone(),
                name,
                is_space,
                parent_id: Some(space_id.clone()),
            });
            if is_space {
                visited_spaces.insert(room.room_id.clone());
            }
        }
    }

    // 4. Top-level spaces (parent_id == None).
    let mut result: Vec<WasmSpaceTreeEntry> = space_ids
        .iter()
        .map(|id| WasmSpaceTreeEntry {
            id: id.clone(),
            name: room_names.get(id).cloned().unwrap_or_else(|| id.clone()),
            is_space: true,
            parent_id: None,
        })
        .collect();

    // 5. Orphan rooms — neither a space nor already a child.
    let orphans: Vec<WasmSpaceTreeEntry> = joined
        .joined_rooms
        .iter()
        .filter(|id| {
            !room_is_space.get(*id).copied().unwrap_or(false) && !child_ids.contains(*id)
        })
        .map(|id| WasmSpaceTreeEntry {
            id: id.clone(),
            name: room_names.get(id).cloned().unwrap_or_else(|| id.clone()),
            is_space: false,
            parent_id: None,
        })
        .collect();

    result.extend(entries);
    result.extend(orphans);
    Ok(result)
}

/// F10 — per-WASM-instance state for context-menu state pairs.
struct MatrixMenuState {
    muted_rooms: HashSet<String>,
    ignored_users: HashSet<String>,
    marked_read: HashSet<String>,
}

impl MatrixMenuState {
    fn new() -> Self {
        Self {
            muted_rooms: HashSet::new(),
            ignored_users: HashSet::new(),
            marked_read: HashSet::new(),
        }
    }
}

thread_local! {
    static STATE: RefCell<Option<StoredSession>> = const { RefCell::new(None) };
    static MENU_STATE: RefCell<MatrixMenuState> = RefCell::new(MatrixMenuState::new());
}

/// Build a simple `MenuItem` without icon, shortcut, or block.
fn simple_item(
    id: &str,
    slot: MenuSlot,
    label_key: &str,
    item_variant: MenuItemVariant,
) -> MenuItem {
    MenuItem {
        id: id.to_string(),
        parent_id: None,
        slot,
        label_key: label_key.to_string(),
        icon: None,
        item_variant,
        shortcut: None,
        block: None,
    }
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
                        backend: "matrix".to_string(),
                    },
                    token,
                    backend: "matrix".to_string(),
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
                        backend: "matrix".to_string(),
                    },
                    token: login.access_token,
                    backend: "matrix".to_string(),
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
            backend: "matrix".to_string(),
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

    fn get_backend_type() -> String {
        "matrix".to_string()
    }

    fn get_backend_name() -> String {
        "Matrix".to_string()
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
            "matrix has no code channels".to_string(),
        ))
    }

    fn read_file(_channel_id: String, _path: String) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "matrix has no code channels".to_string(),
        ))
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

    fn get_display_name_key() -> String {
        "plugin-matrix-title".to_string()
    }

    fn get_icon() -> String {
        "🟦".to_string()
    }

    fn get_plugin_manifest() -> crate::wit_bindings::PluginManifest {
        crate::wit_bindings::PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["matrix.org".to_string()],
            description: "Connect to Matrix, an open decentralized instant messaging protocol. \
                          Supports text rooms, DMs, and rich message reactions."
                .to_string(),
            homepage: Some("https://matrix.org".to_string()),
        }
    }
}

impl ClientMenusGuest for MatrixPlugin {
    fn get_context_menu_items(
        target: MenuTargetKind,
        target_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        match target {
            // ── Server / Space ─────────────────────────────────────────────
            MenuTargetKind::Server => Ok(vec![
                simple_item("space-settings", MenuSlot::AfterFavorites, "plugin-matrix-menu-space-settings-label", MenuItemVariant::Normal),
                simple_item("edit-per-space-profile", MenuSlot::AfterFavorites, "plugin-matrix-menu-edit-per-space-profile-label", MenuItemVariant::Normal),
                simple_item("e2ee-verification", MenuSlot::AfterFavorites, "plugin-matrix-menu-e2ee-verification-label", MenuItemVariant::Normal),
                simple_item("browse-rooms-in-space", MenuSlot::AfterFavorites, "plugin-matrix-menu-browse-rooms-in-space-label", MenuItemVariant::Normal),
                simple_item("add-room-to-space", MenuSlot::AfterFavorites, "plugin-matrix-menu-add-room-to-space-label", MenuItemVariant::Normal),
                simple_item("leave-space", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-space-label", MenuItemVariant::Destructive),
            ]),

            // ── Channel / Room ─────────────────────────────────────────────
            MenuTargetKind::Channel => {
                let is_read = MENU_STATE.with(|s| s.borrow().marked_read.contains(&target_id));
                let read_item = if is_read {
                    simple_item("mark-unread-room", MenuSlot::Top, "plugin-matrix-menu-mark-unread-room-label", MenuItemVariant::Normal)
                } else {
                    simple_item("mark-read-room", MenuSlot::Top, "plugin-matrix-menu-mark-read-room-label", MenuItemVariant::Normal)
                };

                let is_muted = MENU_STATE.with(|s| s.borrow().muted_rooms.contains(&target_id));
                let mute_item = if is_muted {
                    simple_item("unmute-room", MenuSlot::AfterFavorites, "plugin-matrix-menu-unmute-room-label", MenuItemVariant::Normal)
                } else {
                    simple_item("mute-room", MenuSlot::AfterFavorites, "plugin-matrix-menu-mute-room-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    read_item,
                    mute_item,
                    simple_item("leave-room", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-room-label", MenuItemVariant::Destructive),
                ])
            }

            // ── DM Channel ─────────────────────────────────────────────────
            MenuTargetKind::Dm => {
                let is_read = MENU_STATE.with(|s| s.borrow().marked_read.contains(&target_id));
                let read_item = if is_read {
                    simple_item("mark-unread-room", MenuSlot::Top, "plugin-matrix-menu-mark-unread-room-label", MenuItemVariant::Normal)
                } else {
                    simple_item("mark-read-room", MenuSlot::Top, "plugin-matrix-menu-mark-read-room-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    read_item,
                    simple_item("leave-dm", MenuSlot::BeforeLeave, "plugin-matrix-menu-leave-dm-label", MenuItemVariant::Destructive),
                ])
            }

            // ── User ───────────────────────────────────────────────────────
            MenuTargetKind::User => {
                let is_ignored = MENU_STATE.with(|s| s.borrow().ignored_users.contains(&target_id));
                let ignore_item = if is_ignored {
                    simple_item("unignore-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-unignore-user-label", MenuItemVariant::Normal)
                } else {
                    simple_item("ignore-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-ignore-user-label", MenuItemVariant::Normal)
                };

                Ok(vec![
                    simple_item("open-dm", MenuSlot::Top, "plugin-matrix-menu-open-dm-label", MenuItemVariant::Normal),
                    simple_item("view-profile", MenuSlot::Top, "plugin-matrix-menu-view-profile-label", MenuItemVariant::Normal),
                    simple_item("verify-user", MenuSlot::AfterFavorites, "plugin-matrix-menu-verify-user-label", MenuItemVariant::Normal),
                    ignore_item,
                ])
            }

            // ── Message ────────────────────────────────────────────────────
            MenuTargetKind::Message => Ok(vec![
                simple_item("react-message", MenuSlot::Top, "plugin-matrix-menu-react-message-label", MenuItemVariant::Normal),
                simple_item("reply-in-thread", MenuSlot::Top, "plugin-matrix-menu-reply-in-thread-label", MenuItemVariant::Normal),
                simple_item("copy-permalink", MenuSlot::AfterFavorites, "plugin-matrix-menu-copy-permalink-label", MenuItemVariant::Normal),
                simple_item("redact-message", MenuSlot::BeforeLeave, "plugin-matrix-menu-redact-message-label", MenuItemVariant::Destructive),
            ]),

            MenuTargetKind::Category => Ok(Vec::new()),
        }
    }

    fn invoke_context_action(
        action_id: String,
        _target: MenuTargetKind,
        target_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        match action_id.as_str() {
            // ── Server / Space ──────────────────────────────────────────────
            "space-settings"
            | "edit-per-space-profile"
            | "e2ee-verification"
            | "browse-rooms-in-space"
            | "add-room-to-space"
            | "leave-space" => Ok(ActionOutcome::Noop),

            // ── Channel / Room — state mutations ────────────────────────────
            "mark-read-room" => {
                MENU_STATE.with(|s| s.borrow_mut().marked_read.insert(target_id));
                Ok(ActionOutcome::Noop)
            }
            "mark-unread-room" => {
                MENU_STATE.with(|s| s.borrow_mut().marked_read.remove(&target_id));
                Ok(ActionOutcome::Noop)
            }
            "mute-room" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_rooms.insert(target_id));
                Ok(ActionOutcome::Noop)
            }
            "unmute-room" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_rooms.remove(&target_id));
                Ok(ActionOutcome::Noop)
            }
            "leave-room" | "leave-dm" => Ok(ActionOutcome::Noop),

            // ── User — state mutations ───────────────────────────────────────
            "open-dm" | "view-profile" | "verify-user" => Ok(ActionOutcome::Noop),
            "ignore-user" => {
                MENU_STATE.with(|s| s.borrow_mut().ignored_users.insert(target_id));
                Ok(ActionOutcome::Noop)
            }
            "unignore-user" => {
                MENU_STATE.with(|s| s.borrow_mut().ignored_users.remove(&target_id));
                Ok(ActionOutcome::Noop)
            }

            // ── Message ───────────────────────────────────────────────────────
            "react-message"
            | "reply-in-thread"
            | "copy-permalink"
            | "redact-message" => Ok(ActionOutcome::Noop),

            _ => Err(wit::ClientError::NotFound(action_id)),
        }
    }

    fn poll_action(
        _handle: crate::wit_bindings::PendingHandle,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Ok(ActionOutcome::Completed)
    }
}

fn scope_label(scope: crate::wit_bindings::SettingsScope) -> &'static str {
    use crate::wit_bindings::SettingsScope;
    match scope {
        SettingsScope::AccountGlobal => "account-global",
        SettingsScope::PerServer => "per-server",
        SettingsScope::PerChannel => "per-channel",
        SettingsScope::PerUser => "per-user",
    }
}

fn composite_key(
    scope: crate::wit_bindings::SettingsScope,
    scope_id: &str,
    key: &str,
) -> String {
    format!("settings:{}:{}:{}", scope_label(scope), scope_id, key)
}

impl ClientSettingsGuest for MatrixPlugin {
    fn get_settings_sections(
    ) -> Result<Vec<crate::wit_bindings::SettingsSection>, wit::ClientError> {
        Ok(Vec::new())
    }

    fn get_setting_value(
        scope: crate::wit_bindings::SettingsScope,
        scope_id: String,
        key: String,
    ) -> Result<String, wit::ClientError> {
        let k = composite_key(scope, &scope_id, &key);
        Ok(
            crate::wit_bindings::poly::messenger::host_api::storage_get(&k)
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .unwrap_or_else(|| "null".to_string()),
        )
    }

    fn set_setting_value(
        scope: crate::wit_bindings::SettingsScope,
        scope_id: String,
        key: String,
        value: String,
    ) -> Result<(), wit::ClientError> {
        let k = composite_key(scope, &scope_id, &key);
        crate::wit_bindings::poly::messenger::host_api::storage_set(&k, value.as_bytes())
            .map_err(wit::ClientError::Internal)
    }
}

impl ClientSidebarGuest for MatrixPlugin {
    fn get_sidebar_declaration(
    ) -> Result<crate::wit_bindings::SidebarDeclaration, wit::ClientError> {
        // F4: Build the space tree using host-mediated HTTP requests.
        let session = STATE.with(|s| s.borrow().clone());
        let items = match session {
            None => vec![],
            Some(sess) => {
                let entries =
                    fetch_space_tree_wasm(DEFAULT_HOMESERVER, &sess.access_token)
                        .unwrap_or_default();
                build_sidebar_items_wasm(entries)
            }
        };
        Ok(crate::wit_bindings::SidebarDeclaration {
            layout: crate::wit_bindings::SidebarLayoutKind::Custom,
            sections: vec![SidebarSection {
                header_key: Some("plugin-matrix-sidebar-spaces-section".to_string()),
                collapsible: false,
                default_collapsed: false,
                items,
            }],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: String,
    ) -> Result<crate::wit_bindings::ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }
}

impl ClientViewsGuest for MatrixPlugin {
    fn get_channel_view(
        _channel_id: String,
    ) -> Result<crate::wit_bindings::ViewDescriptor, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "matrix has no non-chat views".to_string(),
        ))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<crate::wit_bindings::Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<crate::wit_bindings::ViewRowsPage, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "matrix has no non-chat views".to_string(),
        ))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<crate::wit_bindings::ViewDetail, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "matrix has no non-chat views".to_string(),
        ))
    }
}

impl ClientComposerGuest for MatrixPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<Vec<crate::wit_bindings::ComposerButton>, wit::ClientError> {
        Ok(Vec::new())
    }

    fn get_message_actions(
        _channel_id: String,
        _message_id: String,
    ) -> Result<Vec<crate::wit_bindings::MenuItem>, wit::ClientError> {
        Ok(Vec::new())
    }

    fn invoke_composer_action(
        action_id: String,
        _channel_id: String,
    ) -> Result<crate::wit_bindings::ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }

    fn invoke_message_action(
        action_id: String,
        _channel_id: String,
        _message_id: String,
    ) -> Result<crate::wit_bindings::ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }
}

export!(MatrixPlugin with_types_in crate::wit_bindings);
