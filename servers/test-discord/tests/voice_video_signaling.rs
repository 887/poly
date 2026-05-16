//! Phase Y.1 — Mock voice WS op 12 STREAM_CREATE → op 21 Stream Subscription.
//!
//! Drives the full voice handshake, then sends op 12 to request a video stream
//! and asserts the server replies with op 21 bearing a fresh non-zero
//! `video_ssrc` distinct from the audio SSRC.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use futures::{SinkExt, StreamExt};
use poly_test_discord::DiscordState;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

async fn start_server() -> (u16, tokio::sync::oneshot::Sender<()>, Arc<DiscordState>) {
    let state = Arc::new(DiscordState::new());
    state.seed();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    *state.gateway_url.write().await = format!("ws://127.0.0.1:{port}/gateway/ws");
    *state.server_addr.write().await = format!("127.0.0.1:{port}");
    poly_test_discord::routes::bind_and_spawn_udp_echo(&state).await;

    let app = poly_test_discord::router(Arc::clone(&state));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
            .await
            .ok();
        drop(state_clone);
    });

    (port, shutdown_tx, state)
}

fn text_of(msg: TungsteniteMessage) -> String {
    match msg {
        TungsteniteMessage::Text(t) => t.to_string(),
        other => panic!("expected text frame, got {:?}", other),
    }
}

/// Drive a full handshake and then op 12 STREAM_CREATE → op 21.
#[tokio::test]
async fn op_12_stream_create_returns_op_21_with_video_ssrc() {
    let (port, shutdown_tx, state) = start_server().await;

    // Pass channel_id via query so the SELECT_PROTOCOL handler registers
    // the session without needing the gateway op-4 → voice_session_channels
    // seeding dance.
    let voice_url = format!("ws://127.0.0.1:{port}/voice/ws?channel_id=200");
    let (mut ws, _) = connect_async(&voice_url).await.expect("voice WS connect failed");

    // op 8 HELLO
    let _ = text_of(ws.next().await.unwrap().unwrap());

    // op 0 IDENTIFY
    let identify = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": "100",
            "user_id": "mock-user-1",
            "session_id": "video-sess-abc",
            "token": "mock-voice-token"
        }
    });
    ws.send(TungsteniteMessage::Text(identify.to_string().into())).await.unwrap();

    // op 2 READY — capture the audio SSRC
    let ready_txt = text_of(ws.next().await.unwrap().unwrap());
    let ready: serde_json::Value = serde_json::from_str(&ready_txt).unwrap();
    assert_eq!(ready["op"], 2);
    let audio_ssrc = ready["d"]["ssrc"].as_u64().unwrap() as u32;
    assert!(audio_ssrc > 0);

    // op 1 SELECT_PROTOCOL
    let select = serde_json::json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": { "address": "127.0.0.1", "port": 54321, "mode": "aead_xchacha20_poly1305_rtpsize" }
        }
    });
    ws.send(TungsteniteMessage::Text(select.to_string().into())).await.unwrap();

    // op 4 SESSION_DESCRIPTION
    let _ = text_of(ws.next().await.unwrap().unwrap());

    // op 12 STREAM_CREATE — request video
    let stream_create = serde_json::json!({
        "op": 12,
        "d": { "type": "video", "rid": "high", "quality": 100 }
    });
    ws.send(TungsteniteMessage::Text(stream_create.to_string().into())).await.unwrap();

    // Expect op 21 Stream Subscription back.
    let sub_txt = text_of(ws.next().await.unwrap().unwrap());
    let sub: serde_json::Value = serde_json::from_str(&sub_txt).unwrap();
    assert_eq!(sub["op"], 21, "expected op 21 Stream Subscription, got {:?}", sub);
    let d = &sub["d"];
    assert_eq!(d["type"], "video");
    assert_eq!(d["rid"], "high");
    assert_eq!(d["quality"], 100);
    assert_eq!(d["audio_ssrc"].as_u64().unwrap() as u32, audio_ssrc);
    let video_ssrc = d["video_ssrc"].as_u64().unwrap() as u32;
    assert!(video_ssrc > 0, "video_ssrc should be non-zero");
    assert_ne!(video_ssrc, audio_ssrc, "video_ssrc must differ from audio_ssrc");

    // The server-side session record should also reflect the video SSRC.
    {
        let sessions = state.voice_sessions.read().await;
        let sess = sessions.get(&audio_ssrc).expect("audio session should be registered");
        assert_eq!(sess.video_ssrc, Some(video_ssrc));
    }

    let _ = shutdown_tx.send(());
}
