//! GitHub API v3 route handlers for the mock test server.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::state::GitHubState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract token from `Authorization: token {value}` or `Authorization: Bearer {value}` header.
fn token_user_id(state: &GitHubState, headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok())?;
    let token = auth
        .strip_prefix("token ")
        .or_else(|| auth.strip_prefix("Bearer "))?;
    state.auth.validate(token)
}

fn auth_error() -> (StatusCode, Json<Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "message": "Requires authentication" })),
    )
}

fn user_json(u: &crate::state::User) -> Value {
    json!({
        "id": u.id,
        "login": u.login,
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
        "pushed_at": r.pushed_at,
        "default_branch": r.default_branch,
        "html_url": r.html_url,
        "stargazers_count": r.stargazers_count,
        "language": r.language,
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
        "reactions": { "total_count": i.reactions_total },
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
    Json(json!({ "status": "ok", "backend": "github" }))
}

// ---------------------------------------------------------------------------
// GET /user
// ---------------------------------------------------------------------------

pub async fn get_user(
    State(state): State<Arc<GitHubState>>,
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
            Json(json!({ "message": "Not Found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /user/repos
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListUserReposQuery {
    #[allow(dead_code)]
    pub affiliation: Option<String>,
    #[allow(dead_code)]
    pub per_page: Option<i64>,
    #[allow(dead_code)]
    pub sort: Option<String>,
}

pub async fn list_user_repos(
    State(state): State<Arc<GitHubState>>,
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
// GET /repos/{owner}/{repo}/issues
// ---------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ListIssuesQuery {
    #[allow(dead_code)]
    pub state: Option<String>,
    #[allow(dead_code)]
    pub per_page: Option<i64>,
    #[allow(dead_code)]
    pub sort: Option<String>,
}

pub async fn list_issues(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
    Query(_q): Query<ListIssuesQuery>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    let all_issues = state
        .issues
        .get(&full_name)
        .map(|v| v.clone())
        .unwrap_or_default();

    let result: Vec<Value> = all_issues.iter().map(issue_json).collect();
    Json(result).into_response()
}

// ---------------------------------------------------------------------------
// GET /repos/{owner}/{repo}/issues/{number}/comments
// ---------------------------------------------------------------------------

pub async fn list_comments(
    State(state): State<Arc<GitHubState>>,
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
// GET /repos/{owner}/{repo}/contents  (root)
// ---------------------------------------------------------------------------

pub async fn get_contents_root(
    State(state): State<Arc<GitHubState>>,
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
            Json(json!({ "message": "Not Found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /repos/{owner}/{repo}/contents/{path}
// ---------------------------------------------------------------------------

pub async fn get_contents(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
    Path((owner, repo, path)): Path<(String, String, String)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    let file_key = format!("{full_name}/{path}");
    let subdir_key = format!("{full_name}/{path}");

    // Check if it's a known file with content
    if let Some(entry) = state.file_contents.get(&file_key) {
        return Json(entry.clone()).into_response();
    }

    // Check if it's a known subdir listing
    if let Some(entries) = state.subdir_contents.get(&subdir_key) {
        return Json(entries.clone()).into_response();
    }

    // Check if path matches a dir entry in root listing
    if let Some(root_entries) = state.contents.get(&full_name) {
        let is_dir = root_entries
            .iter()
            .any(|e| e.path == path && e.kind == "dir");
        if is_dir {
            return Json(serde_json::json!([])).into_response();
        }
    }

    (
        StatusCode::NOT_FOUND,
        Json(json!({ "message": "Not Found" })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /repos/{owner}/{repo}/issues/{number}  — single issue
// ---------------------------------------------------------------------------

pub async fn get_issue(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
    Path((owner, repo, number)): Path<(String, String, i64)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }

    let full_name = format!("{owner}/{repo}");
    let found = state
        .issues
        .get(&full_name)
        .and_then(|issues| issues.iter().find(|i| i.number == number).cloned());

    match found {
        Some(issue) => Json(issue_json(&issue)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "message": "Not Found" }))).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /user/starred/{owner}/{repo}  — check if authenticated user has starred
// ---------------------------------------------------------------------------

pub async fn check_starred(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
) -> impl IntoResponse {
    let user_id = match token_user_id(&state, &headers) {
        Some(u) => u,
        None => return auth_error().into_response(),
    };
    let key = format!("{user_id}/{owner}/{repo}");
    if state.starred.contains(&key) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(json!({ "message": "Not Found" }))).into_response()
    }
}

// ---------------------------------------------------------------------------
// POST /graphql — minimal GraphQL stub (returns empty discussions)
// ---------------------------------------------------------------------------

pub async fn graphql(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        // GraphQL returns 200 with an errors array even for auth failures,
        // but for simplicity we mirror the REST behaviour here.
        return auth_error().into_response();
    }
    // Return a well-formed empty discussions page for any query.
    Json(json!({
        "data": {
            "repository": {
                "discussions": {
                    "pageInfo": { "endCursor": null, "hasNextPage": false },
                    "nodes": []
                }
            }
        }
    }))
    .into_response()
}

// ---------------------------------------------------------------------------
// GET /repos/{owner}/{repo} — single repo with permissions
// ---------------------------------------------------------------------------

pub async fn get_repo(
    State(state): State<Arc<GitHubState>>,
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
            let perm_key = format!("{user_id}/{full_name}");
            let perms = state.repo_permissions.get(&perm_key);
            let permissions = if let Some(p) = perms {
                json!({
                    "admin": p.admin,
                    "maintain": p.maintain,
                    "push": p.push,
                    "triage": p.triage,
                    "pull": p.pull,
                })
            } else {
                // Default: read-only for unknown users
                json!({ "admin": false, "maintain": false, "push": false, "triage": false, "pull": true })
            };
            let mut repo_val = repo_json(r.value());
            repo_val["permissions"] = permissions;
            Json(repo_val).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "Not Found" })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// DELETE /repos/{owner}/{repo}/issues/comments/{comment_id}
// ---------------------------------------------------------------------------

pub async fn delete_issue_comment(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
    Path((owner, repo, comment_id)): Path<(String, String, i64)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }
    let key = format!("{owner}/{repo}/issues/comments/{comment_id}");
    state.deleted_issue_comments.insert(key);
    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// DELETE /repos/{owner}/{repo}/pulls/comments/{comment_id}
// ---------------------------------------------------------------------------

pub async fn delete_pr_comment(
    State(state): State<Arc<GitHubState>>,
    headers: HeaderMap,
    Path((owner, repo, comment_id)): Path<(String, String, i64)>,
) -> impl IntoResponse {
    if token_user_id(&state, &headers).is_none() {
        return auth_error().into_response();
    }
    let key = format!("{owner}/{repo}/pulls/comments/{comment_id}");
    state.deleted_pr_comments.insert(key);
    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------------------------------------------------------
// POST /test/auth/token — test-only bypass
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
pub struct TestAuthTokenRequest {
    pub username: String,
}

pub async fn test_auth_token(
    State(state): State<Arc<GitHubState>>,
    Json(body): Json<TestAuthTokenRequest>,
) -> impl IntoResponse {
    if !state.users.contains_key(&body.username) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "Not Found" })),
        )
            .into_response();
    }
    let token = state.auth.create_token(&body.username);
    Json(json!({ "token": token })).into_response()
}
