//! Integration tests — D.1: avatar serving for test-matrix.
//!
//! Boots the mock Matrix homeserver in-process, calls seed(), then fetches
//! each seeded user's avatar URL and asserts HTTP 200 + nonzero body +
//! Content-Type starting with "image/".
//!
//! Avatar URL format:  mxc://localhost/{name}_avatar
//! Server route:       GET /_matrix/media/v3/thumbnail/{server}/{mediaId}

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use poly_test_matrix::MatrixState;
use std::sync::Arc;
use tower::ServiceExt;

fn seeded_router() -> axum::Router {
    let state = Arc::new(MatrixState::new());
    state.seed();
    poly_test_matrix::router(state)
}

/// Fetch an avatar thumbnail via the Matrix media route.
///
/// `media_id` is the last segment of the mxc:// URI (e.g. "owl_avatar").
async fn fetch_avatar(media_id: &str) -> (StatusCode, Vec<u8>, String) {
    let path = format!("/_matrix/media/v3/thumbnail/localhost/{media_id}");
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
async fn owl_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("owl_avatar").await;
    assert_eq!(status, StatusCode::OK, "owl avatar must return 200");
    assert!(!body.is_empty(), "owl avatar body must be nonempty");
    assert!(ct.starts_with("image/"), "owl avatar Content-Type must start with image/, got {ct}");
}

#[tokio::test]
async fn axolotl_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("axolotl_avatar").await;
    assert_eq!(status, StatusCode::OK, "axolotl avatar must return 200");
    assert!(!body.is_empty(), "axolotl avatar body must be nonempty");
    assert!(ct.starts_with("image/"), "axolotl avatar Content-Type must start with image/, got {ct}");
}

#[tokio::test]
async fn cat_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("cat_avatar").await;
    assert_eq!(status, StatusCode::OK, "cat avatar must return 200");
    assert!(!body.is_empty(), "cat avatar body must be nonempty");
    assert!(ct.starts_with("image/"), "cat avatar Content-Type must start with image/, got {ct}");
}

#[tokio::test]
async fn dog_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("dog_avatar").await;
    assert_eq!(status, StatusCode::OK, "dog avatar must return 200");
    assert!(!body.is_empty(), "dog avatar body must be nonempty");
    assert!(ct.starts_with("image/"), "dog avatar Content-Type must start with image/, got {ct}");
}

#[tokio::test]
async fn unknown_avatar_returns_404() {
    let (status, _body, _ct) = fetch_avatar("nonexistent_avatar").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown avatar must return 404");
}
