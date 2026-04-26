//! Lemmy API route handlers for the mock test server.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::state::LemmyState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bearer_user_id(state: &LemmyState, headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok())?;
    let token = auth.strip_prefix("Bearer ")?;
    state.auth.validate(token)
}

fn auth_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "not_logged_in" })),
    )
}

fn community_view(c: &crate::state::Community) -> Value {
    json!({
        "community": {
            "id": c.id,
            "name": c.name,
            "title": c.title,
            "description": c.description,
            "icon": c.icon,
            "banner": c.banner,
            "actor_id": c.actor_id,
            "local": true,
            "removed": false,
            "deleted": false,
            "nsfw": false,
            "hidden": false,
            "posting_restricted_to_mods": false,
            "instance_id": 1,
        },
        "subscribed": if c.subscribed { "Subscribed" } else { "NotSubscribed" },
        "blocked": false,
        "counts": {
            "id": c.id,
            "community_id": c.id,
            "subscribers": 100,
            "posts": 10,
            "comments": 50,
            "published": "2024-01-01T00:00:00Z",
            "users_active_day": 5,
            "users_active_week": 15,
            "users_active_month": 40,
            "users_active_half_year": 80,
            "hot_rank": 1000,
        },
    })
}

fn post_view(p: &crate::state::Post) -> Value {
    json!({
        "post": {
            "id": p.id,
            "name": p.name,
            "body": p.body,
            "url": p.url,
            "creator_id": p.creator_id,
            "community_id": p.community_id,
            "removed": false,
            "locked": false,
            "published": p.published,
            "deleted": false,
            "nsfw": false,
            "ap_id": format!("https://lemmy.example.com/post/{}", p.id),
            "local": true,
            "embed_title": null,
            "embed_description": null,
            "embed_video_url": null,
            "thumbnail_url": null,
            "language_id": 0,
            "featured_community": false,
            "featured_local": false,
            "instance_id": 1,
        },
        "creator": {
            "id": p.creator_id,
            "name": p.creator_name,
            "display_name": null,
            "avatar": null,
            "banned": false,
            "published": "2024-01-01T00:00:00Z",
            "actor_id": format!("https://lemmy.example.com/u/{}", p.creator_name),
            "local": true,
            "deleted": false,
            "matrix_user_id": null,
            "admin": false,
            "bot_account": false,
            "instance_id": 1,
        },
        "community": {
            "id": p.community_id,
            "name": "community",
            "title": "Community",
            "removed": false,
            "published": "2024-01-01T00:00:00Z",
            "deleted": false,
            "nsfw": false,
            "actor_id": format!("https://lemmy.example.com/c/community{}", p.community_id),
            "local": true,
            "hidden": false,
            "posting_restricted_to_mods": false,
            "instance_id": 1,
        },
        "creator_banned_from_community": false,
        "counts": {
            "id": p.id,
            "post_id": p.id,
            "comments": p.comment_count,
            "score": p.score,
            "upvotes": p.score.max(0),
            "downvotes": 0,
            "hot_rank": 1000,
            "hot_rank_active": 1000,
            "published": p.published,
            "newest_comment_time_necro": p.published,
            "newest_comment_time": p.published,
            "featured_community": false,
            "featured_local": false,
            "controversy_rank": 0.0_f64,
        },
        "subscribed": "Subscribed",
        "saved": false,
        "read": false,
        "creator_blocked": false,
        "my_vote": null,
        "unread_comments": 0,
    })
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "backend": "lemmy" }))
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username_or_email: String,
    pub password: String,
}

/// POST /api/v3/user/login
pub async fn login(
    State(state): State<Arc<LemmyState>>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    let username = body.username_or_email.trim().to_string();
    let expected_password = state.passwords.get(&username).map(|p| p.clone());

    match expected_password {
        Some(pw) if pw == body.password => {
            let token = state.auth.create_token(&username);
            (StatusCode::OK, Json(json!({ "jwt": token }))).into_response()
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "incorrect_login" })),
        )
            .into_response(),
    }
}

/// POST /api/v3/user/logout
pub async fn logout(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(auth) = headers.get("authorization").and_then(|v| v.to_str().ok())
        && let Some(token) = auth.strip_prefix("Bearer ")
    {
        state.auth.revoke(token);
    }
    Json(json!({}))
}

// ---------------------------------------------------------------------------
// Communities
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListCommunitiesQuery {
    #[allow(dead_code)]
    pub type_: Option<String>,
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub page: Option<i64>,
}

/// GET /api/v3/community/list
pub async fn list_communities(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(_q): Query<ListCommunitiesQuery>,
) -> impl IntoResponse {
    // Require auth
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let communities: Vec<Value> = state
        .communities
        .iter()
        .map(|entry| community_view(entry.value()))
        .collect();

    Json(json!({ "communities": communities })).into_response()
}

#[derive(Deserialize)]
pub struct GetCommunityQuery {
    pub id: Option<i64>,
    pub name: Option<String>,
}

/// GET /api/v3/community
pub async fn get_community(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(q): Query<GetCommunityQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let community = if let Some(id) = q.id {
        state.communities.get(&id.to_string()).map(|e| e.clone())
    } else if let Some(name) = q.name {
        state
            .communities
            .iter()
            .find(|e| e.value().name == name)
            .map(|e| e.value().clone())
    } else {
        None
    };

    match community {
        Some(c) => Json(json!({ "community_view": community_view(&c), "site": null, "moderators": [], "discussion_languages": [] })).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "couldnt_find_community" }))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Posts
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListPostsQuery {
    pub community_id: Option<i64>,
    pub community_name: Option<String>,
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub page: Option<i64>,
    #[allow(dead_code)]
    pub sort: Option<String>,
    #[allow(dead_code)]
    pub type_: Option<String>,
}

/// GET /api/v3/post/list
pub async fn list_posts(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(q): Query<ListPostsQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let community_id = if let Some(id) = q.community_id {
        Some(id.to_string())
    } else if let Some(name) = q.community_name {
        state
            .communities
            .iter()
            .find(|e| e.value().name == name)
            .map(|e| e.value().id.to_string())
    } else {
        None
    };

    let posts: Vec<Value> = match community_id {
        Some(cid) => state
            .posts
            .get(&cid)
            .map(|posts| posts.iter().map(post_view).collect())
            .unwrap_or_default(),
        None => {
            // Return all posts across all communities
            state
                .posts
                .iter()
                .flat_map(|entry| entry.value().iter().map(post_view).collect::<Vec<_>>())
                .collect()
        }
    };

    Json(json!({ "posts": posts, "next_page": null })).into_response()
}

#[derive(Deserialize)]
pub struct GetPostQuery {
    pub id: Option<i64>,
}

/// GET /api/v3/post?id={id} — fetch a single post.
pub async fn get_post(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(q): Query<GetPostQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let Some(post_id) = q.id else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing_id" })),
        )
            .into_response();
    };

    let found = state
        .posts
        .iter()
        .flat_map(|entry| entry.value().clone())
        .find(|p| p.id == post_id);

    match found {
        Some(p) => Json(json!({
            "post_view": post_view(&p),
            "community_view": null,
            "moderators": [],
            "cross_posts": [],
        }))
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "couldnt_find_post" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Private Messages
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListPrivateMessagesQuery {
    #[allow(dead_code)]
    pub unread_only: Option<bool>,
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub page: Option<i64>,
}

/// GET /api/v3/private_message/list
pub async fn list_private_messages(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(_q): Query<ListPrivateMessagesQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    // Mock server has no private messages — Lemmy federation makes this rare
    Json(json!({ "private_messages": [] })).into_response()
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GetUserQuery {
    pub username: Option<String>,
    pub person_id: Option<i64>,
}

/// GET /api/v3/user
pub async fn get_user(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(q): Query<GetUserQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let user = if let Some(username) = q.username {
        state.users.get(&username).map(|e| e.clone())
    } else if let Some(pid) = q.person_id {
        state
            .users
            .iter()
            .find(|e| e.value().id == pid)
            .map(|e| e.value().clone())
    } else {
        None
    };

    match user {
        Some(u) => Json(json!({
            "person_view": {
                "person": {
                    "id": u.id,
                    "name": u.name,
                    "display_name": u.display_name,
                    "avatar": u.avatar,
                    "banned": false,
                    "published": "2024-01-01T00:00:00Z",
                    "actor_id": u.actor_id,
                    "local": true,
                    "deleted": false,
                    "bot_account": false,
                    "instance_id": 1,
                },
                "counts": {
                    "id": u.id,
                    "person_id": u.id,
                    "post_count": 10,
                    "comment_count": 50,
                    "published": "2024-01-01T00:00:00Z",
                }
            },
            "comments": [],
            "posts": [],
            "moderates": [],
        })).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "couldnt_find_that_username_or_email" }))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Site info (current user)
// ---------------------------------------------------------------------------

/// GET /api/v3/site — returns current logged-in user info.
pub async fn get_site(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match bearer_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };
    let user = state.users.get(&user_id).map(|u| u.clone());
    match user {
        Some(u) => Json(json!({
            "site_view": {
                "site": { "id": 1, "name": "Test Lemmy" }
            },
            "my_user": {
                "local_user_view": {
                    "local_user": { "id": u.id, "person_id": u.id },
                    "person": {
                        "id": u.id,
                        "name": u.name,
                        "display_name": u.display_name,
                        "avatar": u.avatar,
                        "banned": false,
                        "published": "2024-01-01T00:00:00Z",
                        "actor_id": u.actor_id,
                        "local": true,
                        "deleted": false,
                        "bot_account": false,
                        "instance_id": 1,
                    }
                }
            }
        })).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "user_not_found" }))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListCommentsQuery {
    pub post_id: Option<i64>,
    #[allow(dead_code)]
    pub community_id: Option<i64>,
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub page: Option<i64>,
    #[allow(dead_code)]
    pub sort: Option<String>,
    #[allow(dead_code)]
    pub type_: Option<String>,
}

/// GET /api/v3/comment/list
///
/// Returns comments stored for the post referenced by `post_id` query param.
pub async fn list_comments(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(q): Query<ListCommentsQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    // If a post_id is provided, look up comments keyed by post ID.
    if let Some(post_id) = q.post_id {
        let comments: Vec<Value> = state
            .comments
            .iter()
            .flat_map(|entry| {
                entry
                    .value()
                    .iter()
                    .filter(|c| c.post_id == post_id)
                    .map(|c| comment_view(c, &state))
                    .collect::<Vec<_>>()
            })
            .collect();
        return Json(json!({ "comments": comments })).into_response();
    }

    Json(json!({ "comments": [] })).into_response()
}

#[derive(Deserialize)]
pub struct CreateCommentRequest {
    pub content: String,
    pub post_id: i64,
    #[serde(default)]
    pub parent_id: Option<i64>,
}

/// POST /api/v3/comment — create a new comment on a post.
pub async fn create_comment(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Json(body): Json<CreateCommentRequest>,
) -> impl IntoResponse {
    let user_id = match bearer_user_id(&state, &headers) {
        Some(uid) => uid,
        None => return auth_error().into_response(),
    };

    let user = match state.users.get(&user_id) {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "user_not_found" })),
            )
                .into_response();
        }
    };

    // Find the community this post belongs to
    let post_community_id: Option<String> = state
        .posts
        .iter()
        .find_map(|entry| {
            entry
                .value()
                .iter()
                .find(|p| p.id == body.post_id)
                .map(|p| p.community_id.to_string())
        });

    let community_id_str = match post_community_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "post_not_found" })),
            )
                .into_response();
        }
    };

    // Allocate a new comment ID
    let new_id = {
        let max = state
            .comments
            .iter()
            .flat_map(|e| e.value().iter().map(|c| c.id).collect::<Vec<_>>())
            .max()
            .unwrap_or(0);
        max + 1
    };

    let comment = crate::state::Comment {
        id: new_id,
        content: body.content.clone(),
        creator_id: user.id,
        creator_name: user.name.clone(),
        post_id: body.post_id,
        community_id: community_id_str.parse::<i64>().unwrap_or(0),
        published: chrono::Utc::now().to_rfc3339(),
    };

    let view = comment_view(&comment, &state);

    state
        .comments
        .entry(community_id_str)
        .or_default()
        .push(comment);

    Json(json!({ "comment_view": view })).into_response()
}

fn comment_view(comment: &crate::state::Comment, state: &LemmyState) -> Value {
    let creator = state
        .users
        .get(&comment.creator_name)
        .map(|u| {
            json!({
                "id": u.id,
                "name": u.name,
                "display_name": u.display_name,
                "avatar": u.avatar,
                "actor_id": u.actor_id,
            })
        })
        .unwrap_or_else(|| json!({ "id": comment.creator_id, "name": comment.creator_name }));

    json!({
        "comment": {
            "id": comment.id,
            "content": comment.content,
            "creator_id": comment.creator_id,
            "post_id": comment.post_id,
            "community_id": comment.community_id,
            "published": comment.published,
            "deleted": false,
            "removed": false,
            "local": true,
            "path": format!("0.{}", comment.id),
        },
        "creator": creator,
        "counts": {
            "comment_id": comment.id,
            "score": 0,
            "upvotes": 0,
            "downvotes": 0,
            "published": comment.published,
        }
    })
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub password_verify: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub show_nsfw: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)]
    pub email: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub captcha_uuid: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub captcha_answer: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub answer: Option<String>,
}

/// POST /api/v3/user/register
pub async fn register(
    State(state): State<Arc<LemmyState>>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    let username = body.username.trim().to_string();

    if username.is_empty() || body.password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_registration" })),
        )
            .into_response();
    }

    if state.users.contains_key(&username) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "username_already_exists" })),
        )
            .into_response();
    }

    let next_id = (state.users.len() as i64) + 1;
    state.users.insert(
        username.clone(),
        crate::state::User {
            id: next_id,
            name: username.clone(),
            display_name: None,
            avatar: None,
            actor_id: format!("https://lemmy.example.com/u/{}", username),
        },
    );
    state.passwords.insert(username.clone(), body.password);

    let token = state.auth.create_token(&username);
    Json(json!({
        "jwt": token,
        "registration_created": true,
        "verify_email_sent": false,
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Community update
// ---------------------------------------------------------------------------

/// PUT /api/v3/community — update community (EditCommunity).
///
/// Accepts `banner` (string URL or JSON null to clear) and other optional
/// fields. Uses raw `Value` extraction so that a JSON `null` value is
/// distinguished from a field that is simply absent.
/// Returns `{ community_view: … }`.
pub async fn edit_community(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let community_id = match body.get("community_id").and_then(|v| v.as_i64()) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing_community_id" })),
            )
                .into_response();
        }
    };

    let key = community_id.to_string();
    let mut entry = match state.communities.get_mut(&key) {
        Some(e) => e,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "couldnt_find_community" })),
            )
                .into_response();
        }
    };

    // Apply updates — field present (even as null) means "set to this value".
    if let Some(banner_val) = body.get("banner") {
        entry.banner = if banner_val.is_null() {
            None
        } else {
            banner_val.as_str().map(str::to_string)
        };
    }
    if let Some(icon_val) = body.get("icon") {
        entry.icon = if icon_val.is_null() {
            None
        } else {
            icon_val.as_str().map(str::to_string)
        };
    }
    if let Some(title) = body.get("title").and_then(|v| v.as_str()) {
        entry.title = title.to_string();
    }
    if let Some(desc) = body.get("description").and_then(|v| v.as_str()) {
        entry.description = Some(desc.to_string());
    }

    let view = community_view(&entry);
    Json(json!({ "community_view": view })).into_response()
}

// ---------------------------------------------------------------------------
// Moderation: community/ban_user
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BanFromCommunityRequest {
    pub community_id: i64,
    pub person_id: i64,
    pub ban: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub expires: Option<i64>,
    #[serde(default)]
    pub remove_data: bool,
}

/// POST /api/v3/community/ban_user
pub async fn community_ban_user(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Json(body): Json<BanFromCommunityRequest>,
) -> impl IntoResponse {
    let mod_username = match bearer_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };
    let mod_user = match state.users.get(&mod_username) {
        Some(u) => u.clone(),
        None => return auth_error().into_response(),
    };

    // Look up the target person.
    let target = match state
        .users
        .iter()
        .find(|e| e.value().id == body.person_id)
        .map(|e| e.value().clone())
    {
        Some(u) => u,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "couldnt_find_person" })),
            )
                .into_response();
        }
    };

    let when_ = chrono::Utc::now().to_rfc3339();
    let entry_id = state.next_modlog_id();

    let ban_entry = crate::state::BanEntry {
        id: entry_id,
        community_id: body.community_id,
        person_id: body.person_id,
        person_name: target.name.clone(),
        moderator_id: mod_user.id,
        banned: body.ban,
        reason: body.reason.clone(),
        expires: body.expires,
        when_: when_.clone(),
    };

    state
        .bans
        .entry(body.community_id.to_string())
        .or_default()
        .push(ban_entry);

    (
        StatusCode::OK,
        Json(json!({
            "banned_person": {
                "person": {
                    "id": target.id,
                    "name": target.name,
                    "display_name": target.display_name,
                    "avatar": target.avatar,
                    "banned": body.ban,
                    "published": "2024-01-01T00:00:00Z",
                    "actor_id": target.actor_id,
                    "local": true,
                    "deleted": false,
                    "bot_account": false,
                    "instance_id": 1,
                },
                "counts": { "id": target.id, "person_id": target.id, "post_count": 0, "comment_count": 0, "published": "2024-01-01T00:00:00Z" }
            },
            "banned": body.ban
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Moderation: post/remove and comment/remove
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RemovePostRequest {
    pub post_id: i64,
    pub removed: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// POST /api/v3/post/remove
pub async fn post_remove(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Json(body): Json<RemovePostRequest>,
) -> impl IntoResponse {
    let mod_username = match bearer_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };
    let mod_user = match state.users.get(&mod_username) {
        Some(u) => u.clone(),
        None => return auth_error().into_response(),
    };

    // Find the post.
    let post = state
        .posts
        .iter()
        .flat_map(|entry| entry.value().clone())
        .find(|p| p.id == body.post_id);

    let post = match post {
        Some(p) => p,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "couldnt_find_post" })),
            )
                .into_response();
        }
    };

    let when_ = chrono::Utc::now().to_rfc3339();
    let entry_id = state.next_modlog_id();

    let modlog_entry = crate::state::ModlogEntry {
        id: entry_id,
        community_id: post.community_id,
        moderator_id: mod_user.id,
        action: "ModRemovePost".to_string(),
        post_id: Some(post.id),
        post_name: Some(post.name.clone()),
        comment_id: None,
        comment_content: None,
        commenter_id: None,
        commenter_name: None,
        reason: body.reason.clone(),
        removed: body.removed,
        when_: when_,
    };

    state
        .modlog
        .entry(post.community_id.to_string())
        .or_default()
        .push(modlog_entry);

    (StatusCode::OK, Json(json!({ "post_view": post_view(&post) }))).into_response()
}

#[derive(Deserialize)]
pub struct RemoveCommentRequest {
    pub comment_id: i64,
    pub removed: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// POST /api/v3/comment/remove
pub async fn comment_remove(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Json(body): Json<RemoveCommentRequest>,
) -> impl IntoResponse {
    let mod_username = match bearer_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };
    let mod_user = match state.users.get(&mod_username) {
        Some(u) => u.clone(),
        None => return auth_error().into_response(),
    };

    // Find the comment.
    let comment = state
        .comments
        .iter()
        .flat_map(|entry| entry.value().clone())
        .find(|c| c.id == body.comment_id);

    let comment = match comment {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "couldnt_find_comment" })),
            )
                .into_response();
        }
    };

    let commenter = state
        .users
        .iter()
        .find(|e| e.value().id == comment.creator_id)
        .map(|e| e.value().clone());

    let when_ = chrono::Utc::now().to_rfc3339();
    let entry_id = state.next_modlog_id();

    let modlog_entry = crate::state::ModlogEntry {
        id: entry_id,
        community_id: comment.community_id,
        moderator_id: mod_user.id,
        action: "ModRemoveComment".to_string(),
        post_id: None,
        post_name: None,
        comment_id: Some(comment.id),
        comment_content: Some(comment.content.clone()),
        commenter_id: commenter.as_ref().map(|u| u.id),
        commenter_name: commenter.as_ref().map(|u| u.name.clone()),
        reason: body.reason.clone(),
        removed: body.removed,
        when_: when_,
    };

    state
        .modlog
        .entry(comment.community_id.to_string())
        .or_default()
        .push(modlog_entry);

    (StatusCode::OK, Json(json!({ "comment_view": {
        "comment": {
            "id": comment.id,
            "content": comment.content,
            "published": comment.published,
        },
        "creator": {
            "id": comment.creator_id,
            "name": comment.creator_name,
        }
    }})))
        .into_response()
}

// ---------------------------------------------------------------------------
// Moderation: modlog
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ModlogQuery {
    pub community_id: Option<i64>,
    #[serde(rename = "type_")]
    pub type_: Option<String>,
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub page: Option<i64>,
}

/// GET /api/v3/modlog
pub async fn get_modlog(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(q): Query<ModlogQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    // Build banned_from_community array from ban entries.
    let build_ban_entry = |be: &crate::state::BanEntry| -> Value {
        let person = state
            .users
            .iter()
            .find(|e| e.value().id == be.person_id)
            .map(|e| e.value().clone());
        let mod_user = state
            .users
            .iter()
            .find(|e| e.value().id == be.moderator_id)
            .map(|e| e.value().clone());
        json!({
            "mod_ban_from_community": {
                "id": be.id,
                "when_": be.when_,
                "reason": be.reason,
                "banned": be.banned,
                "expires": be.expires.map(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt: chrono::DateTime<chrono::Utc>| dt.to_rfc3339())
                }),
            },
            "moderator": mod_user.as_ref().map(|u| json!({
                "id": u.id,
                "name": u.name,
                "display_name": u.display_name,
                "avatar": u.avatar,
            })),
            "banned_person": person.as_ref().map(|u| json!({
                "id": u.id,
                "name": u.name,
                "display_name": u.display_name,
                "avatar": u.avatar,
            })).unwrap_or(json!({
                "id": be.person_id,
                "name": be.person_name,
                "display_name": null,
                "avatar": null,
            })),
            "community": {
                "id": be.community_id,
                "title": "",
                "icon": null,
                "banner": null,
            }
        })
    };

    let build_remove_post_entry = |me: &crate::state::ModlogEntry| -> Value {
        let mod_user = state
            .users
            .iter()
            .find(|e| e.value().id == me.moderator_id)
            .map(|e| e.value().clone());
        json!({
            "mod_remove_post": {
                "id": me.id,
                "when_": me.when_,
                "reason": me.reason,
                "removed": me.removed,
            },
            "moderator": mod_user.as_ref().map(|u| json!({
                "id": u.id,
                "name": u.name,
                "display_name": u.display_name,
            })),
            "post": {
                "id": me.post_id.unwrap_or(0),
                "name": me.post_name.clone().unwrap_or_default(),
                "body": null,
                "url": null,
                "published": me.when_,
            },
            "community": {
                "id": me.community_id,
                "title": "",
                "icon": null,
                "banner": null,
            }
        })
    };

    let build_remove_comment_entry = |me: &crate::state::ModlogEntry| -> Value {
        let mod_user = state
            .users
            .iter()
            .find(|e| e.value().id == me.moderator_id)
            .map(|e| e.value().clone());
        json!({
            "mod_remove_comment": {
                "id": me.id,
                "when_": me.when_,
                "reason": me.reason,
                "removed": me.removed,
            },
            "moderator": mod_user.as_ref().map(|u| json!({
                "id": u.id,
                "name": u.name,
                "display_name": u.display_name,
            })),
            "comment": {
                "id": me.comment_id.unwrap_or(0),
                "content": me.comment_content.clone().unwrap_or_default(),
                "published": me.when_,
            },
            "commenter": {
                "id": me.commenter_id.unwrap_or(0),
                "name": me.commenter_name.clone().unwrap_or_default(),
                "display_name": null,
                "avatar": null,
            },
            "community": {
                "id": me.community_id,
                "title": "",
                "icon": null,
                "banner": null,
            }
        })
    };

    let type_filter = q.type_.as_deref().unwrap_or("All");

    let mut banned_from_community: Vec<Value> = Vec::new();
    let mut removed_posts: Vec<Value> = Vec::new();
    let mut removed_comments: Vec<Value> = Vec::new();

    // If community_id given, filter; else include all.
    let community_ids: Vec<String> = if let Some(cid) = q.community_id {
        vec![cid.to_string()]
    } else {
        state.communities.iter().map(|e| e.key().clone()).collect()
    };

    for cid in &community_ids {
        if type_filter == "All" || type_filter == "ModBanFromCommunity" {
            if let Some(entries) = state.bans.get(cid) {
                for be in entries.iter() {
                    banned_from_community.push(build_ban_entry(be));
                }
            }
        }

        if type_filter == "All" || type_filter == "ModRemovePost" {
            if let Some(entries) = state.modlog.get(cid) {
                for me in entries.iter().filter(|e| e.action == "ModRemovePost") {
                    removed_posts.push(build_remove_post_entry(me));
                }
            }
        }

        if type_filter == "All" || type_filter == "ModRemoveComment" {
            if let Some(entries) = state.modlog.get(cid) {
                for me in entries.iter().filter(|e| e.action == "ModRemoveComment") {
                    removed_comments.push(build_remove_comment_entry(me));
                }
            }
        }
    }

    Json(json!({
        "banned_from_community": banned_from_community,
        "removed_posts": removed_posts,
        "removed_comments": removed_comments,
        "locked_posts": [],
        "featured_posts": [],
        "removed_communities": [],
        "banned": [],
        "added_to_community": [],
        "transferred_to_community": [],
        "added": [],
        "admin_purged_persons": [],
        "admin_purged_communities": [],
        "admin_purged_posts": [],
        "admin_purged_comments": [],
        "hidden_communities": [],
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// Test-only endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TestAuthTokenRequest {
    pub username: String,
}

/// POST /test/auth/token — get a token without a password (test only)
pub async fn test_auth_token(
    State(state): State<Arc<LemmyState>>,
    Json(body): Json<TestAuthTokenRequest>,
) -> impl IntoResponse {
    if !state.users.contains_key(&body.username) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "user_not_found" })),
        )
            .into_response();
    }
    let token = state.auth.create_token(&body.username);
    Json(json!({ "jwt": token })).into_response()
}
