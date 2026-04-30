//! Integration tests — D.1: avatar serving for test-stoat.
//!
//! Boots the mock Stoat/Revolt API server in-process, calls seed(), then fetches
//! each seeded user's avatar via the Stoat avatar route and asserts
//! HTTP 200 + nonzero body + Content-Type starting with "image/".
//!
//! Avatar route: GET /avatars/{id}
//! The route maps "av_STOAT01" → "stoat", "av_RACCOON01" → "raccoon", etc.
//! Seed users:   STOAT01, RACCOON01, LEMMING01

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use poly_test_stoat::StoatState;
use std::sync::Arc;
use tower::ServiceExt;

fn seeded_router() -> axum::Router {
    let state = Arc::new(StoatState::new());
    state.seed();
    poly_test_stoat::router(state)
}

/// Fetch an avatar using the Revolt-style avatar ID (e.g. "av_STOAT01").
async fn fetch_avatar(avatar_id: &str) -> (StatusCode, Vec<u8>, String) {
    let path = format!("/avatars/{avatar_id}");
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
async fn stoat_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("av_STOAT01").await;
    assert_eq!(status, StatusCode::OK, "Stoat avatar must return 200");
    assert!(!body.is_empty(), "Stoat avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "Stoat avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn raccoon_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("av_RACCOON01").await;
    assert_eq!(status, StatusCode::OK, "Raccoon avatar must return 200");
    assert!(!body.is_empty(), "Raccoon avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "Raccoon avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn lemming_avatar_200_image() {
    let (status, body, ct) = fetch_avatar("av_LEMMING01").await;
    assert_eq!(status, StatusCode::OK, "Lemming avatar must return 200");
    assert!(!body.is_empty(), "Lemming avatar body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "Lemming avatar Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn unknown_avatar_returns_404() {
    let (status, _body, _ct) = fetch_avatar("av_UNKNOWN01").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Unknown avatar must return 404"
    );
}
