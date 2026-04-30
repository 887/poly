//! Integration tests — D.1: avatar serving for test-lemmy.
//!
//! Boots the mock Lemmy server in-process (seed() called inside router()),
//! then fetches each seeded user's avatar via the pict-rs-style image route
//! and asserts HTTP 200 + nonzero body + Content-Type starting with "image/".
//!
//! Avatar route: GET /pictrs/image/{filename}
//! Seed users and their avatar filenames:
//!   testuser  → axolotl.svg
//!   beaver    → beaver.svg
//!   hedgehog  → hedgehog.svg

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn seeded_router() -> axum::Router {
    // poly_test_lemmy::router() creates a fresh state and calls seed() internally.
    poly_test_lemmy::router()
}

/// Fetch a pict-rs-style image by filename (with extension).
async fn fetch_pictrs(filename: &str) -> (StatusCode, Vec<u8>, String) {
    let path = format!("/pictrs/image/{filename}");
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
async fn axolotl_svg_200_image() {
    // testuser's avatar
    let (status, body, ct) = fetch_pictrs("axolotl.svg").await;
    assert_eq!(status, StatusCode::OK, "axolotl.svg must return 200");
    assert!(!body.is_empty(), "axolotl.svg body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "axolotl.svg Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn beaver_svg_200_image() {
    let (status, body, ct) = fetch_pictrs("beaver.svg").await;
    assert_eq!(status, StatusCode::OK, "beaver.svg must return 200");
    assert!(!body.is_empty(), "beaver.svg body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "beaver.svg Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn hedgehog_svg_200_image() {
    let (status, body, ct) = fetch_pictrs("hedgehog.svg").await;
    assert_eq!(status, StatusCode::OK, "hedgehog.svg must return 200");
    assert!(!body.is_empty(), "hedgehog.svg body must be nonempty");
    assert!(
        ct.starts_with("image/"),
        "hedgehog.svg Content-Type must start with image/, got {ct}"
    );
}

#[tokio::test]
async fn unknown_image_returns_404() {
    let (status, _body, _ct) = fetch_pictrs("nonexistent.png").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Unknown pict-rs image must return 404"
    );
}
