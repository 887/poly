//! Hardcoded route handlers for the mock Lemmy test server.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::{
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{Value, json};

// ── Constants ────────────────────────────────────────────────────────────────

const TEST_TOKEN_PREFIX: &str = "test-token-";
const TEST_USER_ID: i64 = 1;
const TEST_USER_NAME: &str = "testuser";
const TEST_USER_DISPLAY: &str = "Test User";

// ── Auth helpers ─────────────────────────────────────────────────────────────

fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").map(str::to_string))
}

fn require_auth(headers: &HeaderMap) -> Result<String, impl IntoResponse> {
    extract_bearer(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "missing or invalid Authorization header"})),
        )
    })
}

// ── Fixed data ────────────────────────────────────────────────────────────────

fn communities() -> Vec<Value> {
    vec![
        json!({
            "community": {
                "id": 101,
                "name": "rust",
                "title": "Rust Programming Language",
                "description": "A community for Rust enthusiasts.",
                "icon": null,
                "banner": null,
                "nsfw": false,
                "removed": false,
                "deleted": false,
                "published": "2023-01-01T00:00:00Z"
            },
            "subscribed": "Subscribed",
            "counts": {
                "id": 101,
                "community_id": 101,
                "subscribers": 5000,
                "posts": 1200,
                "comments": 9000,
                "published": "2023-01-01T00:00:00Z",
                "users_active_day": 50,
                "users_active_week": 300,
                "users_active_month": 1000
            }
        }),
        json!({
            "community": {
                "id": 102,
                "name": "opensource",
                "title": "Open Source",
                "description": "All things open source.",
                "icon": null,
                "banner": null,
                "nsfw": false,
                "removed": false,
                "deleted": false,
                "published": "2023-02-01T00:00:00Z"
            },
            "subscribed": "Subscribed",
            "counts": {
                "id": 102,
                "community_id": 102,
                "subscribers": 3000,
                "posts": 500,
                "comments": 4000,
                "published": "2023-02-01T00:00:00Z",
                "users_active_day": 20,
                "users_active_week": 100,
                "users_active_month": 400
            }
        }),
        json!({
            "community": {
                "id": 103,
                "name": "linux",
                "title": "Linux",
                "description": "The Linux community.",
                "icon": null,
                "banner": null,
                "nsfw": false,
                "removed": false,
                "deleted": false,
                "published": "2023-03-01T00:00:00Z"
            },
            "subscribed": "Subscribed",
            "counts": {
                "id": 103,
                "community_id": 103,
                "subscribers": 8000,
                "posts": 2000,
                "comments": 15000,
                "published": "2023-03-01T00:00:00Z",
                "users_active_day": 80,
                "users_active_week": 400,
                "users_active_month": 1500
            }
        }),
    ]
}

fn posts_for_community(community_id: i64) -> Vec<Value> {
    vec![
        json!({
            "post": {
                "id": community_id * 10 + 1,
                "name": format!("Welcome to community {community_id}"),
                "body": "This is the welcome post for the community.",
                "url": null,
                "creator_id": 2,
                "published": "2024-01-15T10:00:00Z",
                "updated": null,
                "deleted": false,
                "nsfw": false,
                "community_id": community_id
            },
            "creator": {
                "id": 2,
                "name": "alice",
                "display_name": "Alice",
                "avatar": null,
                "banned": false
            },
            "counts": {
                "post_id": community_id * 10 + 1,
                "comments": 3,
                "score": 42,
                "upvotes": 45,
                "downvotes": 3,
                "newest_comment_time": "2024-01-15T12:00:00Z"
            },
            "my_vote": null,
            "saved": false,
            "read": false
        }),
        json!({
            "post": {
                "id": community_id * 10 + 2,
                "name": "Interesting link for this community",
                "body": null,
                "url": "https://example.com/interesting",
                "creator_id": 3,
                "published": "2024-01-16T09:30:00Z",
                "updated": null,
                "deleted": false,
                "nsfw": false,
                "community_id": community_id
            },
            "creator": {
                "id": 3,
                "name": "bob",
                "display_name": "Bob",
                "avatar": null,
                "banned": false
            },
            "counts": {
                "post_id": community_id * 10 + 2,
                "comments": 1,
                "score": 10,
                "upvotes": 11,
                "downvotes": 1,
                "newest_comment_time": "2024-01-16T10:00:00Z"
            },
            "my_vote": null,
            "saved": false,
            "read": false
        }),
        json!({
            "post": {
                "id": community_id * 10 + 3,
                "name": "Discussion: best practices",
                "body": "Let's discuss best practices in this community.",
                "url": null,
                "creator_id": 2,
                "published": "2024-01-17T14:00:00Z",
                "updated": null,
                "deleted": false,
                "nsfw": false,
                "community_id": community_id
            },
            "creator": {
                "id": 2,
                "name": "alice",
                "display_name": "Alice",
                "avatar": null,
                "banned": false
            },
            "counts": {
                "post_id": community_id * 10 + 3,
                "comments": 2,
                "score": 20,
                "upvotes": 22,
                "downvotes": 2,
                "newest_comment_time": "2024-01-17T16:00:00Z"
            },
            "my_vote": null,
            "saved": false,
            "read": false
        }),
    ]
}

fn comments_for_post(post_id: i64) -> Vec<Value> {
    vec![
        json!({
            "comment": {
                "id": post_id * 10 + 1,
                "content": "Great post! Very informative.",
                "creator_id": 3,
                "post_id": post_id,
                "path": format!("0.{}", post_id * 10 + 1),
                "published": "2024-01-15T11:00:00Z",
                "updated": null,
                "deleted": false,
                "removed": false
            },
            "creator": {
                "id": 3,
                "name": "bob",
                "display_name": "Bob",
                "avatar": null,
                "banned": false
            },
            "counts": {
                "comment_id": post_id * 10 + 1,
                "score": 5,
                "upvotes": 6,
                "downvotes": 1,
                "child_count": 1
            },
            "my_vote": null,
            "saved": false
        }),
        json!({
            "comment": {
                "id": post_id * 10 + 2,
                "content": "I agree, thanks for sharing!",
                "creator_id": 2,
                "post_id": post_id,
                "path": format!("0.{}.{}", post_id * 10 + 1, post_id * 10 + 2),
                "published": "2024-01-15T11:30:00Z",
                "updated": null,
                "deleted": false,
                "removed": false
            },
            "creator": {
                "id": 2,
                "name": "alice",
                "display_name": "Alice",
                "avatar": null,
                "banned": false
            },
            "counts": {
                "comment_id": post_id * 10 + 2,
                "score": 3,
                "upvotes": 3,
                "downvotes": 0,
                "child_count": 0
            },
            "my_vote": null,
            "saved": false
        }),
        json!({
            "comment": {
                "id": post_id * 10 + 3,
                "content": "Good discussion, everyone.",
                "creator_id": 4,
                "post_id": post_id,
                "path": format!("0.{}", post_id * 10 + 3),
                "published": "2024-01-15T12:00:00Z",
                "updated": null,
                "deleted": false,
                "removed": false
            },
            "creator": {
                "id": 4,
                "name": "carol",
                "display_name": "Carol",
                "avatar": null,
                "banned": false
            },
            "counts": {
                "comment_id": post_id * 10 + 3,
                "score": 2,
                "upvotes": 2,
                "downvotes": 0,
                "child_count": 0
            },
            "my_vote": null,
            "saved": false
        }),
    ]
}

// ── Route handlers ────────────────────────────────────────────────────────────

/// `POST /api/v3/user/login`
pub async fn login(Json(body): Json<Value>) -> impl IntoResponse {
    let username = body
        .get("username_or_email")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    Json(json!({
        "jwt": format!("{TEST_TOKEN_PREFIX}{username}"),
        "registration_created": false,
        "verify_email_sent": false
    }))
}

/// `GET /api/v3/site`
pub async fn get_site(headers: HeaderMap) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "site_view": {
                "site": {
                    "id": 1,
                    "name": "Test Lemmy Instance",
                    "description": "A test instance for Poly development.",
                    "published": "2023-01-01T00:00:00Z"
                }
            },
            "my_user": {
                "local_user_view": {
                    "local_user": {
                        "id": TEST_USER_ID,
                        "person_id": TEST_USER_ID
                    },
                    "person": {
                        "id": TEST_USER_ID,
                        "name": TEST_USER_NAME,
                        "display_name": TEST_USER_DISPLAY,
                        "avatar": null,
                        "banned": false,
                        "published": "2023-01-01T00:00:00Z"
                    }
                }
            }
        })),
    )
}

#[derive(Deserialize)]
pub struct CommunityListParams {
    #[serde(rename = "type_")]
    pub type_: Option<String>,
    pub limit: Option<u32>,
    pub page: Option<u32>,
}

/// `GET /api/v3/community/list`
pub async fn list_communities(
    headers: HeaderMap,
    Query(_params): Query<CommunityListParams>,
) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "communities": communities()
        })),
    )
}

#[derive(Deserialize)]
pub struct CommunityParams {
    pub id: Option<i64>,
    pub name: Option<String>,
}

/// `GET /api/v3/community`
pub async fn get_community(
    headers: HeaderMap,
    Query(params): Query<CommunityParams>,
) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    let community_id = params.id.unwrap_or(101);
    let all = communities();
    let found = all
        .iter()
        .find(|c| c["community"]["id"].as_i64() == Some(community_id));

    match found {
        Some(view) => (
            StatusCode::OK,
            Json(json!({
                "community_view": view,
                "moderators": [],
                "online": 5
            })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "community not found"})),
        ),
    }
}

#[derive(Deserialize)]
pub struct PostListParams {
    pub community_id: Option<i64>,
    pub sort: Option<String>,
    pub limit: Option<u32>,
    pub page: Option<u32>,
}

/// `GET /api/v3/post/list`
pub async fn list_posts(
    headers: HeaderMap,
    Query(params): Query<PostListParams>,
) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    let community_id = params.community_id.unwrap_or(101);
    let posts = posts_for_community(community_id);

    (
        StatusCode::OK,
        Json(json!({
            "posts": posts
        })),
    )
}

#[derive(Deserialize)]
pub struct CommentListParams {
    pub post_id: Option<i64>,
    pub sort: Option<String>,
    pub limit: Option<u32>,
    pub page: Option<u32>,
}

/// `GET /api/v3/comment/list`
pub async fn list_comments(
    headers: HeaderMap,
    Query(params): Query<CommentListParams>,
) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    let post_id = params.post_id.unwrap_or(1011);
    let comments = comments_for_post(post_id);

    (
        StatusCode::OK,
        Json(json!({
            "comments": comments
        })),
    )
}

/// `POST /api/v3/comment`
pub async fn create_comment(headers: HeaderMap, Json(body): Json<Value>) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    let post_id = body.get("post_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let parent_id = body.get("parent_id").and_then(|v| v.as_i64());

    let comment_id = 9000_i64 + post_id;
    let path = if let Some(pid) = parent_id {
        format!("0.{pid}.{comment_id}")
    } else {
        format!("0.{comment_id}")
    };

    (
        StatusCode::OK,
        Json(json!({
            "comment_view": {
                "comment": {
                    "id": comment_id,
                    "content": content,
                    "creator_id": TEST_USER_ID,
                    "post_id": post_id,
                    "path": path,
                    "published": "2024-01-18T10:00:00Z",
                    "updated": null,
                    "deleted": false,
                    "removed": false
                },
                "creator": {
                    "id": TEST_USER_ID,
                    "name": TEST_USER_NAME,
                    "display_name": TEST_USER_DISPLAY,
                    "avatar": null,
                    "banned": false
                },
                "counts": {
                    "comment_id": comment_id,
                    "score": 1,
                    "upvotes": 1,
                    "downvotes": 0,
                    "child_count": 0
                },
                "my_vote": 1,
                "saved": false
            }
        })),
    )
}

/// `GET /api/v3/private_message/list`
pub async fn list_private_messages(headers: HeaderMap) -> impl IntoResponse {
    if require_auth(&headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "private_messages": []
        })),
    )
}
