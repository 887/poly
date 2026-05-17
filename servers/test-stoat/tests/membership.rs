//! Regression tests — GET /users/@me/servers returns SRV001 for STOAT01.
//!
//! Covers the bug where the handler returned [] for authenticated users
//! because the members filter was not matching the user_id returned by
//! session_user() against Server::members.

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

fn issue_token(state: &Arc<StoatState>, user_id: &str) -> String {
    state.auth.create_token(user_id)
}

async fn get_my_servers(router: axum::Router, token: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .uri("/users/@me/servers")
        .header("x-session-token", token)
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

/// STOAT01 must see SRV001 (The Burrow) in their server list.
#[tokio::test]
async fn stoat01_sees_srv001() {
    let state = Arc::new(StoatState::new());
    state.seed();
    let token = issue_token(&state, "STOAT01");
    let router = poly_test_stoat::router(state);
    let (status, body) = get_my_servers(router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().expect("expected JSON array from /users/@me/servers");
    assert!(
        !arr.is_empty(),
        "GET /users/@me/servers returned [] for STOAT01 — members filter is broken"
    );
    let ids: Vec<&str> = arr
        .iter()
        .filter_map(|s| s.get("_id").and_then(|v| v.as_str()))
        .collect();
    assert!(
        ids.contains(&"SRV001"),
        "SRV001 (The Burrow) not in STOAT01 server list; got: {ids:?}"
    );
}

/// STOAT01 sees both SRV001 and SRV002 — member of both in seed data.
#[tokio::test]
async fn stoat01_sees_both_servers() {
    let state = Arc::new(StoatState::new());
    state.seed();
    let token = issue_token(&state, "STOAT01");
    let router = poly_test_stoat::router(state);
    let (status, body) = get_my_servers(router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().expect("expected JSON array");
    let ids: Vec<&str> = arr
        .iter()
        .filter_map(|s| s.get("_id").and_then(|v| v.as_str()))
        .collect();
    // STOAT01 is in members for SRV001, SRV_ARENA, and SRV002
    for expected in ["SRV001", "SRV002"] {
        assert!(
            ids.contains(&expected),
            "{expected} not in STOAT01 server list; got: {ids:?}"
        );
    }
}

/// SRV001 (The Burrow) has the correct name.
#[tokio::test]
async fn srv001_is_the_burrow() {
    let state = Arc::new(StoatState::new());
    state.seed();
    let token = issue_token(&state, "STOAT01");
    let router = poly_test_stoat::router(state);
    let (status, body) = get_my_servers(router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().expect("expected JSON array");
    let srv001 = arr
        .iter()
        .find(|s| s.get("_id").and_then(|v| v.as_str()) == Some("SRV001"))
        .expect("SRV001 not found in response");
    assert_eq!(
        srv001.get("name").and_then(|v| v.as_str()),
        Some("The Burrow"),
        "SRV001 name mismatch"
    );
}

/// RACCOON01 sees their own servers (SRV001, SRV_ARENA, SRV002 — all three seed servers).
#[tokio::test]
async fn raccoon01_sees_servers() {
    let state = Arc::new(StoatState::new());
    state.seed();
    let token = issue_token(&state, "RACCOON01");
    let router = poly_test_stoat::router(state);
    let (status, body) = get_my_servers(router, &token).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().expect("expected JSON array");
    assert!(
        !arr.is_empty(),
        "GET /users/@me/servers returned [] for RACCOON01"
    );
}

/// An invalid token must receive 401 Unauthorized, not [].
#[tokio::test]
async fn invalid_token_returns_401() {
    let router = seeded_router();
    let (status, _) = get_my_servers(router, "not-a-real-token").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
