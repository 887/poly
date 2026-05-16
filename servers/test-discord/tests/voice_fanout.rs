//! Phase X.1 — UDP fan-out integration test.
//!
//! Drives two concurrent voice-WS clients through the full Discord-style
//! handshake (gateway op-4 → voice WS HELLO/IDENTIFY/READY/SELECT_PROTOCOL/
//! SESSION_DESCRIPTION) and verifies that audio bytes sent by one client are
//! delivered to the other (and NOT echoed back to the sender).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use futures::{SinkExt, StreamExt};
use poly_test_discord::DiscordState;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, UdpSocket};
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

/// Start a test server bound to a random port. Mirrors the helper in
/// `tests/voice.rs` — kept a private copy because integration test crates
/// can't share helpers across files easily.
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

/// Drive one client end-to-end through the gateway op-4 + voice WS handshake,
/// bind a UDP socket on a random ephemeral port, register that port via
/// SELECT_PROTOCOL, and return `(udp_sock, udp_port_on_server, voice_ws,
/// gateway_ws)`. The voice_ws and gateway_ws are returned so the caller can
/// keep them alive — dropping them would deregister the session.
async fn establish_client(
    server_port: u16,
    user_token: &str,
    guild_id: &str,
    channel_id: &str,
) -> (
    UdpSocket,
    u16,
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) {
    // 1. Gateway WS — IDENTIFY + op 4 voice-state-update.
    let gw_url = format!("ws://127.0.0.1:{server_port}/gateway/ws");
    let (mut gw_ws, _) = connect_async(&gw_url).await.unwrap();
    // op 10 HELLO + op 0 READY
    gw_ws.next().await.unwrap().unwrap();
    gw_ws.next().await.unwrap().unwrap();

    let id = serde_json::json!({"op":2,"d":{"token":user_token,"intents":513,"properties":{}}});
    gw_ws.send(TungsteniteMessage::Text(id.to_string().into())).await.unwrap();

    let op4 = serde_json::json!({"op":4,"d":{"guild_id":guild_id,"channel_id":channel_id,"self_mute":false,"self_deaf":false}});
    gw_ws.send(TungsteniteMessage::Text(op4.to_string().into())).await.unwrap();

    // VOICE_STATE_UPDATE → grab session_id
    let vsu_txt = text_of(gw_ws.next().await.unwrap().unwrap());
    let vsu: serde_json::Value = serde_json::from_str(&vsu_txt).unwrap();
    assert_eq!(vsu["t"], "VOICE_STATE_UPDATE");
    let session_id = vsu["d"]["session_id"].as_str().unwrap().to_string();

    // VOICE_SERVER_UPDATE → grab endpoint
    let vsup_txt = text_of(gw_ws.next().await.unwrap().unwrap());
    let vsup: serde_json::Value = serde_json::from_str(&vsup_txt).unwrap();
    let endpoint = vsup["d"]["endpoint"].as_str().unwrap().to_string();

    // 2. Voice WS handshake.
    let voice_url = format!("ws://{endpoint}/voice/ws");
    let (mut voice_ws, _) = connect_async(&voice_url).await.unwrap();

    // HELLO
    let hello_txt = text_of(voice_ws.next().await.unwrap().unwrap());
    let hello: serde_json::Value = serde_json::from_str(&hello_txt).unwrap();
    assert_eq!(hello["op"], 8);

    // IDENTIFY — pass the session_id from the gateway so the voice WS can
    // resolve our channel_id via voice_session_channels.
    let identify = serde_json::json!({
        "op": 0,
        "d": {
            "server_id": guild_id,
            "user_id": "mock-user-x",
            "session_id": session_id,
            "token": "mock-voice-token"
        }
    });
    voice_ws.send(TungsteniteMessage::Text(identify.to_string().into())).await.unwrap();

    // READY → get the announced UDP port.
    let ready_txt = text_of(voice_ws.next().await.unwrap().unwrap());
    let ready: serde_json::Value = serde_json::from_str(&ready_txt).unwrap();
    assert_eq!(ready["op"], 2);
    let server_udp_port = u16::try_from(ready["d"]["port"].as_u64().unwrap()).unwrap();

    // 3. Bind our local UDP socket BEFORE sending SELECT_PROTOCOL so we can
    // advertise the real ephemeral port (the server matches recv_from source
    // against the registered peer_addr).
    let client_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let local_port = client_sock.local_addr().unwrap().port();
    client_sock
        .connect(format!("127.0.0.1:{server_udp_port}"))
        .await
        .unwrap();

    // SELECT_PROTOCOL
    let select = serde_json::json!({
        "op": 1,
        "d": {
            "protocol": "udp",
            "data": {
                "address": "127.0.0.1",
                "port": local_port,
                "mode": "aead_xchacha20_poly1305_rtpsize"
            }
        }
    });
    voice_ws.send(TungsteniteMessage::Text(select.to_string().into())).await.unwrap();

    // SESSION_DESCRIPTION
    let sd_txt = text_of(voice_ws.next().await.unwrap().unwrap());
    let sd: serde_json::Value = serde_json::from_str(&sd_txt).unwrap();
    assert_eq!(sd["op"], 4);

    (client_sock, server_udp_port, voice_ws, gw_ws)
}

#[tokio::test]
async fn test_cross_client_audio_fanout() {
    let (port, shutdown_tx, _state) = start_server().await;

    // Two clients join the SAME voice channel (204 in guild 100).
    let (sock_a, _udp_port, _vws_a, _gw_a) =
        establish_client(port, "token-A", "100", "204").await;
    let (sock_b, _udp_port_b, _vws_b, _gw_b) =
        establish_client(port, "token-B", "100", "204").await;

    // Tiny settle window so the SELECT_PROTOCOL inserts above are visible to
    // the fan-out loop (single-threaded test runtime — usually unnecessary
    // but cheap insurance against ordering surprises).
    tokio::time::sleep(Duration::from_millis(20)).await;

    // ---- A → B ----
    let payload_a = b"hello-from-A";
    sock_a.send(payload_a).await.unwrap();

    let mut buf = [0u8; 256];
    let n = tokio::time::timeout(Duration::from_millis(500), sock_b.recv(&mut buf))
        .await
        .expect("client B should receive A's packet within 500ms")
        .unwrap();
    assert_eq!(&buf[..n], payload_a, "B received wrong bytes from A");

    // A must NOT receive its own packet (no self-echo for registered senders).
    let self_echo = tokio::time::timeout(Duration::from_millis(100), sock_a.recv(&mut buf)).await;
    assert!(
        self_echo.is_err(),
        "A unexpectedly received its own packet back (got {} bytes)",
        self_echo.ok().and_then(Result::ok).unwrap_or(0)
    );

    // ---- B → A ----
    let payload_b = b"hello-from-B";
    sock_b.send(payload_b).await.unwrap();

    let n2 = tokio::time::timeout(Duration::from_millis(500), sock_a.recv(&mut buf))
        .await
        .expect("client A should receive B's packet within 500ms")
        .unwrap();
    assert_eq!(&buf[..n2], payload_b, "A received wrong bytes from B");

    // B must not self-echo either.
    let self_echo_b = tokio::time::timeout(Duration::from_millis(100), sock_b.recv(&mut buf)).await;
    assert!(
        self_echo_b.is_err(),
        "B unexpectedly received its own packet back"
    );

    let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ip_discovery_still_self_echoes() {
    // Sanity check: even with fan-out wired in, an IP-discovery packet
    // (first 2 bytes 0x00 0x01) from an unregistered sender must bounce
    // back to the sender — the real Discord behavior the bridge relies on
    // to learn its public IP/port.
    let (_port, shutdown_tx, state) = start_server().await;

    let udp_port = state
        .voice_udp_port
        .load(std::sync::atomic::Ordering::Relaxed);
    assert_ne!(udp_port, 0);

    let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sock.connect(format!("127.0.0.1:{udp_port}")).await.unwrap();

    let mut discovery = [0u8; 74];
    discovery[0] = 0x00;
    discovery[1] = 0x01;
    discovery[2] = 0x00;
    discovery[3] = 0x46;
    discovery[7] = 7;
    sock.send(&discovery).await.unwrap();

    let mut buf = [0u8; 128];
    let n = tokio::time::timeout(Duration::from_millis(500), sock.recv(&mut buf))
        .await
        .expect("IP discovery should still self-echo")
        .unwrap();
    assert_eq!(n, 74);
    assert_eq!(&buf[..8], &discovery[..8]);

    let _ = shutdown_tx.send(());
}
