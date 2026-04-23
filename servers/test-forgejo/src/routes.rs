//! Forgejo API v1 route handlers for the mock test server.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::state::ForgejoState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract token from `Authorization: token {value}` header.
fn token_user_id(state: &ForgejoState, headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok())?;
    let token = auth.strip_prefix("token ")?;
    state.auth.validate(token)
}

fn auth_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "message": "token is required" })),
    )
}

fn user_json(u: &crate::state::User) -> Value {
    json!({
        "id": u.id,
        "login": u.login,
        "full_name": u.full_name,
        "avatar_url": u.avatar_url,
    })
}

fn repo_json(r: &crate::state::Repo) -> Value {
    json!({
        "id": r.id,
        "full_name": r.full_name,
        "name": r.name,
        "description": r.description,
        "owner": user_json(&r.owner),
        "private": r.private,
        "archived": r.archived,
        "updated_at": r.updated_at,
        "default_branch": r.default_branch,
        "html_url": r.html_url,
    })
}

fn issue_json(i: &crate::state::Issue) -> Value {
    json!({
        "id": i.id,
        "number": i.number,
        "title": i.title,
        "body": i.body,
        "user": user_json(&i.user),
        "state": i.state,
        "created_at": i.created_at,
        "updated_at": i.updated_at,
        "html_url": i.html_url,
        "comments": i.comments,
        "pull_request": i.pull_request,
    })
}

fn comment_json(c: &crate::state::Comment) -> Value {
    json!({
        "id": c.id,
        "user": user_json(&c.user),
        "body": c.body,
        "created_at": c.created_at,
        "html_url": c.html_url,
    })
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "backend": "forgejo" }))
}

// ---------------------------------------------------------------------------
// GET /api/v1/user
// ---------------------------------------------------------------------------

pub async fn get_user(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let user_id = match token_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };
    match state.users.get(&user_id) {
        Some(u) => Json(user_json(u.value())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "user not found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/user/repos
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListUserReposQuery {
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub sort: Option<String>,
}

pub async fn list_user_repos(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Query(_q): Query<ListUserReposQuery>,
) -> impl IntoResponse {
    let user_id = match token_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };

    let repos: Vec<Value> = state
        .repos
        .iter()
        .filter(|entry| entry.value().owner.login == user_id)
        .map(|entry| repo_json(entry.value()))
        .collect();

    Json(repos).into_response()
}

// ---------------------------------------------------------------------------
// GET /api/v1/repos/{owner}/{repo}/issues
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListIssuesQuery {
    #[allow(dead_code)]
    pub state: Option<String>,
    #[allow(dead_code)]
    pub limit: Option<i64>,
    #[allow(dead_code)]
    pub sort: Option<String>,
    /// `"issues"` or `"pulls"` — filter by kind
    pub r#type: Option<String>,
}

pub async fn list_issues(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
    Query(q): Query<ListIssuesQuery>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    let all_issues = state.issues.get(&full_name).map(|v| v.clone()).unwrap_or_default();

    let filtered: Vec<Value> = all_issues
        .iter()
        .filter(|i| match q.r#type.as_deref() {
            Some("pulls") => i.pull_request.is_some(),
            Some("issues") => i.pull_request.is_none(),
            _ => true,
        })
        .map(issue_json)
        .collect();

    Json(filtered).into_response()
}

// ---------------------------------------------------------------------------
// GET /api/v1/repos/{owner}/{repo}/issues/{number}/comments
// ---------------------------------------------------------------------------

pub async fn list_comments(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo, number)): Path<(String, String, i64)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let key = format!("{owner}/{repo}/{number}");
    let comments: Vec<Value> = state
        .comments
        .get(&key)
        .map(|v| v.iter().map(comment_json).collect())
        .unwrap_or_default();

    Json(comments).into_response()
}

// ---------------------------------------------------------------------------
// GET /api/v1/repos/{owner}/{repo}/contents  (root)
// ---------------------------------------------------------------------------

pub async fn get_contents_root(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    match state.contents.get(&full_name) {
        Some(entries) => Json(entries.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "repository not found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/repos/{owner}/{repo}/contents/{path}
// ---------------------------------------------------------------------------

pub async fn get_contents(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo, path)): Path<(String, String, String)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    let file_key = format!("{full_name}/{path}");

    // Check if it's a known file
    if let Some(entry) = state.file_contents.get(&file_key) {
        return Json(entry.clone()).into_response();
    }

    // Check if it's a directory (path matches a dir in root listing or future dir listings)
    if let Some(root_entries) = state.contents.get(&full_name) {
        // Check if path matches a dir entry in root
        let is_dir = root_entries.iter().any(|e| e.path == path && e.kind == "dir");
        if is_dir {
            // Return empty listing for unknown subdirectory contents
            return Json(serde_json::json!([])).into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "message": "file not found" })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /api/v1/repos/{owner}/{repo}/issues/{index} — single issue
// ---------------------------------------------------------------------------

pub async fn get_issue(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo, index)): Path<(String, String, i64)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    if let Some(issues) = state.issues.get(&full_name) {
        if let Some(issue) = issues.iter().find(|i| i.number == index) {
            return Json(issue_json(issue)).into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "message": "issue not found" })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /api/v1/user/starred/{owner}/{repo} — 204 if starred, 404 if not
// ---------------------------------------------------------------------------

pub async fn check_starred(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
) -> impl IntoResponse {
    let user_id = match token_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };

    let full_name = format!("{owner}/{repo}");
    let is_starred = state
        .starred
        .get(&user_id)
        .map(|set| set.contains(&full_name))
        .unwrap_or(false);

    if is_starred {
        (StatusCode::NO_CONTENT, "").into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "not starred" })),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/repos/{owner}/{repo} — single repo (includes permissions)
// ---------------------------------------------------------------------------

pub async fn get_repo(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
) -> impl IntoResponse {
    let user_id = match token_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };

    let full_name = format!("{owner}/{repo}");
    match state.repos.get(&full_name) {
        Some(r) => {
            // The authenticated user gets admin+push if they own the repo,
            // push if they're a collaborator, read-only otherwise.
            let is_owner = r.owner.login == user_id;
            let repo_json = json!({
                "id": r.id,
                "full_name": r.full_name,
                "name": r.name,
                "permissions": {
                    "admin": is_owner,
                    "push": is_owner,
                    "pull": true,
                }
            });
            Json(repo_json).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "repo not found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/repos/{owner}/{repo}/issues/comments/{id}
// ---------------------------------------------------------------------------

pub async fn delete_issue_comment(
    State(state): State<Arc<ForgejoState>>,
    headers: HeaderMap,
    Path((owner, repo, comment_id)): Path<(String, String, i64)>,
) -> impl IntoResponse {
    let user_id = match token_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };

    let full_name = format!("{owner}/{repo}");

    // Check whether the user is the repo owner (admin) — only owners can
    // delete any comment in the mock server. Others get 403.
    let is_owner = state
        .repos
        .get(&full_name)
        .map(|r| r.owner.login == user_id)
        .unwrap_or(false);

    if !is_owner {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "message": "not authorized" })),
        )
            .into_response();
    }

    // Search all issue comment lists for this repo and remove the comment.
    let mut found = false;
    for mut entry in state.comments.iter_mut() {
        if entry.key().starts_with(&format!("{full_name}/")) {
            if let Some(pos) = entry.value().iter().position(|c| c.id == comment_id) {
                entry.value_mut().remove(pos);
                found = true;
                break;
            }
        }
    }

    if found {
        (StatusCode::NO_CONTENT, "").into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "comment not found" })),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// POST /test/auth/token — test-only bypass
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct TestAuthTokenRequest {
    pub username: String,
}

/// GET /avatars/{name} — serve SVG avatar for test users.
pub async fn serve_avatar(Path(name): Path<String>) -> impl IntoResponse {
    static OTTER_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/otter.svg");
    static FLAMINGO_SVG: &[u8] = include_bytes!("../../../clients/demo/assets/flamingo.svg");

    let bytes: &[u8] = match name.as_str() {
        "otter" => OTTER_SVG,
        "flamingo" => FLAMINGO_SVG,
        // Unknown users get the otter as a fallback
        _ => OTTER_SVG,
    };
    (
        StatusCode::OK,
        [("content-type", "image/svg+xml")],
        bytes,
    )
        .into_response()
}

pub async fn test_auth_token(
    State(state): State<Arc<ForgejoState>>,
    Json(body): Json<TestAuthTokenRequest>,
) -> impl IntoResponse {
    if !state.users.contains_key(&body.username) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "user not found" })),
        )
            .into_response();
    }
    let token = state.auth.create_token(&body.username);
    Json(json!({ "token": token })).into_response()
}
