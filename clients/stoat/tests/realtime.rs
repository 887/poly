//! Integration tests for Stoat real-time event streaming via Bonfire WebSocket.
//!
//! Spins up the mock poly-test-stoat server, connects the Stoat client's
//! `event_stream()`, and verifies that REST-triggered events arrive over the WS.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;

use poly_stoat::StoatClient;
use poly_test_common::TestServerBase;
use poly_test_stoat::{StoatState, router};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Start a seeded test-stoat server. Returns (base_url, shutdown_sender).
async fn start_test_server() -> (String, tokio::sync::oneshot::Sender<()>) {
    let state = Arc::new(StoatState::new());
    state.seed();

    let base = TestServerBase::bind(0).await.expect("bind random port");
    let base_url = base.base_url();

    let app = router(Arc::clone(&state));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(base.listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("test-stoat serve");
    });

    // Give the server a moment to start accepting connections.
    tokio::time::sleep(Duration::from_millis(30)).await;

    (base_url, shutdown_tx)
}

/// Create an authenticated StoatClient pointed at base_url.
async fn authenticated_client(base_url: &str) -> StoatClient {
    let mut client = StoatClient::with_base_url(base_url).expect("valid base url");
    client
        .authenticate(AuthCredentials::EmailPassword {
            email: "stoat".into(),
            password: "testpass123".into(),
        })
        .await
        .expect("authenticate");
    client
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test 1: event_stream receives MessageReceived when a message is sent to a server channel.
#[tokio::test]
async fn test_event_stream_receives_message_in_server_channel() {
    let (base_url, _shutdown) = start_test_server().await;

    let client = authenticated_client(&base_url).await;
    let mut stream = client.event_stream();

    // Give the WS connection a moment to authenticate.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a message via REST to CH001
    let http_client = reqwest::Client::new();
    http_client
        .post(format!("{base_url}/channels/CH001/messages"))
        .header("x-session-token", get_token(&base_url).await)
        .json(&serde_json::json!({"content": "hello from test"}))
        .send()
        .await
        .expect("send message REST call")
        .error_for_status()
        .expect("REST send message success");

    // Wait for the event with a timeout
    let event = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .expect("timed out waiting for MessageReceived event")
        .expect("stream ended unexpectedly");

    match event {
        poly_client::ClientEvent::MessageReceived { channel_id, message } => {
            assert_eq!(channel_id, "CH001", "wrong channel_id");
            match message.content {
                poly_client::MessageContent::Text(text) => {
                    assert_eq!(text, "hello from test");
                }
                other => panic!("expected Text content, got: {other:?}"),
            }
        }
        other => panic!("expected MessageReceived, got: {other:?}"),
    }
}

/// Test 2: event_stream receives TypingStarted when the typing endpoint is called.
#[tokio::test]
async fn test_event_stream_receives_typing_started() {
    let (base_url, _shutdown) = start_test_server().await;

    let client = authenticated_client(&base_url).await;
    let mut stream = client.event_stream();

    // Give the WS connection a moment to authenticate.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // POST to /channels/CH001/typing
    let token = get_token(&base_url).await;
    let http_client = reqwest::Client::new();
    http_client
        .post(format!("{base_url}/channels/CH001/typing"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("typing POST")
        .error_for_status()
        .expect("typing POST success");

    // Wait for the event
    let event = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .expect("timed out waiting for TypingStarted event")
        .expect("stream ended unexpectedly");

    match event {
        poly_client::ClientEvent::TypingStarted { channel_id, .. } => {
            assert_eq!(channel_id, "CH001");
        }
        other => panic!("expected TypingStarted, got: {other:?}"),
    }
}

/// Test 3: event_stream receives MessageReceived for a DM channel.
#[tokio::test]
async fn test_event_stream_receives_message_in_dm_channel() {
    let (base_url, _shutdown) = start_test_server().await;

    let client = authenticated_client(&base_url).await;
    let mut stream = client.event_stream();

    // Give the WS connection a moment to authenticate.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a message to DM channel CHDM001
    let http_client = reqwest::Client::new();
    http_client
        .post(format!("{base_url}/channels/CHDM001/messages"))
        .header("x-session-token", get_token(&base_url).await)
        .json(&serde_json::json!({"content": "DM test message"}))
        .send()
        .await
        .expect("send DM message REST call")
        .error_for_status()
        .expect("REST send DM message success");

    let event = tokio::time::timeout(Duration::from_secs(3), stream.next())
        .await
        .expect("timed out waiting for MessageReceived event for DM")
        .expect("stream ended unexpectedly");

    match event {
        poly_client::ClientEvent::MessageReceived { channel_id, message } => {
            assert_eq!(channel_id, "CHDM001", "wrong channel_id for DM");
            match message.content {
                poly_client::MessageContent::Text(text) => {
                    assert_eq!(text, "DM test message");
                }
                other => panic!("expected Text content, got: {other:?}"),
            }
        }
        other => panic!("expected MessageReceived for DM, got: {other:?}"),
    }
}

/// Test 4: typing endpoint returns a success status without a WebSocket connection.
#[tokio::test]
async fn test_typing_endpoint_standalone() {
    let (base_url, _shutdown) = start_test_server().await;

    let token = get_token(&base_url).await;
    let http_client = reqwest::Client::new();
    let resp = http_client
        .post(format!("{base_url}/channels/CH001/typing"))
        .header("x-session-token", &token)
        .send()
        .await
        .expect("typing POST");

    // 204 No Content or any 2xx is acceptable
    assert!(
        resp.status().is_success() || resp.status().as_u16() == 204,
        "expected 2xx/204 from typing endpoint, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Get a session token for the stoat user via the test-only endpoint.
async fn get_token(base_url: &str) -> String {
    let http_client = reqwest::Client::new();
    let resp = http_client
        .post(format!("{base_url}/test/auth/token"))
        .json(&serde_json::json!({"username": "stoat"}))
        .send()
        .await
        .expect("test/auth/token request")
        .error_for_status()
        .expect("test/auth/token success");

    resp.json::<serde_json::Value>()
        .await
        .expect("parse token response")
        .get("token")
        .and_then(|t| t.as_str())
        .expect("token field")
        .to_string()
}
