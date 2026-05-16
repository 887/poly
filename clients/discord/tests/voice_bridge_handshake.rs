#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Phase X.0 F.7 — voice bridge handshake foundation tests.
//!
//! Verifies that the test-discord mock implements the Discord voice
//! gateway v8 handshake correctly and that the bridge's
//! `parse_session_description` helper extracts the secret_key from the
//! op 4 frame the mock emits.
//!
//! Full `DiscordVoiceBridgeClient::connect_voice` end-to-end coverage
//! requires a running `poly-host` daemon (for `/host/udp/*`,
//! `/host/codec/opus/*`, `/host/aead/*`) and is out of scope for this
//! foundation phase. The audio capture/playback agents (Phase X.2/X.3)
//! will add it once they have the host-bridge under integration test.
//!
//! Gated `cfg(all(not(target_arch = "wasm32"), feature = "voice-bridge",
//! feature = "gateway"))` so it only runs on native with the deps we need.

#![cfg(all(not(target_arch = "wasm32"), feature = "voice-bridge", feature = "gateway"))]

use std::sync::Arc;
use std::time::Duration;

use axum::serve;
use futures::{SinkExt, StreamExt};
use poly_discord::voice_bridge::voice_protocol::parse_session_description;
use poly_test_discord::{router, DiscordState};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// Spin up the test-discord mock on a random port + bind the voice UDP
/// echo socket (normally done in `BackendHarness::post_bind`).
async fn start_mock() -> (String, u16, tokio::sync::oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();

    let state = Arc::new(DiscordState::new());
    state.seed();
    // BackendHarness normally calls post_bind which binds the UDP echo
    // socket. Call it directly so /voice/ws can return a real UDP port.
    poly_test_common::BackendHarness::post_bind(
        &state,
        format!("127.0.0.1:{port}").parse().unwrap(),
    )
    .await;

    let app = router(Arc::clone(&state));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        serve(listener, app)
            .with_graceful_shutdown(async {
                rx.await.ok();
            })
            .await
            .ok();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (format!("ws://127.0.0.1:{port}/voice/ws"), port, tx)
}

#[tokio::test]
async fn handshake_emits_session_description_with_secret_key() {
    let (ws_url, _port, _shutdown) = start_mock().await;

    let (mut ws, _resp) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("connect_async");

    // op 8 HELLO
    let hello = next_text(&mut ws).await;
    let v: serde_json::Value = serde_json::from_str(&hello).unwrap();
    assert_eq!(v["op"], 8);

    // op 0 IDENTIFY
    ws.send(Message::Text(
        serde_json::json!({
            "op": 0,
            "d": {
                "server_id": "G001",
                "user_id": "U001",
                "session_id": "sess_abc",
                "token": "tok_abc"
            }
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();

    // op 2 READY
    let ready = next_text(&mut ws).await;
    let v: serde_json::Value = serde_json::from_str(&ready).unwrap();
    assert_eq!(v["op"], 2);
    assert_eq!(v["d"]["ssrc"], 1);
    assert!(v["d"]["modes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "aead_xchacha20_poly1305_rtpsize"));

    // op 1 SELECT_PROTOCOL
    ws.send(Message::Text(
        serde_json::json!({
            "op": 1,
            "d": {
                "protocol": "udp",
                "data": {
                    "address": "127.0.0.1",
                    "port": 12345_u16,
                    "mode": "aead_xchacha20_poly1305_rtpsize"
                }
            }
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();

    // op 4 SESSION_DESCRIPTION — the load-bearing frame this phase wires
    // up. parse_session_description must extract a 32-byte secret_key.
    let session_desc = next_text(&mut ws).await;
    let key = parse_session_description(&session_desc)
        .expect("parse op 4")
        .expect("op 4 is SESSION_DESCRIPTION");
    assert_eq!(key.len(), 32, "secret_key must be 32 bytes");
}

#[tokio::test]
async fn parse_session_description_skips_non_op4_frames() {
    // The bridge's finish_handshake loops past unrelated ops. Verify that
    // common post-handshake frames produce Ok(None), not Err.
    for op in [0u64, 2, 3, 5, 6, 8] {
        let frame = serde_json::json!({"op": op, "d": {}}).to_string();
        let result = parse_session_description(&frame).expect("parse ok");
        assert!(
            result.is_none(),
            "op {op} should yield Ok(None), got {result:?}"
        );
    }
}

async fn next_text<S>(
    ws: &mut tokio_tungstenite::WebSocketStream<S>,
) -> String
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("WS recv timed out")
            .expect("WS closed")
            .expect("WS frame")
        {
            Message::Text(t) => return t.to_string(),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected WS frame: {other:?}"),
        }
    }
}
