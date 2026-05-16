//! Integration tests for Phase 6.5: mock Gateway WebSocket + testhook.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use poly_test_discord::DiscordState;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceExt;

fn seeded_state() -> Arc<DiscordState> {
    let state = Arc::new(DiscordState::new());
    state.seed();
    state
}

async fn post_json(
    state: &Arc<DiscordState>,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let router = poly_test_discord::router(Arc::clone(state));
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    (status, json)
}

// ---------------------------------------------------------------------------
// Gateway URL reflects configured gateway_url
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_gateway_url_route() {
    let state = seeded_state();
    // Set a custom gateway URL.
    *state.gateway_url.write().await = "ws://127.0.0.1:9999/gateway/ws".to_string();

    let router = poly_test_discord::router(Arc::clone(&state));
    let req = Request::builder()
        .uri("/api/v10/gateway")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["url"], "ws://127.0.0.1:9999/gateway/ws");
}

// ---------------------------------------------------------------------------
// Testhook: emit_thread_event returns ok:true
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_testhook_thread_create_ok() {
    let state = seeded_state();
    let thread_payload = serde_json::json!({
        "id": "9001",
        "name": "Test thread",
        "type": 11,
        "guild_id": "100",
        "parent_id": "500",
        "thread_metadata": {
            "archived": false,
            "locked": false,
            "auto_archive_duration": 1440,
            "archive_timestamp": null,
            "create_timestamp": "2026-04-19T00:00:00.000Z"
        },
        "applied_tags": []
    });

    let (status, json) = post_json(
        &state,
        "/testhook/emit_thread_event",
        serde_json::json!({ "event_type": "THREAD_CREATE", "thread": thread_payload }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

#[tokio::test]
async fn test_testhook_thread_delete_ok() {
    let state = seeded_state();
    let (status, json) = post_json(
        &state,
        "/testhook/emit_thread_event",
        serde_json::json!({
            "event_type": "THREAD_DELETE",
            "thread_id": "9002",
            "guild_id": "100",
            "parent_id": "500"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], serde_json::Value::Bool(true));
}

#[tokio::test]
async fn test_testhook_unknown_event_type_returns_bad_request() {
    let state = seeded_state();
    let (status, _) = post_json(
        &state,
        "/testhook/emit_thread_event",
        serde_json::json!({ "event_type": "NOT_A_REAL_EVENT" }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// WS gateway route: upgrades connection (real WS test over TCP)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ws_gateway_route_exists_and_sends_ready() {
    use tokio_tungstenite::connect_async;
    use futures::StreamExt;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let state = seeded_state();
    *state.gateway_url.write().await = format!("ws://127.0.0.1:{port}/gateway/ws");

    let app = poly_test_discord::router(Arc::clone(&state));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .ok();
    });

    // Connect as a WS client.
    let ws_url = format!("ws://127.0.0.1:{port}/gateway/ws");
    let (mut ws, _) = connect_async(ws_url).await.expect("WS connect failed");

    // First message should be op 10 HELLO (the mock now mirrors real Discord
    // protocol: HELLO precedes READY so the wasm gateway-bridge handshake
    // succeeds — see routes.rs::handle_gateway_socket).
    let msg = ws.next().await.expect("no message").expect("WS error");
    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("expected text frame, got {:?}", other),
    };
    let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(frame["op"], 10);
    assert!(frame["d"]["heartbeat_interval"].as_u64().is_some());

    // Second message should be READY (op 0, t "READY").
    let msg = ws.next().await.expect("no second message").expect("WS error");
    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("expected text frame, got {:?}", other),
    };
    let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(frame["op"], 0);
    assert_eq!(frame["t"], "READY");

    let _ = shutdown_tx.send(());
}
