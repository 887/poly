//! WASM Component Model guest implementation for the Discord messenger plugin.
//!
//! Partial real implementation — forum/thread methods route through the host
//! HTTP bridge; auth remains a stub until 3.3.5 (gateway WebSocket).
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashSet;

use serde::Deserialize;

use crate::wit_bindings::{
    ActionOutcome, ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, Guest, MenuItem, MenuItemVariant, MenuSlot, MenuTargetKind, PendingHandle,
    PluginManifest, PluginMetadataGuest, SettingsScope, SidebarDeclaration, SidebarLayoutKind,
    export, poly::messenger::host_api, wit,
};

// ─── Authenticated session state ──────────────────────────────────────────

/// Minimal session data needed to make Discord REST calls.
#[derive(Clone)]
struct DiscordSession {
    /// Bot-prefixed Authorization header value: `"Bot <token>"`.
    auth_header: String,
    /// Base URL of the Discord API (default: `https://discord.com`).
    base_url: String,
}

// ─── F10 — in-memory state for state-aware context-menu items ─────────────

/// Per-plugin-instance mutable state.  WASM components are single-threaded;
/// `thread_local! + RefCell` is the canonical pattern (see demo plugin).
struct DiscordGuestState {
    muted_channels: HashSet<String>,
    muted_servers: HashSet<String>,
    blocked_users: HashSet<String>,
    friend_ids: HashSet<String>,
    muted_dms: HashSet<String>,
    /// Authenticated session, set after `authenticate()` succeeds.
    session: Option<DiscordSession>,
}

impl Default for DiscordGuestState {
    fn default() -> Self {
        Self {
            muted_channels: HashSet::new(),
            muted_servers: HashSet::new(),
            blocked_users: HashSet::new(),
            friend_ids: HashSet::new(),
            muted_dms: HashSet::new(),
            session: None,
        }
    }
}

thread_local! {
    static STATE: RefCell<DiscordGuestState> = RefCell::new(DiscordGuestState::default());
}

// ─── HTTP helpers ──────────────────────────────────────────────────────────

/// Make a GET request through the host HTTP bridge.
fn host_get(
    url: &str,
    auth_header: &str,
) -> Result<Vec<u8>, wit::ClientError> {
    let headers = vec![
        ("Authorization".to_string(), auth_header.to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ];
    let resp = host_api::http_request("GET", url, &headers, None)
        .map_err(wit::ClientError::Internal)?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(wit::ClientError::Network(format!("HTTP {}", resp.status)));
    }
    Ok(resp.body)
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, wit::ClientError> {
    serde_json::from_slice(body)
        .map_err(|e| wit::ClientError::Internal(format!("JSON parse error: {e}")))
}

/// Retrieve the current session or return `AuthFailed`.
fn current_session() -> Result<DiscordSession, wit::ClientError> {
    STATE.with(|s| {
        s.borrow()
            .session
            .clone()
            .ok_or_else(|| wit::ClientError::AuthFailed("not authenticated".into()))
    })
}

// ─── Discord wire types (WASM-side, minimal) ──────────────────────────────

/// Minimal Discord channel object for WASM deserialization.
#[derive(Debug, Deserialize)]
struct WasmDiscordChannel {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub message_count: Option<u32>,
    #[serde(default)]
    pub member_count: Option<u32>,
    #[serde(default)]
    pub applied_tags: Option<Vec<String>>,
    #[serde(default)]
    pub thread_metadata: Option<WasmThreadMetadata>,
}

#[derive(Debug, Deserialize)]
struct WasmThreadMetadata {
    /// ISO 8601 timestamp of when the thread was created (used for sort-by-creation-date).
    #[serde(default)]
    pub create_timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WasmActiveThreadsResponse {
    pub threads: Vec<WasmDiscordChannel>,
    // `has_more` from Discord is intentionally not captured here —
    // pagination of active threads is a future enhancement.
}

#[derive(Debug, Deserialize)]
struct WasmArchivedThreadsResponse {
    pub threads: Vec<WasmDiscordChannel>,
}

fn wasm_thread_to_info(t: &WasmDiscordChannel) -> wit::ThreadInfo {
    wit::ThreadInfo {
        thread_id: t.id.clone(),
        parent_channel_id: t.parent_id.clone().unwrap_or_default(),
        message_count: t.message_count.unwrap_or(0),
        member_count: t.member_count.unwrap_or(0),
    }
}

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

    // G.5 — voice-transport WIT stubs (mirrors VoiceTransportBackend defaults).
    // Real Discord voice work lives in native lib.rs (gateway op 4); this
    // sandboxed WIT-plugin variant returns the documented defaults.
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
        // B.4 — WIT-guest gateway event parser. Mirrors the subset of
        // `DiscordClient::parse_gateway_event` that does not need the full
        // native `discord_message_to_poly` / `discord_channel_to_poly`
        // conversion surface (which would require porting ~500 LoC of
        // attachment / reaction / forum-tag / role marshalling into the
        // guest). Heavy-payload events (MESSAGE_CREATE/UPDATE, THREAD_CREATE/
        // UPDATE, THREAD_LIST_SYNC) are logged but not emitted — the host
        // can fall back to polling for messages, and the gateway already
        // serves the lightweight TYPING / PRESENCE / DELETE notifications
        // that matter for UI snappiness.
        //
        // Discord Gateway frame envelope: `{ "op": <int>, "t": <event>, "d": <payload>, "s": <seq> }`
        // Only op=0 (DISPATCH) frames have a `t` and contain real events.
        #[derive(serde::Deserialize)]
        struct GwEnvelope {
            #[serde(default)]
            op: u8,
            #[serde(default)]
            t: Option<String>,
            #[serde(default)]
            d: serde_json::Value,
        }
        let env: GwEnvelope = match serde_json::from_slice(&data) {
            Ok(e) => e,
            Err(e) => {
                host_api::log(
                    host_api::LogLevel::Debug,
                    &format!("discord-guest: gateway frame parse error: {e}"),
                );
                return;
            }
        };
        if env.op != 0 {
            // Non-dispatch (HELLO, HEARTBEAT_ACK, etc.) — nothing to emit.
            return;
        }
        let Some(event_name) = env.t.as_deref() else { return };
        let d = &env.d;

        match event_name {
            // ── MESSAGE_DELETE — channel-id + message-id, no marshalling needed.
            "MESSAGE_DELETE" => {
                let channel_id = d.get("channel_id").and_then(|v| v.as_str()).unwrap_or_default();
                let message_id = d.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                if channel_id.is_empty() || message_id.is_empty() {
                    return;
                }
                host_api::emit_event(&wit::ClientEvent::MessageDeleted(
                    wit::MessageDeletedEvent {
                        channel_id: channel_id.to_string(),
                        message_id: message_id.to_string(),
                    },
                ));
            }

            // ── TYPING_START — channel-id + user-id + current time.
            "TYPING_START" => {
                let channel_id = d.get("channel_id").and_then(|v| v.as_str()).unwrap_or_default();
                let user_id = d.get("user_id").and_then(|v| v.as_str()).unwrap_or_default();
                if channel_id.is_empty() || user_id.is_empty() {
                    return;
                }
                let timestamp = host_api::get_current_time();
                host_api::emit_event(&wit::ClientEvent::TypingStarted(
                    wit::TypingStartedEvent {
                        channel_id: channel_id.to_string(),
                        user_id: user_id.to_string(),
                        timestamp,
                    },
                ));
            }

            // ── PRESENCE_UPDATE — user-id + status enum.
            "PRESENCE_UPDATE" => {
                let user_id = d
                    .get("user")
                    .and_then(|u| u.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let status_str = d.get("status").and_then(|v| v.as_str()).unwrap_or("offline");
                if user_id.is_empty() {
                    return;
                }
                let status = match status_str {
                    "online" => wit::PresenceStatus::Online,
                    "idle" => wit::PresenceStatus::Idle,
                    "dnd" => wit::PresenceStatus::DoNotDisturb,
                    _ => wit::PresenceStatus::Offline,
                };
                host_api::emit_event(&wit::ClientEvent::PresenceChanged(
                    wit::PresenceChangedEvent {
                        user_id: user_id.to_string(),
                        status,
                    },
                ));
            }

            // ── THREAD_DELETE — emit a tombstone Channel via ChannelUpdated.
            // Native impl writes a synthetic archived+locked Thread channel
            // so subscribers can drop the entry from their caches.
            "THREAD_DELETE" => {
                let thread_id = d.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                if thread_id.is_empty() {
                    return;
                }
                let server_id = d.get("guild_id").and_then(|v| v.as_str()).unwrap_or_default();
                let parent_channel_id = d
                    .get("parent_id")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string);
                let tombstone = wit::Channel {
                    id: thread_id.to_string(),
                    name: String::new(),
                    channel_type: wit::ChannelType::Thread,
                    server_id: server_id.to_string(),
                    unread_count: 0,
                    mention_count: 0,
                    last_message_id: None,
                    forum_tags: None,
                    parent_channel_id,
                    thread_metadata: Some(wit::ThreadMetadata {
                        archived: true,
                        locked: true,
                        auto_archive_minutes: 0,
                        archived_at: None,
                        created_at: host_api::get_current_time(),
                    }),
                };
                host_api::emit_event(&wit::ClientEvent::ChannelUpdated(tombstone));
            }

            // ── Heavy-payload events — deferred. See header comment.
            //
            // MESSAGE_CREATE / MESSAGE_UPDATE need a full wit::Message which
            // requires marshalling User (with avatar URL CDN formatting),
            // attachments, reactions, reply-preview, and thread-info — the
            // native path uses `discord_message_to_poly` (~80 LoC of mapping).
            //
            // THREAD_CREATE / THREAD_UPDATE / THREAD_LIST_SYNC need a full
            // wit::Channel built from the Discord channel payload including
            // forum-tag enumeration and thread-metadata field threading.
            //
            // The host can poll `get_messages` / `get_channels` to recover
            // these. When the conversion helpers land in the guest (3.3.6+),
            // this match should be extended to emit them too.
            "MESSAGE_CREATE" | "MESSAGE_UPDATE" | "THREAD_CREATE" | "THREAD_UPDATE"
            | "THREAD_LIST_SYNC" => {
                host_api::log(
                    host_api::LogLevel::Debug,
                    &format!("discord-guest: heavy gateway event {event_name} not yet marshalled"),
                );
            }

            _ => {}
        }
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

    fn get_forum_posts(
        forum_channel_id: String,
        sort: wit::ForumSortOrder,
        limit: Option<u32>,
    ) -> Result<Vec<wit::ForumPost>, wit::ClientError> {
        let sess = current_session()?;

        // Fetch the forum channel to get its guild_id.
        let ch_body = host_get(
            &format!("{}/api/v10/channels/{forum_channel_id}", sess.base_url),
            &sess.auth_header,
        )?;
        let forum_ch: WasmDiscordChannel = parse_json(&ch_body)?;
        let guild_id = forum_ch
            .guild_id
            .ok_or_else(|| wit::ClientError::Internal("forum channel missing guild_id".into()))?;

        let cap = limit.unwrap_or(50).min(100) as usize;

        // Fetch all active threads in the guild.
        let body = host_get(
            &format!("{}/api/v10/guilds/{guild_id}/threads/active", sess.base_url),
            &sess.auth_header,
        )?;
        let active: WasmActiveThreadsResponse = parse_json(&body)?;

        let mut threads: Vec<WasmDiscordChannel> = active
            .threads
            .into_iter()
            .filter(|t| {
                t.parent_id.as_deref() == Some(&forum_channel_id)
            })
            .collect();

        match sort {
            wit::ForumSortOrder::LatestActivity => {
                // Discord returns newest-activity first by default; keep order.
            }
            wit::ForumSortOrder::CreationDate => {
                threads.sort_by(|a, b| {
                    let ts_a = a.thread_metadata.as_ref().and_then(|m| m.create_timestamp.as_deref()).unwrap_or("");
                    let ts_b = b.thread_metadata.as_ref().and_then(|m| m.create_timestamp.as_deref()).unwrap_or("");
                    ts_b.cmp(ts_a)
                });
            }
        }

        threads.truncate(cap);

        let posts = threads
            .into_iter()
            .map(|t| {
                let applied_tags = t.applied_tags.clone().unwrap_or_default();
                wit::ForumPost {
                    thread: wasm_thread_to_info(&t),
                    applied_tags,
                    starter_message_id: None,
                }
            })
            .collect();

        Ok(posts)
    }

    fn get_active_threads(
        server_id: String,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        let sess = current_session()?;
        let body = host_get(
            &format!("{}/api/v10/guilds/{server_id}/threads/active", sess.base_url),
            &sess.auth_header,
        )?;
        let resp: WasmActiveThreadsResponse = parse_json(&body)?;
        Ok(resp.threads.iter().map(wasm_thread_to_info).collect())
    }

    fn get_archived_threads(
        parent_channel_id: String,
        limit: Option<u32>,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        let sess = current_session()?;
        let cap = limit.unwrap_or(50).min(100);
        let body = host_get(
            &format!(
                "{}/api/v10/channels/{parent_channel_id}/threads/archived/public?limit={cap}",
                sess.base_url
            ),
            &sess.auth_header,
        )?;
        let resp: WasmArchivedThreadsResponse = parse_json(&body)?;
        Ok(resp.threads.iter().map(wasm_thread_to_info).collect())
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

// ─── ClientMenusGuest — F10 state-aware context menus ─────────────────────

impl ClientMenusGuest for DiscordPlugin {
    fn get_context_menu_items(
        target: MenuTargetKind,
        target_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        match target {
            MenuTargetKind::Server => {
                let muted = STATE.with(|s| s.borrow().muted_servers.contains(&target_id));
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    MenuItem {
                        id: "invite-people".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-invite-people-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "privacy-settings".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-privacy-settings-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "edit-per-server-profile".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-edit-per-server-profile-label"
                            .to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    MenuItem {
                        id: "server-boost".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-server-boost-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    mute_item,
                    MenuItem {
                        id: "leave-server".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-leave-server-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Channel => {
                let muted = STATE.with(|s| s.borrow().muted_channels.contains(&target_id));
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-channel".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-channel-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-channel".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-channel-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    mute_item,
                    MenuItem {
                        id: "mark-channel-read".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mark-channel-read-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::User => {
                let blocked = STATE.with(|s| s.borrow().blocked_users.contains(&target_id));
                let is_friend = STATE.with(|s| s.borrow().friend_ids.contains(&target_id));
                let block_item = if blocked {
                    MenuItem {
                        id: "unblock-user".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-unblock-user-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "block-user".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-block-user-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    }
                };
                let friend_item = if is_friend {
                    MenuItem {
                        id: "remove-friend".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-remove-friend-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "add-friend".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-add-friend-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    MenuItem {
                        id: "open-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-open-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    },
                    friend_item,
                    block_item,
                ])
            }
            MenuTargetKind::Message => Ok(vec![
                MenuItem {
                    id: "copy-message-link".to_string(),
                    parent_id: None,
                    slot: MenuSlot::AfterFavorites,
                    label_key: "plugin-discord-menu-copy-message-link-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Normal,
                    shortcut: None,
                    block: None,
                },
                MenuItem {
                    id: "delete-message".to_string(),
                    parent_id: None,
                    slot: MenuSlot::BeforeLeave,
                    label_key: "plugin-discord-menu-delete-message-label".to_string(),
                    icon: None,
                    item_variant: MenuItemVariant::Destructive,
                    shortcut: None,
                    block: None,
                },
            ]),
            MenuTargetKind::Dm => {
                let muted = STATE.with(|s| s.borrow().muted_dms.contains(&target_id));
                let mute_item = if muted {
                    MenuItem {
                        id: "unmute-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-unmute-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                } else {
                    MenuItem {
                        id: "mute-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::AfterFavorites,
                        label_key: "plugin-discord-menu-mute-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Normal,
                        shortcut: None,
                        block: None,
                    }
                };
                Ok(vec![
                    mute_item,
                    MenuItem {
                        id: "close-dm".to_string(),
                        parent_id: None,
                        slot: MenuSlot::BeforeLeave,
                        label_key: "plugin-discord-menu-close-dm-label".to_string(),
                        icon: None,
                        item_variant: MenuItemVariant::Destructive,
                        shortcut: None,
                        block: None,
                    },
                ])
            }
            MenuTargetKind::Category => Ok(Vec::new()),
        }
    }

    fn invoke_context_action(
        action_id: String,
        _target: MenuTargetKind,
        target_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        match action_id.as_str() {
            // Server actions
            "invite-people" | "privacy-settings" | "edit-per-server-profile"
            | "server-boost" | "leave-server" => Ok(ActionOutcome::Completed),
            "mute-server" => {
                STATE.with(|s| s.borrow_mut().muted_servers.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unmute-server" => {
                STATE.with(|s| s.borrow_mut().muted_servers.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            // Channel actions
            "mute-channel" => {
                STATE.with(|s| s.borrow_mut().muted_channels.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unmute-channel" => {
                STATE.with(|s| s.borrow_mut().muted_channels.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "mark-channel-read" => Ok(ActionOutcome::Completed),
            // User actions
            "open-dm" => Ok(ActionOutcome::Completed),
            "add-friend" => {
                STATE.with(|s| s.borrow_mut().friend_ids.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "remove-friend" => {
                STATE.with(|s| s.borrow_mut().friend_ids.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "block-user" => {
                STATE.with(|s| s.borrow_mut().blocked_users.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unblock-user" => {
                STATE.with(|s| s.borrow_mut().blocked_users.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            // Message actions
            "copy-message-link" | "delete-message" => Ok(ActionOutcome::Completed),
            // DM actions
            "mute-dm" => {
                STATE.with(|s| s.borrow_mut().muted_dms.insert(target_id));
                Ok(ActionOutcome::Completed)
            }
            "unmute-dm" => {
                STATE.with(|s| s.borrow_mut().muted_dms.remove(&target_id));
                Ok(ActionOutcome::Completed)
            }
            "close-dm" => Ok(ActionOutcome::Completed),
            _ => Err(wit::ClientError::NotFound(action_id)),
        }
    }

    fn poll_action(
        _handle: PendingHandle,
    ) -> Result<ActionOutcome, wit::ClientError> {
        Ok(ActionOutcome::Completed)
    }
}

// ─── Settings helpers ──────────────────────────────────────────────────────

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

impl ClientSettingsGuest for DiscordPlugin {
    fn get_settings_sections() -> Result<Vec<crate::wit_bindings::SettingsSection>, wit::ClientError> {
        Ok(Vec::new())
    }

    fn get_setting_value(
        scope: SettingsScope,
        scope_id: String,
        key: String,
    ) -> Result<String, wit::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        match host_api::storage_get(&storage_key) {
            Some(bytes) => String::from_utf8(bytes)
                .map_err(|e| wit::ClientError::Internal(format!("settings decode error: {e}"))),
            None => Ok("null".to_string()),
        }
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
