//! WASM Component Model guest implementation for the Lemmy messenger plugin.
//!
//! This module is only compiled when targeting `wasm32-wasip2` (gated in `lib.rs`).
//!
//! ## C.1 — Real Lemmy API implementation
//!
//! Read-only endpoints (`authenticate`, `get_servers`, `get_channels`,
//! `get_messages`, `get_forum_posts`) now route through the host HTTP bridge
//! and hit the real Lemmy v3 REST API. Mutating endpoints (post creation,
//! moderation, etc.) and DM/voice/presence remain `NotSupported` stubs —
//! the native `clients/lemmy/src/api/` layer holds those; porting them to
//! WASM-guest is tracked separately.
//!
//! Pattern follows the Discord WASM guest:
//! `thread_local!` session state + `host_api::http_request` for HTTP +
//! `serde_json` deserialization into minimal wire structs.

#![allow(unsafe_code)]

use std::cell::RefCell;

use serde::Deserialize;

use crate::wit_bindings::{
    ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, MessengerClientGuest, PluginManifest, PluginMetadataGuest, export,
    poly::messenger::host_api,
    wit,
};
use crate::wit_bindings::exports::poly::messenger::client_config::{
    Guest as ClientConfigGuest, Mechanism,
};

use exports::poly::messenger::{
    client_composer::ComposerButton,
    client_menus::{ActionOutcome, MenuItem, MenuTargetKind, PendingHandle},
    client_settings::{SettingDescriptor, SettingKind, SettingsScope, SettingsSection},
    client_sidebar::{SidebarDeclaration, SidebarLayoutKind},
    client_views::{ViewDescriptor, ViewDetail, ViewRowsPage},
};

use crate::wit_bindings::exports;

/// Zero-sized marker struct for the Lemmy WASM plugin component.
pub struct LemmyPlugin;

// ── NotSupported / stub string constants — avoids repeated heap allocations ───
const NS_GROUP_DMS: &str = "Lemmy has no group DMs";
const NS_CODE_CHANNELS: &str = "Lemmy has no code channels";
const NS_WASM_NOT_IMPL: &str = "not yet implemented in WASM plugin";
const NS_FORUM_NOT_IMPL: &str = "not implemented in WASM plugin";

// ─── Per-instance authenticated session state ─────────────────────────────

/// Minimal session data needed to make Lemmy REST calls.
#[derive(Clone)]
struct LemmyGuestSession {
    /// Bearer JWT (raw, no `Bearer ` prefix — added at request time).
    jwt: String,
    /// Base URL of the Lemmy instance (e.g. `https://lemmy.world`), no trailing slash.
    base_url: String,
    /// Authenticated user's integer ID (from `/api/v3/site`).
    user_id: i64,
    /// Authenticated user's display name (for synthesized account metadata).
    user_display_name: String,
}

thread_local! {
    /// WASM components are single-threaded; `thread_local! + RefCell` is the
    /// canonical pattern for guest-side mutable state.
    static SESSION: RefCell<Option<LemmyGuestSession>> = const { RefCell::new(None) };
}

fn current_session() -> Result<LemmyGuestSession, wit::ClientError> {
    SESSION.with(|s| {
        s.borrow()
            .clone()
            .ok_or_else(|| wit::ClientError::AuthFailed("Lemmy plugin: not authenticated".into()))
    })
}

fn set_session(s: LemmyGuestSession) {
    SESSION.with(|cell| *cell.borrow_mut() = Some(s));
}

fn clear_session() {
    SESSION.with(|cell| *cell.borrow_mut() = None);
}

// ─── HTTP helpers ─────────────────────────────────────────────────────────

fn lemmy_base_url() -> String {
    // The host bridge stores the per-account base URL in plugin KV at
    // `lemmy:base_url` — set during signup or account import. Without it
    // we fall back to lemmy.ml so anonymous discover still returns useful
    // data instead of hard-failing.
    host_api::storage_get("lemmy:base_url")
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://lemmy.ml".to_string())
}

fn http_get_json<T: for<'de> Deserialize<'de>>(
    base_url: &str,
    path: &str,
    jwt: Option<&str>,
) -> Result<T, wit::ClientError> {
    let url = format!("{base_url}{path}");
    let mut headers = vec![(
        "Content-Type".to_string(),
        "application/json".to_string(),
    )];
    if let Some(jwt) = jwt {
        headers.push(("Authorization".to_string(), format!("Bearer {jwt}")));
    }
    let resp = host_api::http_request("GET", &url, &headers, None)
        .map_err(wit::ClientError::Internal)?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(wit::ClientError::Network(format!(
            "GET {path} returned HTTP {}",
            resp.status
        )));
    }
    serde_json::from_slice(&resp.body)
        .map_err(|e| wit::ClientError::Internal(format!("JSON parse error on {path}: {e}")))
}

fn http_post_json<B: serde::Serialize, T: for<'de> Deserialize<'de>>(
    base_url: &str,
    path: &str,
    body: &B,
    jwt: Option<&str>,
) -> Result<T, wit::ClientError> {
    let url = format!("{base_url}{path}");
    let body_bytes = serde_json::to_vec(body)
        .map_err(|e| wit::ClientError::Internal(format!("JSON encode error: {e}")))?;
    let mut headers = vec![(
        "Content-Type".to_string(),
        "application/json".to_string(),
    )];
    if let Some(jwt) = jwt {
        headers.push(("Authorization".to_string(), format!("Bearer {jwt}")));
    }
    let resp = host_api::http_request("POST", &url, &headers, Some(&body_bytes))
        .map_err(wit::ClientError::Internal)?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(wit::ClientError::Network(format!(
            "POST {path} returned HTTP {}",
            resp.status
        )));
    }
    serde_json::from_slice(&resp.body)
        .map_err(|e| wit::ClientError::Internal(format!("JSON parse error on {path}: {e}")))
}

// ─── Minimal Lemmy wire types (WASM-guest only — kept independent of the
//     native `api::types` module to avoid pulling in reqwest deps). ────────

#[derive(Deserialize)]
struct WirePerson {
    id: i64,
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
}

#[derive(Deserialize)]
struct WireLocalUserView {
    person: WirePerson,
}

#[derive(Deserialize)]
struct WireMyUser {
    local_user_view: WireLocalUserView,
}

#[derive(Deserialize)]
struct WireSite {
    #[serde(default)]
    my_user: Option<WireMyUser>,
}

#[derive(Deserialize)]
struct WireCommunity {
    id: i64,
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    banner: Option<String>,
}

#[derive(Deserialize)]
struct WireCommunityView {
    community: WireCommunity,
}

#[derive(Deserialize)]
struct WireCommunityListResp {
    communities: Vec<WireCommunityView>,
}

#[derive(Deserialize)]
struct WirePost {
    id: i64,
    name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published: Option<String>,
}

#[derive(Deserialize)]
struct WirePostView {
    post: WirePost,
    creator: WirePerson,
}

#[derive(Deserialize)]
struct WirePostListResp {
    posts: Vec<WirePostView>,
}

#[derive(Deserialize)]
struct WireComment {
    id: i64,
    content: String,
    #[serde(default)]
    published: Option<String>,
}

#[derive(Deserialize)]
struct WireCommentView {
    comment: WireComment,
    creator: WirePerson,
}

#[derive(Deserialize)]
struct WireCommentListResp {
    comments: Vec<WireCommentView>,
}

#[derive(serde::Serialize)]
struct WireLoginReq<'a> {
    username_or_email: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct WireLoginResp {
    #[serde(default)]
    jwt: Option<String>,
}

// ─── Mapping helpers ──────────────────────────────────────────────────────

fn map_person_to_user(p: &WirePerson, backend: &str) -> wit::User {
    wit::User {
        id: format!("lemmy-user-{}", p.id),
        display_name: p.display_name.clone().unwrap_or_else(|| p.name.clone()),
        avatar_url: p.avatar.clone(),
        presence: wit::PresenceStatus::Offline,
        backend: backend.to_string(),
    }
}

fn map_community_to_server(
    cv: &WireCommunityView,
    backend: &str,
    account_id: &str,
    account_display_name: &str,
) -> wit::Server {
    wit::Server {
        id: format!("lemmy-community-{}", cv.community.id),
        name: cv.community.title.clone().unwrap_or_else(|| cv.community.name.clone()),
        icon_url: cv.community.icon.clone(),
        banner_url: cv.community.banner.clone(),
        categories: vec![],
        backend: backend.to_string(),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id: Some(format!("lemmy-feed-{}", cv.community.id)),
    }
}

fn community_to_channel(community_id: i64, name: &str) -> wit::Channel {
    wit::Channel {
        id: format!("lemmy-feed-{community_id}"),
        name: name.to_string(),
        channel_type: wit::ChannelType::Forum,
        server_id: format!("lemmy-community-{community_id}"),
        unread_count: 0,
        mention_count: 0,
        last_message_id: None,
        forum_tags: Some(vec![]),
        parent_channel_id: None,
        thread_metadata: None,
    }
}

fn map_post_to_message(pv: &WirePostView, backend: &str) -> wit::Message {
    let text = pv.post.body.clone().unwrap_or_else(|| pv.post.name.clone());
    wit::Message {
        id: format!("lemmy-post-{}", pv.post.id),
        author: map_person_to_user(&pv.creator, backend),
        content: wit::MessageContent::Text(text),
        timestamp: pv.post.published.clone().unwrap_or_default(),
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: false,
        thread: None,
    }
}

fn map_comment_to_message(cv: &WireCommentView, backend: &str) -> wit::Message {
    wit::Message {
        id: format!("lemmy-comment-{}", cv.comment.id),
        author: map_person_to_user(&cv.creator, backend),
        content: wit::MessageContent::Text(cv.comment.content.clone()),
        timestamp: cv.comment.published.clone().unwrap_or_default(),
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: false,
        thread: None,
    }
}

fn map_post_to_forum_post(pv: &WirePostView) -> wit::ForumPost {
    wit::ForumPost {
        thread: wit::ThreadInfo {
            thread_id: format!("lemmy-post-{}", pv.post.id),
            parent_channel_id: format!("lemmy-feed-{}", pv.post.id),
            message_count: 0,
            member_count: 0,
        },
        applied_tags: vec![],
        starter_message_id: Some(format!("lemmy-post-{}", pv.post.id)),
    }
}

fn parse_community_id(server_id: &str) -> Result<i64, wit::ClientError> {
    server_id
        .strip_prefix("lemmy-community-")
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| wit::ClientError::NotFound(format!("invalid Lemmy server id: {server_id}")))
}

fn parse_feed_channel(channel_id: &str) -> Option<i64> {
    channel_id
        .strip_prefix("lemmy-feed-")
        .and_then(|s| s.parse::<i64>().ok())
}

fn parse_post_channel(channel_id: &str) -> Option<i64> {
    channel_id
        .strip_prefix("lemmy-post-")
        .and_then(|s| s.parse::<i64>().ok())
}

const BACKEND_SLUG: &str = "lemmy";

// ─── MessengerClientGuest ─────────────────────────────────────────────────

impl MessengerClientGuest for LemmyPlugin {
    fn authenticate(
        credentials: wit::AuthCredentials,
    ) -> Result<wit::Session, wit::ClientError> {
        let base_url = lemmy_base_url();
        let jwt = match credentials {
            wit::AuthCredentials::EmailPassword(creds) => {
                let req = WireLoginReq {
                    username_or_email: &creds.email,
                    password: &creds.password,
                };
                let resp: WireLoginResp =
                    http_post_json(&base_url, "/api/v3/user/login", &req, None)?;
                resp.jwt.ok_or_else(|| {
                    wit::ClientError::AuthFailed(
                        "Lemmy login OK but no JWT returned (email verification?)".into(),
                    )
                })?
            }
            wit::AuthCredentials::Token(jwt) => jwt,
            wit::AuthCredentials::Oauth(_)
            | wit::AuthCredentials::DeviceCode(_)
            | wit::AuthCredentials::PolyServer(_) => {
                return Err(wit::ClientError::AuthFailed(
                    "Lemmy supports only EmailPassword / Token credentials".into(),
                ));
            }
        };

        let site: WireSite =
            http_get_json(&base_url, "/api/v3/site", Some(&jwt))?;
        let person = site
            .my_user
            .ok_or_else(|| {
                wit::ClientError::AuthFailed(
                    "Site OK but no my_user (JWT invalid/expired?)".into(),
                )
            })?
            .local_user_view
            .person;

        let display_name = person.display_name.clone().unwrap_or_else(|| person.name.clone());
        let session_state = LemmyGuestSession {
            jwt: jwt.clone(),
            base_url: base_url.clone(),
            user_id: person.id,
            user_display_name: display_name.clone(),
        };
        let user = map_person_to_user(&person, BACKEND_SLUG);
        set_session(session_state);

        let instance_id = base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .to_string();
        Ok(wit::Session {
            id: format!("lemmy-session-{}", person.id),
            user,
            token: jwt,
            backend: BACKEND_SLUG.to_string(),
            icon_emoji: None,
            instance_id,
            backend_url: Some(base_url),
        })
    }

    fn logout() -> Result<(), wit::ClientError> {
        clear_session();
        Ok(())
    }

    fn is_authenticated() -> bool {
        SESSION.with(|s| s.borrow().is_some())
    }

    fn get_servers() -> Result<Vec<wit::Server>, wit::ClientError> {
        let sess = current_session()?;
        let resp: WireCommunityListResp = http_get_json(
            &sess.base_url,
            "/api/v3/community/list?type_=Subscribed&limit=50",
            Some(&sess.jwt),
        )?;
        let account_id = format!("lemmy-session-{}", sess.user_id);
        Ok(resp
            .communities
            .iter()
            .map(|cv| map_community_to_server(cv, BACKEND_SLUG, &account_id, &sess.user_display_name))
            .collect())
    }

    fn get_server(id: String) -> Result<wit::Server, wit::ClientError> {
        let sess = current_session()?;
        let community_id = parse_community_id(&id)?;

        #[derive(Deserialize)]
        struct SingleResp {
            community_view: WireCommunityView,
        }
        let resp: SingleResp = http_get_json(
            &sess.base_url,
            &format!("/api/v3/community?id={community_id}"),
            Some(&sess.jwt),
        )?;
        let account_id = format!("lemmy-session-{}", sess.user_id);
        Ok(map_community_to_server(
            &resp.community_view,
            BACKEND_SLUG,
            &account_id,
            &sess.user_display_name,
        ))
    }

    fn get_channels(server_id: String) -> Result<Vec<wit::Channel>, wit::ClientError> {
        let sess = current_session()?;
        let community_id = parse_community_id(&server_id)?;

        #[derive(Deserialize)]
        struct SingleResp {
            community_view: WireCommunityView,
        }
        let resp: SingleResp = http_get_json(
            &sess.base_url,
            &format!("/api/v3/community?id={community_id}"),
            Some(&sess.jwt),
        )?;
        let name = resp
            .community_view
            .community
            .title
            .clone()
            .unwrap_or_else(|| resp.community_view.community.name.clone());
        Ok(vec![community_to_channel(community_id, &name)])
    }

    fn get_channel(id: String) -> Result<wit::Channel, wit::ClientError> {
        if let Some(community_id) = parse_feed_channel(&id) {
            let sess = current_session()?;
            #[derive(Deserialize)]
            struct SingleResp {
                community_view: WireCommunityView,
            }
            let resp: SingleResp = http_get_json(
                &sess.base_url,
                &format!("/api/v3/community?id={community_id}"),
                Some(&sess.jwt),
            )?;
            let name = resp
                .community_view
                .community
                .title
                .clone()
                .unwrap_or_else(|| resp.community_view.community.name.clone());
            return Ok(community_to_channel(community_id, &name));
        }
        Err(wit::ClientError::NotFound(format!("channel not found: {id}")))
    }

    fn send_message(
        _channel_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "send_message {NS_WASM_NOT_IMPL}"
        )))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "send_reply_message {NS_WASM_NOT_IMPL}"
        )))
    }

    fn get_messages(
        channel_id: String,
        _query: wit::MessageQuery,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        let sess = current_session()?;
        // lemmy-feed-{community_id} → posts as messages
        if let Some(community_id) = parse_feed_channel(&channel_id) {
            let resp: WirePostListResp = http_get_json(
                &sess.base_url,
                &format!("/api/v3/post/list?community_id={community_id}&sort=Hot&limit=20"),
                Some(&sess.jwt),
            )?;
            let mut out: Vec<wit::Message> = resp
                .posts
                .iter()
                .map(|pv| map_post_to_message(pv, BACKEND_SLUG))
                .collect();
            out.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            return Ok(out);
        }
        // lemmy-post-{post_id} → comments as messages
        if let Some(post_id) = parse_post_channel(&channel_id) {
            let resp: WireCommentListResp = http_get_json(
                &sess.base_url,
                &format!("/api/v3/comment/list?post_id={post_id}&sort=Hot&limit=50"),
                Some(&sess.jwt),
            )?;
            let mut out: Vec<wit::Message> = resp
                .comments
                .iter()
                .map(|cv| map_comment_to_message(cv, BACKEND_SLUG))
                .collect();
            out.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            return Ok(out);
        }
        Err(wit::ClientError::NotFound(format!(
            "unknown Lemmy channel: {channel_id}"
        )))
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
        Err(wit::ClientError::NotSupported(NS_GROUP_DMS.to_string()))
    }

    fn add_group_member(
        _group_id: String,
        _user_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(NS_GROUP_DMS.to_string()))
    }

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(vec![])
    }

    fn open_direct_message_channel(
        _user_id: String,
    ) -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotFound("DM channel not found".to_string()))
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
        BACKEND_SLUG.to_string()
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
        Err(wit::ClientError::NotSupported(NS_CODE_CHANNELS.to_string()))
    }

    fn read_file(
        _channel_id: String,
        _path: String,
    ) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(NS_CODE_CHANNELS.to_string()))
    }

    fn get_forum_posts(
        forum_channel_id: String,
        sort: wit::ForumSortOrder,
        limit: Option<u32>,
    ) -> Result<Vec<wit::ForumPost>, wit::ClientError> {
        let sess = current_session()?;
        let community_id = parse_feed_channel(&forum_channel_id).ok_or_else(|| {
            wit::ClientError::NotFound(format!(
                "get_forum_posts: not a lemmy-feed channel: {forum_channel_id}"
            ))
        })?;
        let sort_param = match sort {
            wit::ForumSortOrder::LatestActivity => "Active",
            wit::ForumSortOrder::CreationDate => "New",
        };
        let limit = limit.unwrap_or(20).min(50);
        let resp: WirePostListResp = http_get_json(
            &sess.base_url,
            &format!(
                "/api/v3/post/list?community_id={community_id}&sort={sort_param}&limit={limit}"
            ),
            Some(&sess.jwt),
        )?;
        Ok(resp.posts.iter().map(map_post_to_forum_post).collect())
    }

    fn get_active_threads(
        _server_id: String,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "get_active_threads {NS_FORUM_NOT_IMPL}"
        )))
    }

    fn get_archived_threads(
        _parent_channel_id: String,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "get_archived_threads {NS_FORUM_NOT_IMPL}"
        )))
    }

    fn create_forum_post(
        _forum_channel_id: String,
        _title: String,
        _body: String,
        _tags: Vec<String>,
    ) -> Result<wit::ForumPost, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "create_forum_post {NS_FORUM_NOT_IMPL}"
        )))
    }

    fn join_voice_channel_transport(
        _server_id: String,
        _channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no voice transport".to_string(),
        ))
    }

    fn start_dm_call_transport(
        _dm_channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no DM call transport".to_string(),
        ))
    }

    fn set_voice_mute(
        _server_id: String,
        _channel_id: String,
        _self_mute: bool,
        _self_deaf: bool,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Lemmy has no voice transport".to_string(),
        ))
    }

    fn get_signup_method(
        server_url: Option<String>,
    ) -> Result<wit::SignupMethod, wit::ClientError> {
        let base = server_url.unwrap_or_else(|| "https://lemmy.ml".to_string());
        Ok(wit::SignupMethod::External(format!(
            "{}/signup",
            base.trim_end_matches('/')
        )))
    }
}

// ─── PluginMetadataGuest ──────────────────────────────────────────────────

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

// ─── ClientMenusGuest ─────────────────────────────────────────────────────

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

    fn poll_action(_handle: PendingHandle) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(
            "no pending actions".to_string(),
        ))
    }
}

// ─── Settings helpers ─────────────────────────────────────────────────────

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

// ─── ClientSettingsGuest ──────────────────────────────────────────────────

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

// ─── ClientSidebarGuest ───────────────────────────────────────────────────

impl ClientSidebarGuest for LemmyPlugin {
    fn get_sidebar_declaration() -> Result<SidebarDeclaration, wit::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::Communities,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(action_id: String) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!(
            "unknown sidebar action: {action_id}"
        )))
    }
}

// ─── ClientViewsGuest ─────────────────────────────────────────────────────

impl ClientViewsGuest for LemmyPlugin {
    fn get_account_overview_view() -> Result<ViewDescriptor, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "get_account_overview_view {NS_WASM_NOT_IMPL}"
        )))
    }

    fn get_channel_view(_channel_id: String) -> Result<ViewDescriptor, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "get_channel_view {NS_WASM_NOT_IMPL}"
        )))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<exports::poly::messenger::client_views::Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<ViewRowsPage, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "get_view_rows {NS_WASM_NOT_IMPL}"
        )))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<ViewDetail, wit::ClientError> {
        Err(wit::ClientError::NotSupported(format!(
            "get_view_detail {NS_WASM_NOT_IMPL}"
        )))
    }
}

// ─── ClientComposerGuest ──────────────────────────────────────────────────

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

// ─── ClientConfigGuest ────────────────────────────────────────────────────

impl ClientConfigGuest for LemmyPlugin {
    fn get_client_version() -> String {
        // Mirror the native default; if a host-stored override exists, prefer it.
        host_api::storage_get("lemmy:client_version_override")
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| "poly-lemmy/0.1.0".to_string())
    }

    fn set_client_version_override(
        version_override: Option<String>,
    ) -> Result<(), wit::ClientError> {
        match version_override {
            Some(v) => host_api::storage_set("lemmy:client_version_override", v.as_bytes())
                .map_err(wit::ClientError::Internal),
            None => {
                // Best-effort clear via empty-string sentinel (host KV may not
                // expose a delete op to plugins). Reading code treats empty as default.
                host_api::storage_set("lemmy:client_version_override", b"")
                    .map_err(wit::ClientError::Internal)
            }
        }
    }

    fn get_client_mechanisms() -> Result<Vec<Mechanism>, wit::ClientError> {
        // Mirror native: one `render-previews` toggle, default ON.
        let enabled = host_api::storage_get("lemmy:mech:render-previews")
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .is_none_or(|v| v != "false");
        Ok(vec![Mechanism {
            id: "render-previews".to_string(),
            name_key: "plugin-lemmy-mechanism-render-previews-label".to_string(),
            enabled,
            requires_host_cap: None,
            description_key: Some("plugin-lemmy-mechanism-render-previews-desc".to_string()),
        }])
    }

    fn set_client_mechanism(id: String, enabled: bool) -> Result<(), wit::ClientError> {
        match id.as_str() {
            "render-previews" => host_api::storage_set(
                "lemmy:mech:render-previews",
                if enabled { b"true" } else { b"false" },
            )
            .map_err(wit::ClientError::Internal),
            other => Err(wit::ClientError::NotFound(format!(
                "unknown mechanism: {other}"
            ))),
        }
    }
}

// ─── Component export registration ────────────────────────────────────────

export!(LemmyPlugin with_types_in crate::wit_bindings);
