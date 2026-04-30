//! Integration tests — D.1: avatar serving for test-teams.
//!
//! Boots the mock Teams/Graph API server in-process, calls seed(), then fetches
//! each seeded user's avatar via the Microsoft Graph profile-photo route and
//! asserts HTTP 200 + nonzero body + Content-Type starting with "image/".
//!
//! Avatar route: GET /v1.0/users/{user_id}/photo/$value
//! Seed users:   U001 (Sheep) → "sheep", U002 (Walrus) → "walrus"

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use poly_test_teams::TeamsState;
use std::sync::Arc;
use tower::ServiceExt;

fn seeded_router() -> axum::Router {
    let state = Arc::new(TeamsState::new());
    state.seed();
    poly_test_teams::router(state)
}

/// Fetch a Graph profile photo by user ID.
async fn fetch_photo(user_id: &str) -> (StatusCode, Vec<u8>, String) {
    let path = format!("/v1.0/users/{user_id}/photo/$value");
    let router = seeded_router();
    let req = Request::builder()
        .method("GET")
        .uri(&path)
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, body, ct)
}

#[tokio::test]
async fn sheep_photo_200_image() {
    let (status, body, ct) = fetch_photo("U001").await;
    assert_eq!(status, StatusCode::OK, "Sheep (U001) photo must return 200");
    assert!(!body.is_empty(), "Sheep photo body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "Sheep photo Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn walrus_photo_200_image() {
    let (status, body, ct) = fetch_photo("U002").await;
    assert_eq!(status, StatusCode::OK, "Walrus (U002) photo must return 200");
    assert!(!body.is_empty(), "Walrus photo body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "Walrus photo Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn unknown_user_photo_returns_404() {
    let (status, _body, _ct) = fetch_photo("U999").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "Unknown user photo must return 404");
}
