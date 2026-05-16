//! Integration tests for Phase A: mock Discord voice signaling + voice WS + UDP echo.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use futures::{SinkExt, StreamExt};
use poly_test_discord::DiscordState;
use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket};
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

/// Start a test server bound to a random port. Returns `(port, shutdown_tx, state)`.
async fn start_server() -> (u16, tokio::sync::oneshot::Sender<()>, Arc<DiscordState>) {
    let state = Arc::new(DiscordState::new());
    state.seed();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Simulate post_bind to set gateway URL, server_addr, and spawn UDP echo socket.
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
        drop(state_clone); // keep state alive until server shuts down
    });

    (port, shutdown_tx, state)
}

fn text_of(msg: TungsteniteMessage) -> String {
    match msg {
        TungsteniteMessage::Text(t) => t.to_string(),
        other => panic!("expected text frame, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test A.1: op-4 handler — gateway sends VOICE_STATE_UPDATE + VOICE_SERVER_UPDATE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_op4_voice_state_and_server_update() {
    let (port, shutdown_tx, state) = start_server().await;

    let gw_url = format!("ws://127.0.0.1:{port}/gateway/ws");
    let (mut ws, _) = connect_async(&gw_url).await.expect("gateway WS connect failed");

    // Receive the initial READY dispatch.
    let ready_txt = text_of(ws.next().await.unwrap().unwrap());
    let ready: serde_json::Value = serde_json::from_str(&ready_txt).unwrap();
    assert_eq!(ready["op"], 0);
    assert_eq!(ready["t"], "READY");

    // Send op 2 IDENTIFY (extract user id from token; empty token → "mock-user-1").
    let identify = serde_json::json!({
        "op": 2,
        "d": { "token": "test-token", "intents": 513, "properties": {} }
    });
    ws.send(TungsteniteMessage::Text(identify.to_string().into())).await.unwrap();

    // Send op 4 Voice State Update — join a voice channel.
    let op4 = serde_json::json!({
        "op": 4,
        "d": { "guild_id": "100", "channel_id": "200", "self_mute": false, "self_deaf": false }
    });
    ws.send(TungsteniteMessage::Text(op4.to_string().into())).await.unwrap();

    // Expect VOICE_STATE_UPDATE.
    let vsu_txt = text_of(ws.next().await.unwrap().unwrap());
    let vsu: serde_json::Value = serde_json::from_str(&vsu_txt).unwrap();
    assert_eq!(vsu["op"], 0);
    assert_eq!(vsu["t"], "VOICE_STATE_UPDATE");
    let d = &vsu["d"];
    assert_eq!(d["guild_id"], "100");
    assert_eq!(d["channel_id"], "200");
    let session_id = d["session_id"].as_str().unwrap();
    assert!(session_id.starts_with("mock-voice-session-"), "got: {session_id}");

    // Expect VOICE_SERVER_UPDATE.
    let vsup_txt = text_of(ws.next().await.unwrap().unwrap());
    let vsup: serde_json::Value = serde_json::from_str(&vsup_txt).unwrap();
    assert_eq!(vsup["op"], 0);
    assert_eq!(vsup["t"], "VOICE_SERVER_UPDATE");
    let d2 = &vsup["d"];
    assert_eq!(d2["guild_id"], "100");
    assert_eq!(d2["token"], "mock-voice-token");
    let endpoint = d2["endpoint"].as_str().unwrap();
    assert!(
        endpoint.starts_with("127.0.0.1:"),
        "endpoint should be host:port, got: {endpoint}"
    );

    // The endpoint should match the HTTP server address (host:http_port).
    let expected_addr = state.server_addr.read().await.clone();
    assert_eq!(endpoint, expected_addr, "endpoint should match server_addr");

    let _ = shutdown_tx.send(());
}

// ---------------------------------------------------------------------------
// Test A.2: voice WS handshake — HELLO → IDENTIFY → READY → SELECT_PROTOCOL → SESSION_DESCRIPTION
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_voice_ws_full_handshake() {
    let (port, shutdown_tx, _state) = start_server().await;

    let voice_url = format!("ws://127.0.0.1:{port}/voice/ws");
    let (mut ws, _) = connect_async(&voice_url).await.expect("voice WS connect failed");

    // Step 1: receive op 8 HELLO.
    let hello_txt = text_of(ws.next().await.unwrap().unwrap());
    let hello: serde_json::Value = serde_json::from_str(&hello_txt).unwrap();
    assert_eq!(hello["op"], 8, "expected HELLO op=8, got {:?}", hello);
    let hb_interval = hello["d"]["heartbeat_interval"].as_u64().unwrap();
    assert_eq!(hb_interval, 13750);

    // Step 2: send op 0 IDENTIFY.
    let identify = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": "100",
            "user_id": "mock-user-1",
            "session_id": "mock-voice-session-abc123",
            "token": "mock-voice-token"
        }
    });
    ws.send(TungsteniteMessage::Text(identify.to_string().into())).await.unwrap();

    // Step 3: receive op 2 READY.
    let ready_txt = text_of(ws.next().await.unwrap().unwrap());
    let ready: serde_json::Value = serde_json::from_str(&ready_txt).unwrap();
    assert_eq!(ready["op"], 2, "expected READY op=2, got {:?}", ready);
    let d = &ready["d"];
    assert_eq!(d["ssrc"], 1);
    assert_eq!(d["ip"], "127.0.0.1");
    let udp_port = d["port"].as_u64().unwrap();
    assert_ne!(udp_port, 0, "UDP port in READY should be non-zero");
    let modes = d["modes"].as_array().unwrap();
    assert!(modes.iter().any(|m| m == "aead_xchacha20_poly1305_rtpsize"));

    // Step 4: send op 1 SELECT_PROTOCOL.
    let select = serde_json::json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": { "address": "127.0.0.1", "port": 54321, "mode": "aead_xchacha20_poly1305_rtpsize" }
        }
    });
    ws.send(TungsteniteMessage::Text(select.to_string().into())).await.unwrap();

    // Step 5: receive op 4 SESSION_DESCRIPTION.
    let sd_txt = text_of(ws.next().await.unwrap().unwrap());
    let sd: serde_json::Value = serde_json::from_str(&sd_txt).unwrap();
    assert_eq!(sd["op"], 4, "expected SESSION_DESCRIPTION op=4, got {:?}", sd);
    let mode = sd["d"]["mode"].as_str().unwrap();
    assert_eq!(mode, "aead_xchacha20_poly1305_rtpsize");
    let secret_key = sd["d"]["secret_key"].as_array().unwrap();
    assert_eq!(secret_key.len(), 32, "secret_key should be 32 bytes");
    assert!(secret_key.iter().all(|b| b == &serde_json::Value::Number(0.into())));

    let _ = shutdown_tx.send(());
}

// ---------------------------------------------------------------------------
// Test A.2 (heartbeat): voice WS heartbeat exchange
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_voice_ws_heartbeat() {
    let (port, shutdown_tx, _state) = start_server().await;

    let voice_url = format!("ws://127.0.0.1:{port}/voice/ws");
    let (mut ws, _) = connect_async(&voice_url).await.expect("voice WS connect failed");

    // Consume HELLO.
    ws.next().await.unwrap().unwrap();

    // Send HEARTBEAT (op 3) before IDENTIFY — server should reply ACK.
    let hb = serde_json::json!({ "op": 3, "d": 12345 });
    ws.send(TungsteniteMessage::Text(hb.to_string().into())).await.unwrap();

    let ack_txt = text_of(ws.next().await.unwrap().unwrap());
    let ack: serde_json::Value = serde_json::from_str(&ack_txt).unwrap();
    assert_eq!(ack["op"], 6, "expected HEARTBEAT_ACK op=6");
    assert_eq!(ack["d"], 12345);

    let _ = shutdown_tx.send(());
}

// ---------------------------------------------------------------------------
// Test A.3: UDP echo socket — IP discovery packet echoed back
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_udp_echo_ip_discovery() {
    let (_port, shutdown_tx, state) = start_server().await;

    let udp_port = state.voice_udp_port.load(std::sync::atomic::Ordering::Relaxed);
    assert_ne!(udp_port, 0, "UDP echo port must be bound before test");

    // Bind a local UDP socket.
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_sock.connect(format!("127.0.0.1:{udp_port}")).await.unwrap();

    // Send a Discord-style IP discovery packet (74 bytes, type=0x0001, ssrc=42).
    let mut discovery = [0u8; 74];
    discovery[0] = 0x00;
    discovery[1] = 0x01;
    discovery[2] = 0x00;
    discovery[3] = 0x46;
    // SSRC = 42
    discovery[4] = 0;
    discovery[5] = 0;
    discovery[6] = 0;
    discovery[7] = 42;

    client_sock.send(&discovery).await.unwrap();

    let mut buf = [0u8; 256];
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client_sock.recv(&mut buf),
    )
    .await
    .expect("UDP echo timed out")
    .unwrap();

    assert_eq!(n, 74, "echo should return all 74 bytes");
    // The mock echoes verbatim, so the first 8 bytes match.
    assert_eq!(&buf[..8], &discovery[..8]);

    let _ = shutdown_tx.send(());
}

// ---------------------------------------------------------------------------
// Test A (integrated): gateway op-4 → voice WS handshake → UDP echo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_full_voice_pipeline() {
    let (port, shutdown_tx, _state) = start_server().await;

    // 1. Open gateway WS, send op 4, get VOICE_SERVER_UPDATE endpoint.
    let gw_url = format!("ws://127.0.0.1:{port}/gateway/ws");
    let (mut gw_ws, _) = connect_async(&gw_url).await.unwrap();
    // Consume READY.
    gw_ws.next().await.unwrap().unwrap();
    // Send IDENTIFY.
    let id = serde_json::json!({"op":2,"d":{"token":"t","intents":513,"properties":{}}});
    gw_ws.send(TungsteniteMessage::Text(id.to_string().into())).await.unwrap();
    // Send op 4.
    let op4 = serde_json::json!({"op":4,"d":{"guild_id":"100","channel_id":"200","self_mute":false,"self_deaf":false}});
    gw_ws.send(TungsteniteMessage::Text(op4.to_string().into())).await.unwrap();
    // Receive VOICE_STATE_UPDATE.
    let _vsu = gw_ws.next().await.unwrap().unwrap();
    // Receive VOICE_SERVER_UPDATE.
    let vsup_txt = text_of(gw_ws.next().await.unwrap().unwrap());
    let vsup: serde_json::Value = serde_json::from_str(&vsup_txt).unwrap();
    let endpoint = vsup["d"]["endpoint"].as_str().unwrap().to_string();
    // endpoint is "127.0.0.1:<port>"

    // 2. Connect voice WS using the endpoint.
    let voice_url = format!("ws://{endpoint}/voice/ws");
    let (mut voice_ws, _) = connect_async(&voice_url).await.expect("voice WS connect via gateway endpoint failed");

    // Receive HELLO.
    let hello_txt = text_of(voice_ws.next().await.unwrap().unwrap());
    let hello: serde_json::Value = serde_json::from_str(&hello_txt).unwrap();
    assert_eq!(hello["op"], 8);

    // Send IDENTIFY.
    let vid = serde_json::json!({"op":0,"d":{"server_id":"100","user_id":"mock-user-1","session_id":"sess-x","token":"mock-voice-token"}});
    voice_ws.send(TungsteniteMessage::Text(vid.to_string().into())).await.unwrap();

    // Receive READY.
    let ready_txt = text_of(voice_ws.next().await.unwrap().unwrap());
    let ready: serde_json::Value = serde_json::from_str(&ready_txt).unwrap();
    assert_eq!(ready["op"], 2);
    let udp_port = ready["d"]["port"].as_u64().unwrap() as u16;

    // Send SELECT_PROTOCOL.
    let sel = serde_json::json!({"op":1,"d":{"protocol":"udp","data":{"address":"127.0.0.1","port":12345,"mode":"aead_xchacha20_poly1305_rtpsize"}}});
    voice_ws.send(TungsteniteMessage::Text(sel.to_string().into())).await.unwrap();

    // Receive SESSION_DESCRIPTION.
    let sd_txt = text_of(voice_ws.next().await.unwrap().unwrap());
    let sd: serde_json::Value = serde_json::from_str(&sd_txt).unwrap();
    assert_eq!(sd["op"], 4);

    // 3. Verify UDP echo works on the port from READY.
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_sock.connect(format!("127.0.0.1:{udp_port}")).await.unwrap();
    let probe = b"hello-voice-echo";
    client_sock.send(probe).await.unwrap();
    let mut buf = [0u8; 64];
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        client_sock.recv(&mut buf),
    )
    .await
    .expect("UDP echo timed out in full pipeline test")
    .unwrap();
    assert_eq!(&buf[..n], probe);

    let _ = shutdown_tx.send(());
}
