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
    #[allow(dead_code)]
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
/// Returns an empty comment list — the mock server does not seed comments
/// yet, but the route must exist so polling clients don't 404.
pub async fn list_comments(
    State(state): State<Arc<LemmyState>>,
    headers: HeaderMap,
    Query(_q): Query<ListCommentsQuery>,
) -> impl IntoResponse {
    if bearer_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }
    Json(json!({ "comments": [] })).into_response()
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
