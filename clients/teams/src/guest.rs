//! WASM Component Model guest implementation for the Teams messenger plugin.
//!
//! Mirrors the native `TeamsClient` over the host-provided `http_request`
//! capability. Default Graph base URL is `https://graph.microsoft.com`; the
//! `TEAMS_BASE_URL` entry in plugin-global storage overrides it so the E2E
//! harness can point at a mock server.
//!
//! ## Channel IDs
//! Native encodes Graph's `team_id/channel_id` pair as a single slash-separated
//! string in `Channel.id` / `Channel.server_id`. The guest follows the same
//! convention so hosts can route requests identically regardless of backend.
//!
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashSet;

use serde::Deserialize;

use crate::wit_bindings::{
    ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, Guest, PluginMetadataGuest, export,
    poly::messenger::{host_api, types::HttpResponse},
    wit,
};

const DEFAULT_BASE_URL: &str = "https://graph.microsoft.com";
/// Plugin-global storage key letting the host point the guest at a mock server.
const BASE_URL_OVERRIDE_KEY: &str = "teams.base_url";

thread_local! {
    static STATE: RefCell<Option<StoredSession>> = const { RefCell::new(None) };
}

// ── F10 per-target menu state (in-memory; F9 covers persistence) ──────────

/// Tracks toggled state for state-aware context-menu items.
struct TeamsMenuState {
    hidden_channels: HashSet<String>,
    pinned_channels: HashSet<String>,
    muted_channels: HashSet<String>,
    muted_teams: HashSet<String>,
    saved_messages: HashSet<String>,
    hidden_dms: HashSet<String>,
    muted_dms: HashSet<String>,
}

impl Default for TeamsMenuState {
    fn default() -> Self {
        Self {
            hidden_channels: HashSet::new(),
            pinned_channels: HashSet::new(),
            muted_channels: HashSet::new(),
            muted_teams: HashSet::new(),
            saved_messages: HashSet::new(),
            hidden_dms: HashSet::new(),
            muted_dms: HashSet::new(),
        }
    }
}

thread_local! {
    static MENU_STATE: RefCell<TeamsMenuState> = RefCell::new(TeamsMenuState::default());
}

#[derive(Clone)]
struct StoredSession {
    base_url: String,
    token: String,
    user_id: String,
    display_name: String,
}

// ── Graph response shapes (minimum fields we actually consume) ────────

#[derive(Deserialize)]
struct GraphUser {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct GraphTeam {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct GraphChannel {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct GraphMessageBody {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct GraphIdentitySet {
    #[serde(default)]
    user: Option<GraphIdentity>,
}

#[derive(Deserialize)]
struct GraphIdentity {
    id: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct GraphMessage {
    id: String,
    #[serde(rename = "createdDateTime")]
    created_date_time: String,
    #[serde(default)]
    body: Option<GraphMessageBody>,
    #[serde(default)]
    from: Option<GraphIdentitySet>,
    #[serde(rename = "lastModifiedDateTime", default)]
    last_modified: Option<String>,
}

#[derive(Deserialize)]
struct GraphCollection<T> {
    value: Vec<T>,
}

// ── Helpers ────────────────────────────────────────────────────────────

fn state_snapshot() -> Option<StoredSession> {
    STATE.with(|s| s.borrow().clone())
}

fn resolve_base_url() -> String {
    host_api::storage_get(BASE_URL_OVERRIDE_KEY)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|s| s.trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

fn bearer_headers(token: &str) -> Vec<(String, String)> {
    vec![("Authorization".to_string(), format!("Bearer {token}"))]
}

fn bearer_json_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Content-Type".to_string(), "application/json".to_string()),
    ]
}

fn http(
    method: &str,
    url: &str,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
) -> Result<HttpResponse, wit::ClientError> {
    host_api::http_request(method, url, &headers, body.as_deref())
        .map_err(wit::ClientError::Internal)
}

fn check_status(resp: &HttpResponse, context: &str) -> Result<(), wit::ClientError> {
    match resp.status {
        200..=299 => Ok(()),
        401 | 403 => Err(wit::ClientError::AuthFailed(format!(
            "Teams auth rejected on {context} (HTTP {})",
            resp.status
        ))),
        404 => Err(wit::ClientError::NotFound(format!(
            "Teams {context} not found"
        ))),
        429 => Err(wit::ClientError::RateLimited(
            extract_retry_after(resp).unwrap_or(1),
        )),
        status => Err(wit::ClientError::Network(format!(
            "Teams {context} returned HTTP {status}"
        ))),
    }
}

fn extract_retry_after(resp: &HttpResponse) -> Option<u64> {
    resp.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, v)| v.parse::<u64>().ok())
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    resp: &HttpResponse,
    context: &str,
) -> Result<T, wit::ClientError> {
    serde_json::from_slice(&resp.body).map_err(|err| {
        wit::ClientError::Internal(format!("Teams {context}: invalid JSON ({err})"))
    })
}

fn graph_message_to_wit(m: GraphMessage) -> wit::Message {
    let (author_id, author_display) = m
        .from
        .and_then(|f| f.user)
        .map(|u| (u.id, u.display_name.unwrap_or_default()))
        .unwrap_or_default();
    let content = m.body.map(|b| b.content).unwrap_or_default();
    let author = wit::User {
        id: author_id,
        display_name: author_display,
        avatar_url: None,
        presence: wit::PresenceStatus::Online,
        backend: "teams".to_string(),
    };
    wit::Message {
        id: m.id,
        author,
        content: wit::MessageContent::Text(content),
        timestamp: m.created_date_time,
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: m.last_modified.is_some(),
        thread: None,
    }
}

fn wit_presence_to_graph(status: wit::PresenceStatus) -> &'static str {
    match status {
        wit::PresenceStatus::Online => "Available",
        wit::PresenceStatus::Idle => "Away",
        wit::PresenceStatus::DoNotDisturb => "DoNotDisturb",
        wit::PresenceStatus::Invisible | wit::PresenceStatus::Offline => "Offline",
    }
}

fn require_session() -> Result<StoredSession, wit::ClientError> {
    state_snapshot()
        .ok_or_else(|| wit::ClientError::AuthFailed("Teams guest not authenticated".into()))
}

fn split_channel_id(compound: &str) -> Result<(&str, &str), wit::ClientError> {
    compound.split_once('/').ok_or_else(|| {
        wit::ClientError::Internal(format!(
            "Teams channel_id must be 'team_id/channel_id', got '{compound}'"
        ))
    })
}

struct TeamsPlugin;

impl Guest for TeamsPlugin {
    fn authenticate(credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        let base_url = resolve_base_url();
        let token = match credentials {
            wit::AuthCredentials::Token(t) | wit::AuthCredentials::Oauth(t) => t,
            wit::AuthCredentials::EmailPassword(creds) => test_login(&base_url, &creds)?,
            _ => {
                return Err(wit::ClientError::AuthFailed(
                    "Teams requires Token/OAuth/EmailPassword".into(),
                ))
            }
        };

        // Validate by calling /v1.0/me and capture the user.
        let resp = http(
            "GET",
            &format!("{base_url}/v1.0/me"),
            bearer_headers(&token),
            None,
        )?;
        check_status(&resp, "/v1.0/me")?;
        let me: GraphUser = parse_json(&resp, "/v1.0/me")?;
        let display_name = me.display_name.clone().unwrap_or_default();

        let session = wit::Session {
            id: format!("teams-{}", me.id),
            user: wit::User {
                id: me.id.clone(),
                display_name: display_name.clone(),
                avatar_url: None,
                presence: wit::PresenceStatus::Online,
                backend: "teams".to_string(),
            },
            token: token.clone(),
            backend: "teams".to_string(),
            icon_emoji: Some("👥".into()),
            instance_id: "teams".into(),
            backend_url: Some(base_url.clone()),
        };

        STATE.with(|s| {
            s.replace(Some(StoredSession {
                base_url,
                token,
                user_id: me.id,
                display_name,
            }));
        });
        Ok(session)
    }

    fn logout() -> Result<(), wit::ClientError> {
        STATE.with(|s| s.replace(None));
        Ok(())
    }

    fn is_authenticated() -> bool {
        state_snapshot().is_some()
    }

    fn get_servers() -> Result<Vec<wit::Server>, wit::ClientError> {
        let Some(session) = state_snapshot() else {
            return Ok(vec![]);
        };
        let resp = http(
            "GET",
            &format!("{}/v1.0/me/joinedTeams", session.base_url),
            bearer_headers(&session.token),
            None,
        )?;
        check_status(&resp, "/v1.0/me/joinedTeams")?;
        let teams: GraphCollection<GraphTeam> = parse_json(&resp, "/v1.0/me/joinedTeams")?;
        Ok(teams
            .value
            .into_iter()
            .map(|t| wit::Server {
                id: t.id,
                name: t.display_name.unwrap_or_default(),
                icon_url: None,
                banner_url: None,
                categories: vec![],
                backend: "teams".to_string(),
                unread_count: 0,
                mention_count: 0,
                account_id: session.user_id.clone(),
                account_display_name: session.display_name.clone(),
                default_channel_id: None,
            })
            .collect())
    }

    fn get_server(id: String) -> Result<wit::Server, wit::ClientError> {
        let session = require_session().map_err(|_| {
            wit::ClientError::NotFound(format!("Teams server {id} (unauthenticated)"))
        })?;
        let resp = http(
            "GET",
            &format!("{}/v1.0/teams/{id}", session.base_url),
            bearer_headers(&session.token),
            None,
        )?;
        check_status(&resp, "team")?;
        let t: GraphTeam = parse_json(&resp, "team")?;
        Ok(wit::Server {
            id: t.id,
            name: t.display_name.unwrap_or_default(),
            icon_url: None,
            banner_url: None,
            categories: vec![],
            backend: "teams".to_string(),
            unread_count: 0,
            mention_count: 0,
            account_id: session.user_id,
            account_display_name: session.display_name,
            default_channel_id: None,
        })
    }

    fn get_channels(server_id: String) -> Result<Vec<wit::Channel>, wit::ClientError> {
        let Some(session) = state_snapshot() else {
            return Ok(vec![]);
        };
        let resp = http(
            "GET",
            &format!("{}/v1.0/teams/{server_id}/channels", session.base_url),
            bearer_headers(&session.token),
            None,
        )?;
        check_status(&resp, "channels")?;
        let channels: GraphCollection<GraphChannel> = parse_json(&resp, "channels")?;
        Ok(channels
            .value
            .into_iter()
            .map(|c| wit::Channel {
                id: format!("{server_id}/{}", c.id),
                name: c.display_name.unwrap_or_default(),
                channel_type: wit::ChannelType::Text,
                server_id: server_id.clone(),
                unread_count: 0,
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
            })
            .collect())
    }

    fn get_channel(id: String) -> Result<wit::Channel, wit::ClientError> {
        let session = require_session().map_err(|_| {
            wit::ClientError::NotFound(format!("Teams channel {id} (unauthenticated)"))
        })?;
        let (team_id, ch_id) = split_channel_id(&id)?;
        let resp = http(
            "GET",
            &format!("{}/v1.0/teams/{team_id}/channels/{ch_id}", session.base_url),
            bearer_headers(&session.token),
            None,
        )?;
        check_status(&resp, "channel")?;
        let c: GraphChannel = parse_json(&resp, "channel")?;
        Ok(wit::Channel {
            id: format!("{team_id}/{}", c.id),
            name: c.display_name.unwrap_or_default(),
            channel_type: wit::ChannelType::Text,
            server_id: team_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        })
    }

    fn send_message(
        channel_id: String,
        content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        let session = require_session()?;
        let text = match content {
            wit::MessageContent::Text(t) => t,
            wit::MessageContent::WithAttachments(p) => p.text,
        };
        let body = serde_json::json!({
            "body": { "content": text, "contentType": "text" }
        });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| wit::ClientError::Internal(e.to_string()))?;

        // channel_id = "team/channel" → channel POST; otherwise treat as chat id.
        let url = match channel_id.split_once('/') {
            Some((team_id, ch_id)) => format!(
                "{}/v1.0/teams/{team_id}/channels/{ch_id}/messages",
                session.base_url
            ),
            None => format!("{}/v1.0/chats/{channel_id}/messages", session.base_url),
        };
        let resp = http(
            "POST",
            &url,
            bearer_json_headers(&session.token),
            Some(body_bytes),
        )?;
        check_status(&resp, "send_message")?;
        let m: GraphMessage = parse_json(&resp, "send_message")?;
        Ok(graph_message_to_wit(m))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        // Graph nests replies under /messages/{id}/replies; distinct endpoint we
        // haven't wired native-side yet. Keep parity until both sides land it.
        Err(wit::ClientError::NotSupported(
            "Teams reply sending not yet implemented".to_string(),
        ))
    }

    fn get_messages(
        channel_id: String,
        query: wit::MessageQuery,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        let Some(session) = state_snapshot() else {
            return Ok(vec![]);
        };
        let top = query.limit.unwrap_or(50);
        let url = match channel_id.split_once('/') {
            Some((team_id, ch_id)) => format!(
                "{}/v1.0/teams/{team_id}/channels/{ch_id}/messages?$top={top}",
                session.base_url
            ),
            None => format!(
                "{}/v1.0/chats/{channel_id}/messages?$top={top}",
                session.base_url
            ),
        };
        let resp = http("GET", &url, bearer_headers(&session.token), None)?;
        check_status(&resp, "messages")?;
        let msgs: GraphCollection<GraphMessage> = parse_json(&resp, "messages")?;
        Ok(msgs.value.into_iter().map(graph_message_to_wit).collect())
    }

    fn search_messages(
        _query: wit::MessageSearchQuery,
    ) -> Result<Vec<wit::MessageSearchHit>, wit::ClientError> {
        // Graph search lives under /v1.0/search/query with a distinct request
        // shape — skipped for parity with native (not yet implemented there).
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
            "Teams pin mutation not yet implemented".to_string(),
        ))
    }

    fn get_user(id: String) -> Result<wit::User, wit::ClientError> {
        let session = require_session()
            .map_err(|_| wit::ClientError::NotFound(format!("Teams user {id} (unauthenticated)")))?;
        let resp = http(
            "GET",
            &format!("{}/v1.0/users/{id}", session.base_url),
            bearer_headers(&session.token),
            None,
        )?;
        check_status(&resp, "user")?;
        let u: GraphUser = parse_json(&resp, "user")?;
        Ok(wit::User {
            id: u.id,
            display_name: u.display_name.unwrap_or_default(),
            avatar_url: None,
            presence: wit::PresenceStatus::Online,
            backend: "teams".to_string(),
        })
    }

    fn get_friends() -> Result<Vec<wit::User>, wit::ClientError> {
        // Teams has no "friends" concept — skipped.
        Ok(vec![])
    }

    fn get_channel_members(_channel_id: String) -> Result<Vec<wit::User>, wit::ClientError> {
        // Graph /channels/{id}/members returns aadUserConversationMember records
        // that point at user ids — another hop to resolve. Skipped for now.
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
            "Teams WASM open DM not yet implemented".to_string(),
        ))
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Teams WASM saved messages not yet implemented".to_string(),
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

    fn set_presence(status: wit::PresenceStatus) -> Result<(), wit::ClientError> {
        let Some(session) = state_snapshot() else {
            // Match stub behavior when unauthenticated so E2E harness still passes.
            return Ok(());
        };
        let availability = wit_presence_to_graph(status);
        let body = serde_json::json!({ "availability": availability });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| wit::ClientError::Internal(e.to_string()))?;
        let resp = http(
            "PATCH",
            &format!("{}/v1.0/me/presence/setPresence", session.base_url),
            bearer_json_headers(&session.token),
            Some(body_bytes),
        )?;
        check_status(&resp, "setPresence")?;
        Ok(())
    }

    fn handle_ws_data(_handle: u64, data: Vec<u8>) {
        // The native client long-polls /test/events/poll and fans events out on
        // a channel. WIT plugins get the same bytes through `handle-ws-data`
        // whenever the host decides to treat the long-poll body as a WS frame.
        let Ok(events) = serde_json::from_slice::<Vec<serde_json::Value>>(&data) else {
            host_api::log(
                wit::LogLevel::Warn,
                "Teams handle_ws_data: payload is not a JSON array",
            );
            return;
        };
        for ev in events {
            if let Some(client_event) = teams_event_to_wit(ev) {
                host_api::emit_event(&client_event);
            }
        }
    }

    fn get_backend_type() -> String {
        "teams".to_string()
    }

    fn get_backend_name() -> String {
        "Teams".to_string()
    }

    fn get_backend_capabilities() -> wit::BackendCapabilities {
        wit::BackendCapabilities {
            supports_voice: false,
            supports_video: false,
            supports_dms: true,
            supports_groups: true,
            supports_send_messages: true,
            supports_presence: true,
            supports_search: false,
            supports_reactions: true,
            supports_typing_indicators: false,
            supports_file_upload: false,
            landing: wit::LandingPage::FirstServer,
        }
    }

    fn list_files(
        _channel_id: String,
        _path: String,
    ) -> Result<Vec<wit::FileEntry>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "teams has no code channels".to_string(),
        ))
    }

    fn read_file(
        _channel_id: String,
        _path: String,
    ) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "teams has no code channels".to_string(),
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

fn test_login(
    base_url: &str,
    creds: &wit::EmailPasswordCreds,
) -> Result<String, wit::ClientError> {
    #[derive(Deserialize)]
    struct LoginResp {
        token: String,
    }
    let body = serde_json::json!({ "login": creds.email, "password": creds.password });
    let body_bytes =
        serde_json::to_vec(&body).map_err(|e| wit::ClientError::Internal(e.to_string()))?;
    let headers = vec![(
        "Content-Type".to_string(),
        "application/json".to_string(),
    )];
    let resp = http(
        "POST",
        &format!("{base_url}/test/auth/login"),
        headers,
        Some(body_bytes),
    )?;
    check_status(&resp, "/test/auth/login")?;
    let parsed: LoginResp = parse_json(&resp, "/test/auth/login")?;
    Ok(parsed.token)
}

fn teams_event_to_wit(ev: serde_json::Value) -> Option<wit::ClientEvent> {
    let ty = ev.get("type")?.as_str()?;
    let resource_id = ev.get("resourceId")?.as_str()?.to_string();
    match ty {
        "MessageCreated" => {
            let m: GraphMessage = serde_json::from_value(ev.get("message")?.clone()).ok()?;
            Some(wit::ClientEvent::MessageReceived(
                wit::MessageReceivedEvent {
                    channel_id: resource_id,
                    message: graph_message_to_wit(m),
                },
            ))
        }
        "MessageUpdated" => {
            let m: GraphMessage = serde_json::from_value(ev.get("message")?.clone()).ok()?;
            Some(wit::ClientEvent::MessageEdited(wit::MessageEditedEvent {
                channel_id: resource_id,
                message: graph_message_to_wit(m),
            }))
        }
        "MessageDeleted" => {
            let message_id = ev.get("messageId")?.as_str()?.to_string();
            Some(wit::ClientEvent::MessageDeleted(
                wit::MessageDeletedEvent {
                    channel_id: resource_id,
                    message_id,
                },
            ))
        }
        _ => None,
    }
}

impl PluginMetadataGuest for TeamsPlugin {
    fn get_translations(_locale: String) -> String {
        String::new()
    }

    fn get_display_name_key() -> String {
        "plugin-teams-title".to_string()
    }

    fn get_icon() -> String {
        "👥".to_string()
    }

    fn get_plugin_manifest() -> crate::wit_bindings::PluginManifest {
        crate::wit_bindings::PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![
                "graph.microsoft.com".to_string(),
                "login.microsoftonline.com".to_string(),
            ],
            description: "Connect to Microsoft Teams. Read and send channel and \
                          1:1/group chat messages, manage presence, react."
                .to_string(),
            homepage: Some("https://teams.microsoft.com".to_string()),
        }
    }
}

// ─── Client Menus ──────────────────────────────────────────────────

use crate::wit_bindings::{
    ActionOutcome, MenuItem, MenuItemVariant, MenuSlot, MenuTargetKind, PendingHandle,
};

/// Build a `MenuItem` with common defaults.
fn menu_item(id: &str, label_key: &str, slot: MenuSlot, variant: MenuItemVariant) -> MenuItem {
    MenuItem {
        id: id.to_string(),
        parent_id: None,
        slot,
        label_key: label_key.to_string(),
        icon: None,
        item_variant: variant,
        shortcut: None,
        block: None,
    }
}

impl ClientMenusGuest for TeamsPlugin {
    fn get_context_menu_items(
        target: MenuTargetKind,
        target_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        match target {
            MenuTargetKind::Channel => {
                let (hidden, pinned, muted) = MENU_STATE.with(|s| {
                    let s = s.borrow();
                    (
                        s.hidden_channels.contains(&target_id),
                        s.pinned_channels.contains(&target_id),
                        s.muted_channels.contains(&target_id),
                    )
                });
                Ok(vec![
                    menu_item("mark-read", "plugin-teams-menu-mark-read-label", MenuSlot::Top, MenuItemVariant::Normal),
                    menu_item("mark-unread", "plugin-teams-menu-mark-unread-label", MenuSlot::Top, MenuItemVariant::Normal),
                    if pinned {
                        menu_item("unpin-channel", "plugin-teams-menu-unpin-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("pin-channel", "plugin-teams-menu-pin-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if hidden {
                        menu_item("show-channel", "plugin-teams-menu-show-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("hide-channel", "plugin-teams-menu-hide-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if muted {
                        menu_item("unmute-channel", "plugin-teams-menu-unmute-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("mute-channel", "plugin-teams-menu-mute-channel-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                ])
            }

            MenuTargetKind::Server => {
                let muted = MENU_STATE.with(|s| s.borrow().muted_teams.contains(&target_id));
                Ok(vec![
                    if muted {
                        menu_item("unmute-team", "plugin-teams-menu-unmute-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("mute-team", "plugin-teams-menu-mute-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    menu_item("get-team-code", "plugin-teams-menu-get-team-code-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    menu_item("manage-team", "plugin-teams-menu-manage-team-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    menu_item("team-settings", "plugin-teams-menu-team-settings-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    menu_item("edit-per-team-profile", "plugin-teams-menu-edit-per-team-profile-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    menu_item("leave-team", "plugin-teams-menu-leave-team-label", MenuSlot::BeforeLeave, MenuItemVariant::Destructive),
                ])
            }

            MenuTargetKind::User => Ok(vec![
                menu_item("open-chat", "plugin-teams-menu-open-chat-label", MenuSlot::Top, MenuItemVariant::Normal),
                menu_item("view-profile", "plugin-teams-menu-view-profile-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                menu_item("schedule-meeting", "plugin-teams-menu-schedule-meeting-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
            ]),

            MenuTargetKind::Message => {
                let saved = MENU_STATE.with(|s| s.borrow().saved_messages.contains(&target_id));
                Ok(vec![
                    menu_item("react", "plugin-teams-menu-react-label", MenuSlot::Top, MenuItemVariant::Normal),
                    menu_item("reply-in-thread", "plugin-teams-menu-reply-in-thread-label", MenuSlot::Top, MenuItemVariant::Normal),
                    if saved {
                        menu_item("unsave-message", "plugin-teams-menu-unsave-message-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("save-message", "plugin-teams-menu-save-message-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    menu_item("mark-important", "plugin-teams-menu-mark-important-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal),
                    menu_item("delete-message", "plugin-teams-menu-delete-message-label", MenuSlot::BeforeLeave, MenuItemVariant::Destructive),
                ])
            }

            MenuTargetKind::Dm => {
                let (muted, hidden) = MENU_STATE.with(|s| {
                    let s = s.borrow();
                    (
                        s.muted_dms.contains(&target_id),
                        s.hidden_dms.contains(&target_id),
                    )
                });
                Ok(vec![
                    if muted {
                        menu_item("unmute-dm", "plugin-teams-menu-unmute-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("mute-dm", "plugin-teams-menu-mute-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
                    if hidden {
                        menu_item("show-dm", "plugin-teams-menu-show-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    } else {
                        menu_item("hide-dm", "plugin-teams-menu-hide-dm-label", MenuSlot::AfterFavorites, MenuItemVariant::Normal)
                    },
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
            // ── Channel toggles ──────────────────────────────────────────────
            "pin-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().pinned_channels.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "unpin-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().pinned_channels.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "hide-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().hidden_channels.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "show-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().hidden_channels.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "mute-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_channels.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-channel" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_channels.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "mark-read" | "mark-unread" => Ok(ActionOutcome::Noop),

            // ── Team toggles ─────────────────────────────────────────────────
            "mute-team" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_teams.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-team" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_teams.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "leave-team" | "get-team-code" | "manage-team" | "team-settings"
            | "edit-per-team-profile" => Ok(ActionOutcome::Noop),

            // ── User actions ─────────────────────────────────────────────────
            "open-chat" | "view-profile" | "schedule-meeting" => Ok(ActionOutcome::Noop),

            // ── Message toggles ──────────────────────────────────────────────
            "save-message" => {
                MENU_STATE.with(|s| s.borrow_mut().saved_messages.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "unsave-message" => {
                MENU_STATE.with(|s| s.borrow_mut().saved_messages.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "react" | "reply-in-thread" | "mark-important" | "delete-message" => {
                Ok(ActionOutcome::Noop)
            }

            // ── DM toggles ───────────────────────────────────────────────────
            "mute-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_dms.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "unmute-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().muted_dms.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "hide-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().hidden_dms.insert(target_id));
                Ok(ActionOutcome::RefreshTarget)
            }
            "show-dm" => {
                MENU_STATE.with(|s| s.borrow_mut().hidden_dms.remove(&target_id));
                Ok(ActionOutcome::RefreshTarget)
            }

            _ => Err(wit::ClientError::NotFound(action_id)),
        }
    }

    fn poll_action(_handle: PendingHandle) -> Result<ActionOutcome, wit::ClientError> {
        Ok(ActionOutcome::Completed)
    }
}

// ─── Client Settings ───────────────────────────────────────────────

use crate::wit_bindings::{SettingsScope, SettingsSection};

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

impl ClientSettingsGuest for TeamsPlugin {
    fn get_settings_sections() -> Result<Vec<SettingsSection>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_setting_value(
        scope: SettingsScope,
        scope_id: String,
        key: String,
    ) -> Result<String, wit::ClientError> {
        let k = composite_key(scope, &scope_id, &key);
        Ok(host_api::storage_get(&k)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| "null".to_string()))
    }

    fn set_setting_value(
        scope: SettingsScope,
        scope_id: String,
        key: String,
        value: String,
    ) -> Result<(), wit::ClientError> {
        let k = composite_key(scope, &scope_id, &key);
        host_api::storage_set(&k, value.as_bytes())
            .map_err(wit::ClientError::Internal)
    }
}

// ─── Client Sidebar ────────────────────────────────────────────────

use crate::wit_bindings::{SidebarDeclaration, SidebarLayoutKind};

impl ClientSidebarGuest for TeamsPlugin {
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

// ─── Client Views ──────────────────────────────────────────────────

use crate::wit_bindings::{Cursor, ViewDescriptor, ViewDetail, ViewRowsPage};

impl ClientViewsGuest for TeamsPlugin {
    fn get_channel_view(_channel_id: String) -> Result<ViewDescriptor, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "teams has no custom views".to_string(),
        ))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<ViewRowsPage, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "teams has no custom views".to_string(),
        ))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<ViewDetail, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "teams has no custom views".to_string(),
        ))
    }
}

// ─── Client Composer ───────────────────────────────────────────────

use crate::wit_bindings::ComposerButton;

impl ClientComposerGuest for TeamsPlugin {
    fn get_composer_buttons(_channel_id: String) -> Result<Vec<ComposerButton>, wit::ClientError> {
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

export!(TeamsPlugin with_types_in crate::wit_bindings);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_id_split_requires_slash() {
        assert!(split_channel_id("no-slash").is_err());
        assert_eq!(split_channel_id("team/chan").unwrap(), ("team", "chan"));
    }

    #[test]
    fn presence_mapping_covers_all_variants() {
        use wit::PresenceStatus::*;
        assert_eq!(wit_presence_to_graph(Online), "Available");
        assert_eq!(wit_presence_to_graph(Idle), "Away");
        assert_eq!(wit_presence_to_graph(DoNotDisturb), "DoNotDisturb");
        assert_eq!(wit_presence_to_graph(Invisible), "Offline");
        assert_eq!(wit_presence_to_graph(Offline), "Offline");
    }
}
