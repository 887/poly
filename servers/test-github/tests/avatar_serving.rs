//! Integration tests — D.1: avatar serving for test-github.
//!
//! Boots the mock GitHub API server in-process (seed() called inside router()),
//! then fetches each seeded user's avatar via the /avatars/{login}.png route and
//! asserts HTTP 200 + nonzero body + Content-Type starting with "image/".
//!
//! Avatar route: GET /avatars/{filename}
//! Seed users and their avatar URLs:
//!   penguin   → /avatars/penguin.png   (aliased to koala.png bytes)
//!   chameleon → /avatars/chameleon.png (aliased to parrot.png bytes)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn seeded_router() -> axum::Router {
    // poly_test_github::router() creates a fresh state and calls seed() internally.
    poly_test_github::router()
}

/// Fetch an avatar by filename (with extension, e.g. "penguin.png").
async fn fetch_avatar(filename: &str) -> (StatusCode, Vec<u8>, String) {
    let path = format!("/avatars/{filename}");
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
async fn penguin_avatar_200_image() {
    // penguin is aliased to koala.png bytes in the route handler
    let (status, body, ct) = fetch_avatar("penguin.png").await;
    assert_eq!(status, StatusCode::OK, "penguin avatar must return 200");
    assert!(!body.is_empty(), "penguin avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "penguin avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn chameleon_avatar_200_image() {
    // chameleon is aliased to parrot.png bytes in the route handler
    let (status, body, ct) = fetch_avatar("chameleon.png").await;
    assert_eq!(status, StatusCode::OK, "chameleon avatar must return 200");
    assert!(!body.is_empty(), "chameleon avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "chameleon avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn unknown_avatar_returns_404() {
    let (status, _body, _ct) = fetch_avatar("nonexistent.png").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Unknown avatar must return 404"
    );
}
