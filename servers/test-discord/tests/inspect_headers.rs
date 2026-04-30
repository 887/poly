//! Integration test for Phase E: `GET /test/inspect/last-headers` on test-discord.
//!
//! Boots the mock server, fires 3 requests with distinct User-Agent values,
//! then asserts the inspection endpoint returns entries matching those requests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use poly_test_discord::DiscordState;
use poly_test_common::HEADER_INSPECT_CAP;
use std::sync::Arc;
use tower::ServiceExt;

fn fresh_state() -> Arc<DiscordState> {
    Arc::new(DiscordState::new())
}

async fn get_json(
    state: &Arc<DiscordState>,
    path: &str,
    user_agent: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let router = poly_test_discord::router(Arc::clone(state));
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(ua) = user_agent {
        builder = builder.header("user-agent", ua);
    }
    let req = builder.body(Body::empty()).unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    (status, json)
}

// ---------------------------------------------------------------------------
// Basic: 3 requests → 3 entries, most-recent first
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inspect_last_headers_captures_requests() {
    let state = fresh_state();

    // Fire 3 requests with distinct user-agent values.
    let user_agents = ["TestAgent/1.0", "TestAgent/2.0", "TestAgent/3.0"];
    for ua in &user_agents {
        get_json(&state, "/health", Some(ua)).await;
    }

    // Fetch the inspection endpoint.
    let (status, json) = get_json(&state, "/test/inspect/last-headers", None).await;
    assert_eq!(status, StatusCode::OK);

    let entries = json.as_array().expect("response should be an array");

    // We sent 3 requests + this inspect request itself = 4 entries minimum.
    // The inspect request is the most-recent, so entries[0] is that one.
    // entries[1..4] correspond to the 3 user-agent requests in reverse order.
    assert!(entries.len() >= 4, "expected at least 4 entries, got {}", entries.len());

    // The most-recent 3 requests (after the inspect call itself) should contain
    // our user-agents in reverse order (most-recent first).
    let recent_uas: Vec<String> = entries
        .iter()
        .skip(1)   // skip the inspect request itself
        .take(3)
        .filter_map(|e| {
            e.get("headers")
                .and_then(|h| h.get("user-agent"))
                .and_then(|ua| ua.as_str())
                .map(str::to_string)
        })
        .collect();

    assert_eq!(recent_uas, vec!["TestAgent/3.0", "TestAgent/2.0", "TestAgent/1.0"]);
}

// ---------------------------------------------------------------------------
// method + path are captured
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inspect_captures_method_and_path() {
    let state = fresh_state();

    get_json(&state, "/health", Some("MethodPathBot/1.0")).await;

    let (_, json) = get_json(&state, "/test/inspect/last-headers", None).await;
    let entries = json.as_array().expect("array");

    // entries[1] is the /health request
    let health_entry = &entries[1];
    assert_eq!(health_entry["method"].as_str().unwrap(), "GET");
    assert_eq!(health_entry["path"].as_str().unwrap(), "/health");
    assert_eq!(
        health_entry["headers"]["user-agent"].as_str().unwrap(),
        "MethodPathBot/1.0"
    );
    // captured_at must be present
    assert!(health_entry["captured_at"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// Ring buffer cap: stays ≤ HEADER_INSPECT_CAP even after 200 requests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inspect_ring_buffer_cap() {
    let state = fresh_state();

    // Send 200 requests — well over the cap.
    for i in 0..200u32 {
        let router = poly_test_discord::router(Arc::clone(&state));
        let req = Request::builder()
            .method("GET")
            .uri("/health")
            .header("x-seq", i.to_string())
            .body(Body::empty())
            .unwrap();
        let _ = router.oneshot(req).await.unwrap();
    }

    let (status, json) = get_json(&state, "/test/inspect/last-headers", None).await;
    assert_eq!(status, StatusCode::OK);
    let entries = json.as_array().expect("array");
    assert!(
        entries.len() <= HEADER_INSPECT_CAP,
        "buffer exceeded cap: {} > {}",
        entries.len(),
        HEADER_INSPECT_CAP,
    );
}
