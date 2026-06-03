//! WASM Component Model guest implementation for the Forgejo messenger plugin.
//!
//! Exports the `messenger-plugin` WIT world for the wasm32-wasip2 target.
//! Mirrors native `is_backend.rs` semantics — token-only auth, read-only feed.
//! All HTTP calls route through `host_api::http_request`.

#![allow(unsafe_code)]

use std::cell::RefCell;

use serde::Deserialize;

use crate::wit_bindings::{
    ActionOutcome, ClientComposerGuest, ClientConfigGuest, ClientMenusGuest, ClientSettingsGuest,
    ClientSidebarGuest, ClientViewsGuest, Cursor, Guest, MenuItem, MenuTargetKind, PendingHandle,
    PluginMetadataGuest, SidebarDeclaration, SidebarLayoutKind, SettingsScope, export,
    poly::messenger::host_api,
    wit,
};

const FTL_EN: &str = include_str!("../locales/en/plugin.ftl");

/// Host-KV key for the client-version override (mirrors native `client.config.*` namespace).
const CLIENT_VERSION_OVERRIDE_KEY: &str = "client.config.version_override";
/// Default client version string (mirrors native `api::DEFAULT_CLIENT_VERSION`).
const DEFAULT_CLIENT_VERSION: &str = "poly-forgejo/0.0.0";

// ─── Per-instance authenticated session state ─────────────────────────────

#[derive(Clone)]
struct ForgejoGuestSession {
    /// API token (passed as `Authorization: token <TOKEN>`).
    token: String,
    /// Base URL of the Forgejo/Gitea API, e.g. `https://codeberg.org/api/v1`.
    api_base_url: String,
    /// Authenticated user's login name.
    user_login: String,
}

thread_local! {
    static SESSION: RefCell<Option<ForgejoGuestSession>> = const { RefCell::new(None) };
}

fn current_session() -> Result<ForgejoGuestSession, wit::ClientError> {
    SESSION.with(|s| {
        s.borrow()
            .clone()
            .ok_or_else(|| wit::ClientError::AuthFailed("Forgejo plugin: not authenticated".into()))
    })
}

// ─── Wire types (minimal subset of the Forgejo REST API v1 shapes) ───────

#[derive(Deserialize)]
struct FjUser {
    login: String,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct FjRepo {
    full_name: String,
    html_url: String,
}

#[derive(Deserialize)]
struct FjIssue {
    number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    state: String,
    #[serde(default)]
    html_url: Option<String>,
    user: FjUser,
    created_at: String,
}

#[derive(Deserialize)]
struct FjComment {
    id: u64,
    body: String,
    user: FjUser,
    created_at: String,
}

#[derive(Deserialize)]
struct FjContents {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    encoding: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

// ─── HTTP helpers ──────────────────────────────────────────────────────────

fn forgejo_auth_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("token {token}")),
        ("Accept".to_string(), "application/json".to_string()),
    ]
}

fn host_http_get(
    url: &str,
    token: &str,
) -> Result<crate::wit_bindings::poly::messenger::types::HttpResponse, wit::ClientError> {
    crate::wit_bindings::poly::messenger::host_api::http_request(
        "GET",
        url,
        &forgejo_auth_headers(token),
        None,
    )
    .map_err(wit::ClientError::Internal)
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    response: &crate::wit_bindings::poly::messenger::types::HttpResponse,
) -> Result<T, wit::ClientError> {
    serde_json::from_slice(&response.body)
        .map_err(|err| wit::ClientError::Internal(format!("Forgejo JSON parse error: {err}")))
}

fn check_status(
    response: &crate::wit_bindings::poly::messenger::types::HttpResponse,
    context: &str,
) -> Result<(), wit::ClientError> {
    match response.status {
        200..=299 => Ok(()),
        401 => Err(wit::ClientError::AuthFailed(format!(
            "Forgejo token rejected ({context})"
        ))),
        404 => Err(wit::ClientError::NotFound(context.to_string())),
        status => Err(wit::ClientError::Network(format!(
            "{context} returned HTTP {status}"
        ))),
    }
}

// ─── API helpers ───────────────────────────────────────────────────────────

fn fetch_authenticated_user(
    api_base_url: &str,
    token: &str,
) -> Result<FjUser, wit::ClientError> {
    let url = format!("{api_base_url}/user");
    let resp = host_http_get(&url, token)?;
    check_status(&resp, "GET /user")?;
    parse_json(&resp)
}

fn list_user_repos(
    api_base_url: &str,
    token: &str,
) -> Result<Vec<FjRepo>, wit::ClientError> {
    let url = format!("{api_base_url}/repos/search?limit=50");
    let resp = host_http_get(&url, token)?;
    check_status(&resp, "GET /repos/search")?;
    #[derive(Deserialize)]
    struct SearchResult {
        data: Vec<FjRepo>,
    }
    let result: SearchResult = parse_json(&resp)?;
    Ok(result.data)
}

fn list_issues(
    api_base_url: &str,
    token: &str,
    owner: &str,
    repo: &str,
    kind: &str,
) -> Result<Vec<FjIssue>, wit::ClientError> {
    let url = format!(
        "{api_base_url}/repos/{owner}/{repo}/issues?type={kind}&limit=30&state=open"
    );
    let resp = host_http_get(&url, token)?;
    check_status(&resp, &format!("GET /repos/{owner}/{repo}/issues?type={kind}"))?;
    parse_json(&resp)
}

fn list_issue_comments(
    api_base_url: &str,
    token: &str,
    owner: &str,
    repo: &str,
    index: u64,
) -> Result<Vec<FjComment>, wit::ClientError> {
    let url =
        format!("{api_base_url}/repos/{owner}/{repo}/issues/{index}/comments");
    let resp = host_http_get(&url, token)?;
    check_status(
        &resp,
        &format!("GET /repos/{owner}/{repo}/issues/{index}/comments"),
    )?;
    parse_json(&resp)
}

fn list_repo_contents(
    api_base_url: &str,
    token: &str,
    owner: &str,
    repo: &str,
    path: &str,
) -> Result<Vec<FjContents>, wit::ClientError> {
    let url = format!("{api_base_url}/repos/{owner}/{repo}/contents/{path}");
    let resp = host_http_get(&url, token)?;
    check_status(
        &resp,
        &format!("GET /repos/{owner}/{repo}/contents/{path}"),
    )?;
    let body_str = std::str::from_utf8(&resp.body)
        .map_err(|e| wit::ClientError::Internal(format!("UTF-8 error: {e}")))?;
    // Directory → array; file → single object
    if body_str.trim_start().starts_with('[') {
        parse_json(&resp)
    } else {
        let item: FjContents = serde_json::from_str(body_str)
            .map_err(|e| wit::ClientError::Internal(format!("Forgejo contents JSON: {e}")))?;
        Ok(vec![item])
    }
}

fn get_repo_file(
    api_base_url: &str,
    token: &str,
    owner: &str,
    repo: &str,
    path: &str,
) -> Result<FjContents, wit::ClientError> {
    let url = format!("{api_base_url}/repos/{owner}/{repo}/contents/{path}");
    let resp = host_http_get(&url, token)?;
    check_status(
        &resp,
        &format!("GET /repos/{owner}/{repo}/contents/{path}"),
    )?;
    parse_json(&resp)
}

// ─── Channel ID helpers ────────────────────────────────────────────────────
// Channel IDs mirror native channel_ids.rs / mapping.rs conventions:
//   fj-issues-{owner}~{repo}
//   fj-pulls-{owner}~{repo}
//   fj-code-{owner}~{repo}
//   fj-repo-{owner}~{repo}   (server ID)

fn server_id_for_repo(full_name: &str) -> String {
    format!("fj-repo-{}", full_name.replace('/', "~"))
}

fn repo_from_server_id(server_id: &str) -> Option<(String, String)> {
    let rest = server_id.strip_prefix("fj-repo-")?;
    let (owner, repo) = rest.split_once('~')?;
    Some((owner.to_string(), repo.to_string()))
}

fn parse_channel_owner_repo(channel_id: &str) -> Option<(String, String)> {
    let rest = channel_id
        .strip_prefix("fj-issues-")
        .or_else(|| channel_id.strip_prefix("fj-pulls-"))
        .or_else(|| channel_id.strip_prefix("fj-code-"))?;
    let (owner, repo) = rest.split_once('~')?;
    Some((owner.to_string(), repo.to_string()))
}

// ─── Type mappings ─────────────────────────────────────────────────────────

fn wit_user_from_fj(user: &FjUser) -> wit::User {
    wit::User {
        id: user.login.clone(),
        display_name: user
            .full_name
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| user.login.clone()),
        avatar_url: user.avatar_url.clone(),
        presence: wit::PresenceStatus::Offline,
        backend: "forgejo".to_string(),
    }
}

fn instance_id_for_api_base(api_base_url: &str) -> String {
    api_base_url
        .trim_end_matches("/api/v1")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

fn server_from_repo(repo: &FjRepo, account_id: &str, account_display_name: &str) -> wit::Server {
    let id = server_id_for_repo(&repo.full_name);
    let default_channel_id = Some(format!(
        "fj-issues-{}",
        repo.full_name.replace('/', "~")
    ));
    wit::Server {
        id,
        name: repo.full_name.clone(),
        icon_url: None,
        banner_url: None,
        categories: vec![],
        backend: "forgejo".to_string(),
        unread_count: 0,
        mention_count: 0,
        account_id: account_id.to_string(),
        account_display_name: account_display_name.to_string(),
        default_channel_id,
    }
}

fn channels_for_server(server_id: &str) -> Vec<wit::Channel> {
    let rest = server_id.strip_prefix("fj-repo-").unwrap_or(server_id);
    vec![
        wit::Channel {
            id: format!("fj-issues-{rest}"),
            name: "Issues".to_string(),
            channel_type: wit::ChannelType::Forum,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: Some(vec![]),
            parent_channel_id: None,
            thread_metadata: None,
        },
        wit::Channel {
            id: format!("fj-pulls-{rest}"),
            name: "Pull Requests".to_string(),
            channel_type: wit::ChannelType::Forum,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: Some(vec![]),
            parent_channel_id: None,
            thread_metadata: None,
        },
        wit::Channel {
            id: format!("fj-code-{rest}"),
            name: "Code".to_string(),
            channel_type: wit::ChannelType::Code,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        },
    ]
}

fn message_from_issue(issue: &FjIssue) -> wit::Message {
    let body = issue.body.clone().unwrap_or_default();
    wit::Message {
        id: issue.number.to_string(),
        author: wit_user_from_fj(&issue.user),
        content: wit::MessageContent::Text(format!("**{}**\n\n{body}", issue.title)),
        timestamp: issue.created_at.clone(),
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: false,
        thread: None,
    }
}

fn message_from_comment(comment: &FjComment) -> wit::Message {
    wit::Message {
        id: comment.id.to_string(),
        author: wit_user_from_fj(&comment.user),
        content: wit::MessageContent::Text(comment.body.clone()),
        timestamp: comment.created_at.clone(),
        attachments: vec![],
        reactions: vec![],
        reply_to: None,
        edited: false,
        thread: None,
    }
}

fn file_kind_from_str(s: &str) -> wit::FileKind {
    match s {
        "dir" => wit::FileKind::Directory,
        "symlink" => wit::FileKind::Symlink,
        "submodule" => wit::FileKind::Submodule,
        _ => wit::FileKind::File,
    }
}

// ─── Plugin struct ─────────────────────────────────────────────────────────

struct ForgejoPlugin;

impl Guest for ForgejoPlugin {
    fn get_signup_method(server_url: Option<String>) -> Result<wit::SignupMethod, wit::ClientError> {
        // Mirrors native is_backend.rs: external web sign-up page.
        let base = server_url.as_deref().unwrap_or("https://codeberg.org");
        Ok(wit::SignupMethod::External(format!(
            "{}/user/sign_up",
            base.trim_end_matches('/')
        )))
    }

    fn authenticate(credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        // Forgejo only supports token auth (mirrors native is_backend.rs).
        let token = match credentials {
            wit::AuthCredentials::Token(t) => t,
            _ => {
                return Err(wit::ClientError::NotSupported(
                    "Forgejo supports token auth only".into(),
                ));
            }
        };

        // The host stores the per-account base URL in plugin KV at
        // `forgejo:api_base_url` — set during signup or account import.
        let api_base_url = host_api::storage_get("forgejo:api_base_url")
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://codeberg.org/api/v1".to_string());

        let user = fetch_authenticated_user(&api_base_url, &token)?;
        let instance_id = instance_id_for_api_base(&api_base_url);
        let instance_url = api_base_url.trim_end_matches("/api/v1").to_string();

        let session = wit::Session {
            id: format!("fj-{}-{}", instance_id, user.login),
            user: wit_user_from_fj(&user),
            token: token.clone(),
            backend: "forgejo".to_string(),
            icon_emoji: Some("\u{1F98A}".to_string()), // fox emoji
            instance_id,
            backend_url: Some(instance_url),
        };

        SESSION.with(|s| {
            *s.borrow_mut() = Some(ForgejoGuestSession {
                token,
                api_base_url,
                user_login: user.login,
            });
        });

        Ok(session)
    }

    fn logout() -> Result<(), wit::ClientError> {
        SESSION.with(|s| *s.borrow_mut() = None);
        Ok(())
    }

    fn is_authenticated() -> bool {
        SESSION.with(|s| s.borrow().is_some())
    }

    fn get_servers() -> Result<Vec<wit::Server>, wit::ClientError> {
        let sess = current_session()?;
        let repos = list_user_repos(&sess.api_base_url, &sess.token)?;
        Ok(repos
            .iter()
            .map(|r| server_from_repo(r, &sess.user_login, &sess.user_login))
            .collect())
    }

    fn get_server(id: String) -> Result<wit::Server, wit::ClientError> {
        let sess = current_session()?;
        let (owner, repo_name) = repo_from_server_id(&id)
            .ok_or_else(|| wit::ClientError::NotFound(format!("server {id}")))?;
        let url = format!("{}/repos/{owner}/{repo_name}", sess.api_base_url);
        let resp = host_http_get(&url, &sess.token)?;
        check_status(&resp, &format!("GET /repos/{owner}/{repo_name}"))?;
        let repo: FjRepo = parse_json(&resp)?;
        Ok(server_from_repo(&repo, &sess.user_login, &sess.user_login))
    }

    fn get_channels(server_id: String) -> Result<Vec<wit::Channel>, wit::ClientError> {
        Ok(channels_for_server(&server_id))
    }

    fn get_channel(id: String) -> Result<wit::Channel, wit::ClientError> {
        let server_id = if let Some(rest) = id
            .strip_prefix("fj-issues-")
            .or_else(|| id.strip_prefix("fj-pulls-"))
            .or_else(|| id.strip_prefix("fj-code-"))
        {
            format!("fj-repo-{rest}")
        } else {
            return Err(wit::ClientError::NotFound(format!("channel {id}")));
        };
        channels_for_server(&server_id)
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| wit::ClientError::NotFound(format!("channel {id}")))
    }

    // Forgejo is read-only.
    fn send_message(
        _channel_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo is a read-only backend".into(),
        ))
    }

    fn send_reply_message(
        _channel_id: String,
        _reply_to_message_id: String,
        _content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo is a read-only backend".into(),
        ))
    }

    fn get_messages(
        channel_id: String,
        _query: wit::MessageQuery,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        let sess = current_session()?;

        // Issues forum channel: `fj-issues-{owner}~{repo}`
        if let Some(rest) = channel_id.strip_prefix("fj-issues-") {
            if let Some((owner, repo)) = rest.split_once('~') {
                let issues = list_issues(
                    &sess.api_base_url,
                    &sess.token,
                    owner,
                    repo,
                    "issues",
                )?;
                return Ok(issues.iter().map(message_from_issue).collect());
            }
        }
        // PR forum channel: `fj-pulls-{owner}~{repo}`
        if let Some(rest) = channel_id.strip_prefix("fj-pulls-") {
            if let Some((owner, repo)) = rest.split_once('~') {
                let pulls = list_issues(
                    &sess.api_base_url,
                    &sess.token,
                    owner,
                    repo,
                    "pulls",
                )?;
                return Ok(pulls.iter().map(message_from_issue).collect());
            }
        }
        // Issue thread channel: `fj-issue-{owner}~{repo}-{number}`
        if let Some(rest) = channel_id.strip_prefix("fj-issue-") {
            if let Some(dash_pos) = rest.rfind('-') {
                let owner_repo = &rest[..dash_pos];
                let number_str = &rest[dash_pos + 1..];
                if let Ok(number) = number_str.parse::<u64>() {
                    if let Some((owner, repo)) = owner_repo.split_once('~') {
                        let comments = list_issue_comments(
                            &sess.api_base_url,
                            &sess.token,
                            owner,
                            repo,
                            number,
                        )?;
                        return Ok(comments.iter().map(message_from_comment).collect());
                    }
                }
            }
        }
        Ok(Vec::new())
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
            "Forgejo is a read-only backend".into(),
        ))
    }

    fn get_user(id: String) -> Result<wit::User, wit::ClientError> {
        Err(wit::ClientError::NotFound(format!("user {id}")))
    }

    fn get_friends() -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(vec![])
    }

    fn get_channel_members(_channel_id: String) -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(vec![])
    }

    // Forgejo has no DM or group DM concept.
    fn get_groups() -> Result<Vec<wit::Group>, wit::ClientError> {
        Ok(vec![])
    }

    fn remove_group_member(_group_id: String, _user_id: String) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no group DMs".into(),
        ))
    }

    fn add_group_member(_group_id: String, _user_id: String) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no group DMs".into(),
        ))
    }

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(vec![])
    }

    fn open_direct_message_channel(
        _user_id: String,
    ) -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no DM concept".into(),
        ))
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no saved-messages concept".into(),
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

    fn join_voice_channel_transport(
        _server_id: String,
        _channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no voice channels".into(),
        ))
    }

    fn start_dm_call_transport(_dm_channel_id: String) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no DM calls".into(),
        ))
    }

    fn set_voice_mute(
        _server_id: String,
        _channel_id: String,
        _self_mute: bool,
        _self_deaf: bool,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no voice channels".into(),
        ))
    }

    fn get_presence(_user_id: String) -> Result<wit::PresenceStatus, wit::ClientError> {
        Ok(wit::PresenceStatus::Offline)
    }

    fn set_presence(_status: wit::PresenceStatus) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no presence concept".into(),
        ))
    }

    fn handle_ws_data(_handle: u64, _data: Vec<u8>) {
        // Forgejo has no WebSocket event stream; this is a no-op.
    }

    fn get_backend_type() -> String {
        "forgejo".to_string()
    }

    fn get_backend_name() -> String {
        "Forgejo".to_string()
    }

    fn get_backend_capabilities() -> wit::BackendCapabilities {
        wit::BackendCapabilities {
            supports_voice: false,
            supports_video: false,
            supports_dms: false,
            supports_groups: false,
            supports_send_messages: false,
            supports_presence: false,
            supports_search: false,
            supports_reactions: false,
            supports_typing_indicators: false,
            supports_file_upload: false,
            landing: wit::LandingPage::Overview,
        }
    }

    fn list_files(
        channel_id: String,
        path: String,
    ) -> Result<Vec<wit::FileEntry>, wit::ClientError> {
        let sess = current_session()?;
        let (owner, repo) = parse_channel_owner_repo(&channel_id)
            .ok_or_else(|| wit::ClientError::NotFound(format!("code channel {channel_id}")))?;

        let entries =
            list_repo_contents(&sess.api_base_url, &sess.token, &owner, &repo, &path)?;
        Ok(entries
            .iter()
            .map(|e| wit::FileEntry {
                name: e.name.clone(),
                path: e.path.clone(),
                kind: file_kind_from_str(&e.kind),
                size: e.size,
            })
            .collect())
    }

    fn read_file(
        channel_id: String,
        path: String,
    ) -> Result<wit::FileContent, wit::ClientError> {
        let sess = current_session()?;
        let (owner, repo) = parse_channel_owner_repo(&channel_id)
            .ok_or_else(|| wit::ClientError::NotFound(format!("code channel {channel_id}")))?;

        let entry = get_repo_file(&sess.api_base_url, &sess.token, &owner, &repo, &path)?;

        // Forgejo returns base64-encoded content for files.
        let content_bytes = if entry.encoding.as_deref() == Some("base64") {
            let raw = entry.content.clone().unwrap_or_default();
            let stripped: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            base64_decode(&stripped)?
        } else {
            entry.content.unwrap_or_default().into_bytes()
        };

        Ok(wit::FileContent {
            path: path.clone(),
            bytes: content_bytes,
            truncated: false,
        })
    }

    fn get_forum_posts(
        forum_channel_id: String,
        _sort: wit::ForumSortOrder,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ForumPost>, wit::ClientError> {
        let sess = current_session()?;
        let kind = if forum_channel_id.starts_with("fj-pulls-") {
            "pulls"
        } else {
            "issues"
        };
        let (owner, repo) = parse_channel_owner_repo(&forum_channel_id)
            .ok_or_else(|| {
                wit::ClientError::NotFound(format!("forum channel {forum_channel_id}"))
            })?;

        let issues =
            list_issues(&sess.api_base_url, &sess.token, &owner, &repo, kind)?;
        let owner_repo_rest = format!("{owner}~{repo}");
        Ok(issues
            .iter()
            .map(|issue| wit::ForumPost {
                thread: wit::ThreadInfo {
                    thread_id: format!("fj-issue-{owner_repo_rest}-{}", issue.number),
                    parent_channel_id: forum_channel_id.clone(),
                    message_count: 0,
                    member_count: 0,
                },
                applied_tags: vec![issue.state.clone()],
                starter_message_id: Some(issue.number.to_string()),
            })
            .collect())
    }

    fn get_active_threads(
        _server_id: String,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no thread concept".into(),
        ))
    }

    fn get_archived_threads(
        _parent_channel_id: String,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo has no thread concept".into(),
        ))
    }

    fn create_forum_post(
        _forum_channel_id: String,
        _title: String,
        _body: String,
        _tags: Vec<String>,
    ) -> Result<wit::ForumPost, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "Forgejo is a read-only backend".into(),
        ))
    }
}

// ─── PluginMetadata ────────────────────────────────────────────────────────

impl PluginMetadataGuest for ForgejoPlugin {
    fn get_translations(locale: String) -> String {
        let _ = locale;
        FTL_EN.to_string()
    }

    fn get_display_name_key() -> String {
        "plugin-forgejo-title".to_string()
    }

    fn get_icon() -> String {
        "\u{1F98A}".to_string() // fox
    }

    fn get_plugin_manifest() -> crate::wit_bindings::PluginManifest {
        crate::wit_bindings::PluginManifest {
            exec_programs: vec![],
            http_hosts: vec!["codeberg.org".to_string(), "forgejo.org".to_string()],
            description: "Reads repos, issues, pull requests, and source code from any \
                          Forgejo, Gitea, or Codeberg instance via the REST API v1."
                .to_string(),
            homepage: Some("https://forgejo.org".to_string()),
        }
    }
}

// ─── ClientConfig ──────────────────────────────────────────────────────────

impl ClientConfigGuest for ForgejoPlugin {
    fn get_client_version() -> String {
        host_api::storage_get(CLIENT_VERSION_OVERRIDE_KEY)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_else(|| DEFAULT_CLIENT_VERSION.to_string())
    }

    fn set_client_version_override(
        version_override: Option<String>,
    ) -> Result<(), wit::ClientError> {
        match version_override {
            Some(v) => host_api::storage_set(CLIENT_VERSION_OVERRIDE_KEY, v.as_bytes())
                .map_err(wit::ClientError::Internal),
            None => host_api::storage_delete(CLIENT_VERSION_OVERRIDE_KEY)
                .map_err(wit::ClientError::Internal),
        }
    }

    fn get_client_mechanisms(
    ) -> Result<Vec<crate::wit_bindings::Mechanism>, wit::ClientError> {
        Ok(Vec::new())
    }

    fn set_client_mechanism(_id: String, _enabled: bool) -> Result<(), wit::ClientError> {
        Ok(())
    }
}

// ─── ClientSettings ────────────────────────────────────────────────────────

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

impl ClientSettingsGuest for ForgejoPlugin {
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

// ─── ClientSidebar ─────────────────────────────────────────────────────────

impl ClientSidebarGuest for ForgejoPlugin {
    fn get_sidebar_declaration() -> Result<SidebarDeclaration, wit::ClientError> {
        Ok(SidebarDeclaration {
            layout: SidebarLayoutKind::RepoTree,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(action_id: String) -> Result<ActionOutcome, wit::ClientError> {
        Err(wit::ClientError::NotFound(action_id))
    }
}

// ─── ClientViews ───────────────────────────────────────────────────────────

impl ClientViewsGuest for ForgejoPlugin {
    fn get_channel_view(
        channel_id: String,
    ) -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewDescriptor,
        wit::ClientError,
    > {
        use crate::wit_bindings::exports::poly::messenger::client_views::{
            ListSpec, RowTemplate, SplitSpec, ToolbarOption, ViewBody, ViewDescriptor, ViewHeader,
            ViewKind, ViewToolbar,
        };
        let title_key = if channel_id.starts_with("fj-pulls-") {
            "plugin-forgejo-view-pulls-title"
        } else {
            "plugin-forgejo-view-issues-title"
        };
        Ok(ViewDescriptor {
            kind: ViewKind::Split,
            header: Some(ViewHeader {
                title_key: Some(title_key.to_string()),
                subtitle_key: None,
                info_block: None,
            }),
            toolbar: Some(ViewToolbar {
                sort_options: vec![],
                filter_options: vec![
                    ToolbarOption {
                        id: "open".to_string(),
                        label_key: "plugin-forgejo-filter-open".to_string(),
                        icon: None,
                        default_selected: true,
                    },
                    ToolbarOption {
                        id: "closed".to_string(),
                        label_key: "plugin-forgejo-filter-closed".to_string(),
                        icon: None,
                        default_selected: false,
                    },
                ],
                tabs: vec![],
                action_items: vec![],
            }),
            body: ViewBody::SplitBody(SplitSpec {
                list_side: ListSpec {
                    row_template: RowTemplate {
                        primary_field: "title".to_string(),
                        secondary_field: Some("number".to_string()),
                        meta_field: Some("state-labels-author".to_string()),
                        icon_field: None,
                    },
                    page_size: 30,
                },
                detail_view_kind: ViewKind::FlatList,
            }),
        })
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewRowsPage,
        wit::ClientError,
    > {
        use crate::wit_bindings::exports::poly::messenger::client_views::ViewRowsPage;
        Ok(ViewRowsPage {
            rows: vec![],
            next_cursor: None,
        })
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewDetail,
        wit::ClientError,
    > {
        Err(wit::ClientError::NotSupported(
            "get_view_detail not implemented in WASM plugin".to_string(),
        ))
    }

    fn get_account_overview_view() -> Result<
        crate::wit_bindings::exports::poly::messenger::client_views::ViewDescriptor,
        wit::ClientError,
    > {
        use crate::wit_bindings::exports::poly::messenger::client_views::{
            CardSpec, ViewBody, ViewDescriptor, ViewHeader, ViewKind,
        };
        Ok(ViewDescriptor {
            kind: ViewKind::CardGrid,
            header: Some(ViewHeader {
                title_key: Some("plugin-forgejo-overview-title".to_string()),
                subtitle_key: Some("plugin-forgejo-overview-subtitle".to_string()),
                info_block: None,
            }),
            toolbar: None,
            body: ViewBody::CardBody(CardSpec {
                primary_field: "name".to_string(),
            }),
        })
    }
}

// ─── ClientMenus ───────────────────────────────────────────────────────────

fn make_menu_item(
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

impl ClientMenusGuest for ForgejoPlugin {
    fn get_context_menu_items(
        target: MenuTargetKind,
        _target_id: String,
    ) -> Result<Vec<MenuItem>, wit::ClientError> {
        use crate::wit_bindings::exports::poly::messenger::client_menus::MenuSlot;
        match target {
            MenuTargetKind::Server => Ok(vec![
                make_menu_item(
                    "open-in-forgejo",
                    "plugin-forgejo-menu-open-in-forgejo-label",
                    MenuSlot::AfterFavorites,
                ),
                make_menu_item(
                    "star-repo",
                    "plugin-forgejo-menu-star-repo-label",
                    MenuSlot::AfterFavorites,
                ),
            ]),
            _ => Ok(vec![]),
        }
    }

    fn invoke_context_action(
        action_id: String,
        _target: MenuTargetKind,
        _target_id: String,
    ) -> Result<ActionOutcome, wit::ClientError> {
        match action_id.as_str() {
            "open-in-forgejo" | "star-repo" => Ok(ActionOutcome::Noop),
            other => Err(wit::ClientError::NotFound(format!(
                "unknown forgejo action: {other}"
            ))),
        }
    }

    fn poll_action(_handle: PendingHandle) -> Result<ActionOutcome, wit::ClientError> {
        Ok(ActionOutcome::Completed)
    }
}

// ─── ClientComposer ────────────────────────────────────────────────────────

impl ClientComposerGuest for ForgejoPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<
        Vec<crate::wit_bindings::exports::poly::messenger::client_composer::ComposerButton>,
        wit::ClientError,
    > {
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

// ─── Utility: base64 decode ────────────────────────────────────────────────

/// Decode a base64 string (standard alphabet, without pulling in a crate).
fn base64_decode(s: &str) -> Result<Vec<u8>, wit::ClientError> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut map = [0u8; 256];
    for (i, &b) in TABLE.iter().enumerate() {
        map[b as usize] = i as u8;
    }
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = Vec::with_capacity(len * 3 / 4);
    let mut i = 0;
    while i + 3 < len {
        let b0 = map[bytes[i] as usize];
        let b1 = map[bytes[i + 1] as usize];
        let b2 = if bytes[i + 2] == b'=' {
            0
        } else {
            map[bytes[i + 2] as usize]
        };
        let b3 = if i + 3 < len && bytes[i + 3] == b'=' {
            0
        } else {
            map[bytes[i + 3] as usize]
        };
        out.push((b0 << 2) | (b1 >> 4));
        if bytes[i + 2] != b'=' {
            out.push((b1 << 4) | (b2 >> 2));
        }
        if i + 3 < len && bytes[i + 3] != b'=' {
            out.push((b2 << 6) | b3);
        }
        i += 4;
    }
    Ok(out)
}

// Register the component export.
export!(ForgejoPlugin with_types_in crate::wit_bindings);
