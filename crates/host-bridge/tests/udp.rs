//! # Round-trip integration tests for the `/host/udp/*` UDP socket service.
//!
//! Spins up a real axum server on a loopback TcpListener and exercises
//! bind / connect / send / recv_stream / close end-to-end over real HTTP.
//!
//! Run with:
//!   cargo test -p poly-host-bridge --features udp --test udp

#![cfg(feature = "udp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use base64::Engine as _;
use futures::StreamExt as _;
use poly_host_bridge::udp::{UdpState, router};
use poly_host_bridge::udp_client::UdpClient;
use tokio::net::TcpListener;

// ── Test helpers ───────────────────────────────────────────────────────────────

/// Spawn an in-process axum server. Returns the base URL.
async fn spawn_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(UdpState::new());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

// ── Tests ──────────────────────────────────────────────────────────────────────

/// POST /host/udp/bind returns a non-empty session_id and a local_port > 0.
#[tokio::test(flavor = "multi_thread")]
async fn bind_returns_local_port() {
    let base = spawn_server().await;
    let client = UdpClient::new(&base);

    let resp = client.bind().await.expect("bind should succeed");
    assert!(!resp.session_id.is_empty(), "session_id must not be empty");
    assert!(resp.local_port > 0, "local_port must be non-zero");
}

/// Bind two sessions A and B on the loopback, connect A→B and B→A,
/// send a datagram from A, assert B's SSE stream receives it within 2s.
#[tokio::test(flavor = "multi_thread")]
async fn send_and_recv_loopback() {
    let base = spawn_server().await;
    let client = UdpClient::new(&base);

    // Bind session A and B.
    let a = client.bind().await.expect("bind A");
    let b = client.bind().await.expect("bind B");

    let a_addr = format!("127.0.0.1:{}", a.local_port);
    let b_addr = format!("127.0.0.1:{}", b.local_port);

    // Connect A to B and B to A so we can use send() without explicit dst.
    client.connect(&a.session_id, &b_addr).await.expect("connect A→B");
    client.connect(&b.session_id, &a_addr).await.expect("connect B→A");

    let payload = b"hello-udp-loopback";

    // Subscribe to B's recv stream before sending so we don't miss the datagram.
    let b_stream = client.recv_stream(&b.session_id);
    tokio::pin!(b_stream);

    // Send from A to B.
    client.send(&a.session_id, payload, None).await.expect("send A→B");

    // Wait up to 2s for the datagram to appear on B's stream.
    let received = tokio::time::timeout(Duration::from_secs(2), b_stream.next())
        .await
        .expect("timeout waiting for datagram on B")
        .expect("stream ended unexpectedly");

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(received.data.as_bytes())
        .expect("base64 decode");
    assert_eq!(decoded, payload, "received bytes must match sent bytes");
}

/// After close, a subsequent send returns a server error (session not found).
#[tokio::test(flavor = "multi_thread")]
async fn close_releases_socket() {
    let base = spawn_server().await;
    let client = UdpClient::new(&base);

    let resp = client.bind().await.expect("bind");
    let sid = resp.session_id.clone();

    // Close the session.
    client.close(&sid).await.expect("close should succeed");

    // Now send should fail because the session is gone.
    let send_err = client.send(&sid, b"dead", None).await;
    assert!(
        send_err.is_err(),
        "send after close should return an error, got: {send_err:?}"
    );
}
