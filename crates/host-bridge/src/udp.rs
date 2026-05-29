//! # `/host/udp/*` — generic UDP socket service
//!
//! Exposes raw UDP bind/connect/send/recv over HTTP so browser WASM (which
//! cannot open UDP sockets) can drive UDP protocols through the native
//! server-half. Sessions are keyed by an opaque `session_id` string.
//!
//! ## Routes
//!
//! ```text
//! POST /host/udp/bind              -> { session_id, local_port }
//! POST /host/udp/connect           { session_id, peer_addr }
//! POST /host/udp/send              { session_id, data: base64, dst?: addr } -> { bytes_sent }
//! GET  /host/udp/recv_stream/:id   -> SSE stream of { data: base64, src_addr }
//! POST /host/udp/close             { session_id }
//! ```
//!
//! ## WASM safety
//!
//! This module is `#[cfg(all(not(target_arch = "wasm32"), feature = "udp"))]`.
//! WASM callers use [`crate::udp_client::UdpClient`] instead.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
};
use base64::Engine as _;
use futures::Stream;
use tokio::net::UdpSocket;
use uuid::Uuid;

// Wire types and route constants are defined in udp_client (always compiled,
// including on wasm32). Re-export from here for convenience.
pub use crate::udp_client::{
    UdpBindResponse, UdpCloseRequest, UdpCloseResponse, UdpConnectRequest, UdpConnectResponse,
    UdpDatagram, UdpSendRequest, UdpSendResponse, ROUTE_UDP_BIND, ROUTE_UDP_CLOSE,
    ROUTE_UDP_CONNECT, ROUTE_UDP_RECV_STREAM_PATTERN, ROUTE_UDP_SEND,
};

// ── Session state ──────────────────────────────────────────────────────────────

/// Shared state for the UDP service.
#[derive(Clone, Default)]
pub struct UdpState {
    sessions: Arc<Mutex<HashMap<String, UdpSessionEntry>>>,
}

struct UdpSessionEntry {
    socket: Arc<UdpSocket>,
}

impl UdpState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ── Router ─────────────────────────────────────────────────────────────────────

pub fn router(state: UdpState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(ROUTE_UDP_BIND, post(handle_bind))
        .route(ROUTE_UDP_CONNECT, post(handle_connect))
        .route(ROUTE_UDP_SEND, post(handle_send))
        .route(ROUTE_UDP_RECV_STREAM_PATTERN, get(handle_recv_stream))
        .route(ROUTE_UDP_CLOSE, post(handle_close))
        .with_state(state)
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn handle_bind(State(state): State<UdpState>) -> impl IntoResponse {
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(UdpBindResponse {
                    ok: false,
                    session_id: String::new(),
                    local_port: 0,
                    err: Some(format!("UDP bind: {e}")),
                }),
            );
        }
    };

    let local_port = match socket.local_addr() {
        Ok(a) => a.port(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(UdpBindResponse {
                    ok: false,
                    session_id: String::new(),
                    local_port: 0,
                    err: Some(format!("local_addr: {e}")),
                }),
            );
        }
    };

    let session_id = Uuid::new_v4().to_string();

    {
        let mut map = match state.sessions.lock() {
            Ok(m) => m,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(UdpBindResponse {
                        ok: false,
                        session_id: String::new(),
                        local_port: 0,
                        err: Some(format!("sessions lock poisoned: {e}")),
                    }),
                );
            }
        };
        map.insert(session_id.clone(), UdpSessionEntry { socket });
    }

    (
        StatusCode::OK,
        Json(UdpBindResponse { ok: true, session_id, local_port, err: None }),
    )
}

async fn handle_connect(
    State(state): State<UdpState>,
    Json(req): Json<UdpConnectRequest>,
) -> impl IntoResponse {
    let Some(socket) = get_socket(&state, &req.session_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(UdpConnectResponse {
                ok: false,
                err: Some(format!("session {} not found", req.session_id)),
            }),
        );
    };

    let peer: std::net::SocketAddr = match req.peer_addr.parse() {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(UdpConnectResponse {
                    ok: false,
                    err: Some(format!("invalid peer_addr: {e}")),
                }),
            );
        }
    };

    match socket.connect(peer).await {
        Ok(()) => (StatusCode::OK, Json(UdpConnectResponse { ok: true, err: None })),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UdpConnectResponse {
                ok: false,
                err: Some(format!("UDP connect: {e}")),
            }),
        ),
    }
}

async fn handle_send(
    State(state): State<UdpState>,
    Json(req): Json<UdpSendRequest>,
) -> impl IntoResponse {
    let Some(socket) = get_socket(&state, &req.session_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(UdpSendResponse {
                ok: false,
                bytes_sent: 0,
                err: Some(format!("session {} not found", req.session_id)),
            }),
        );
    };

    let data = match base64::engine::general_purpose::STANDARD.decode(req.data.as_bytes()) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(UdpSendResponse {
                    ok: false,
                    bytes_sent: 0,
                    err: Some(format!("invalid base64 data: {e}")),
                }),
            );
        }
    };

    let result = if let Some(dst) = req.dst {
        let addr: std::net::SocketAddr = match dst.parse() {
            Ok(a) => a,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(UdpSendResponse {
                        ok: false,
                        bytes_sent: 0,
                        err: Some(format!("invalid dst addr: {e}")),
                    }),
                );
            }
        };
        socket.send_to(&data, addr).await
    } else {
        socket.send(&data).await
    };

    match result {
        Ok(n) => (StatusCode::OK, Json(UdpSendResponse { ok: true, bytes_sent: n, err: None })),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UdpSendResponse {
                ok: false,
                bytes_sent: 0,
                err: Some(format!("UDP send: {e}")),
            }),
        ),
    }
}

async fn handle_recv_stream(
    State(state): State<UdpState>,
    AxumPath(id): AxumPath<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;
    use futures::stream::BoxStream;

    fn sse_response(
        stream: BoxStream<'static, Result<Event, std::convert::Infallible>>,
    ) -> axum::response::Response {
        Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
    }

    // Extract the dgram_tx and rebuild a fresh Receiver by sending through the
    // same channel. Since we can't clone Receivers, we create a new channel
    // pair and splice from the existing session's socket directly.
    // Actually, each SSE client needs a fresh mpsc::Receiver. We do this by
    // storing only the dgram_tx in the session map and letting the recv task
    // fan-out via clone. But mpsc is single-consumer. So we subscribe via a
    // broadcast channel instead. For simplicity, we swap the session's dgram_tx
    // for a new one and spawn a bridging task from the socket.
    //
    // Simpler approach: store the socket in the session and let the SSE handler
    // spawn its own recv loop that drains into the SSE stream. Only one SSE
    // subscriber is expected per session (the plugin's SSE connection).

    let Some(socket) = get_socket(&state, &id) else {
        use futures::stream;
        let once_stream = stream::once(async move {
            let json = serde_json::json!({ "err": "session not found" }).to_string();
            Ok::<Event, std::convert::Infallible>(Event::default().event("udp").data(json))
        });
        return sse_response(Box::pin(once_stream));
    };

    let stream = make_recv_stream(socket);
    sse_response(Box::pin(stream))
}

fn make_recv_stream(
    socket: Arc<UdpSocket>,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        let mut buf = vec![0u8; 65535];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((n, src)) => {
                    // lint-allow-unused: recv_from guarantees n <= buf.len(), so [..n] is in bounds
                    #[allow(clippy::indexing_slicing)]
                    let data = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                    let dgram = UdpDatagram { data, src_addr: src.to_string() };
                    let Ok(json) = serde_json::to_string(&dgram) else { continue };
                    yield Ok(Event::default().event("udp").data(json));
                }
                Err(e) => {
                    let json = serde_json::json!({ "err": e.to_string() }).to_string();
                    yield Ok(Event::default().event("udp_error").data(json));
                    break;
                }
            }
        }
    }
}

async fn handle_close(
    State(state): State<UdpState>,
    Json(req): Json<UdpCloseRequest>,
) -> impl IntoResponse {
    let removed = state.sessions.lock().ok().and_then(|mut m| m.remove(&req.session_id));
    if removed.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(UdpCloseResponse {
                ok: false,
                err: Some(format!("session {} not found", req.session_id)),
            }),
        );
    }
    (StatusCode::OK, Json(UdpCloseResponse { ok: true, err: None }))
}

// ── Helper ─────────────────────────────────────────────────────────────────────

fn get_socket(state: &UdpState, session_id: &str) -> Option<Arc<UdpSocket>> {
    state
        .sessions
        .lock()
        .ok()
        .and_then(|m| m.get(session_id).map(|e| Arc::clone(&e.socket)))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn udp_bind_response_serializes() {
        let r = UdpBindResponse {
            ok: true,
            session_id: "abc".into(),
            local_port: 12345,
            err: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"local_port\":12345"));
    }

    #[test]
    fn udp_datagram_round_trip() {
        let d = UdpDatagram {
            data: base64::engine::general_purpose::STANDARD.encode(b"hello"),
            src_addr: "127.0.0.1:9999".into(),
        };
        let json = serde_json::to_string(&d).unwrap();
        let parsed: UdpDatagram = serde_json::from_str(&json).unwrap();
        let decoded =
            base64::engine::general_purpose::STANDARD.decode(parsed.data).unwrap();
        assert_eq!(decoded, b"hello");
    }
}
