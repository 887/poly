//! Integration tests — D.1: avatar serving for test-forgejo.
//!
//! Boots the mock Forgejo server in-process (seed() called inside router()),
//! then fetches each seeded user's avatar via the /avatars/{name} route and
//! asserts HTTP 200 + nonzero body + Content-Type starting with "image/".
//!
//! Avatar route: GET /avatars/{name}
//! Seed users and their avatar names:
//!   otter     → otter     (SVG)
//!   flamingo  → flamingo  (SVG)
//!   testuser  → axolotl   (SVG — same asset as Lemmy testuser)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn seeded_router() -> axum::Router {
    // poly_test_forgejo::router() creates a fresh state and calls seed() internally.
    poly_test_forgejo::router()
}

/// Fetch an avatar by bare animal name (no extension).
async fn fetch_avatar(name: &str) -> (StatusCode, Vec<u8>, String) {
    let path = format!("/avatars/{name}");
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
async fn otter_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("otter").await;
    assert_eq!(status, StatusCode::OK, "otter avatar must return 200");
    assert!(!body.is_empty(), "otter avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "otter avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn flamingo_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("flamingo").await;
    assert_eq!(status, StatusCode::OK, "flamingo avatar must return 200");
    assert!(!body.is_empty(), "flamingo avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "flamingo avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn axolotl_avatar_200_image() {
    // testuser's avatar — axolotl for cross-backend recognition
    let (status, body, ct) = fetch_avatar("axolotl").await;
    assert_eq!(status, StatusCode::OK, "axolotl (testuser) avatar must return 200");
    assert!(!body.is_empty(), "axolotl avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "axolotl avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn unknown_avatar_returns_404() {
    let (status, _body, _ct) = fetch_avatar("nonexistent").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Unknown avatar must return 404"
    );
}
