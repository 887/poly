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
    net::SocketAddr,
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
///
/// `policy` decides whether a session may be pointed at a loopback / private
/// / link-local peer. It is default-deny so an untrusted caller that reaches
/// `/host/udp/*` cannot use the host as a datagram cannon into the LAN or
/// into loopback-only services.
#[derive(Clone, Default)]
pub struct UdpState {
    sessions: Arc<Mutex<HashMap<String, UdpSessionEntry>>>,
    policy: crate::net_guard::PrivateNetworkPolicy,
}

struct UdpSessionEntry {
    socket: Arc<UdpSocket>,
    /// Peer fixed by `/host/udp/connect`. `send` may only ever target this
    /// address — a per-datagram `dst` override is accepted solely when it
    /// matches, so a caller cannot re-aim an already-vetted socket.
    peer: Option<SocketAddr>,
}

impl UdpState {
    /// Production constructor — destination policy comes from
    /// [`crate::net_guard::PrivateNetworkPolicy::from_env`] (deny by default,
    /// `POLY_ALLOW_PRIVATE_NETWORK=1` to opt out for local development).
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::default(),
            policy: crate::net_guard::PrivateNetworkPolicy::from_env(),
        }
    }

    /// Constructor for tests and local development against a loopback voice
    /// server: skips the private-destination filter.
    #[must_use]
    pub fn allow_private() -> Self {
        Self {
            sessions: Arc::default(),
            policy: crate::net_guard::PrivateNetworkPolicy::Allow,
        }
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
        map.insert(session_id.clone(), UdpSessionEntry { socket, peer: None });
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

    let peer: SocketAddr = match req.peer_addr.parse() {
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
    if let Err(e) = crate::net_guard::check_socket_addr(peer, state.policy) {
        return (
            StatusCode::FORBIDDEN,
            Json(UdpConnectResponse { ok: false, err: Some(e) }),
        );
    }

    match socket.connect(peer).await {
        Ok(()) => {
            set_peer(&state, &req.session_id, peer);
            (StatusCode::OK, Json(UdpConnectResponse { ok: true, err: None }))
        }
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
    let Some(session) = get_session(&state, &req.session_id) else {
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

    let dst = match resolve_dst(session.peer, req.dst.as_deref()) {
        Ok(d) => d,
        Err((code, err)) => {
            return (
                code,
                Json(UdpSendResponse { ok: false, bytes_sent: 0, err: Some(err) }),
            );
        }
    };

    let result = match dst {
        Some(addr) => session.socket.send_to(&data, addr).await,
        None => session.socket.send(&data).await,
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

/// Immutable view of a session, taken while the map lock is held so no
/// handler keeps the mutex across an `.await`.
struct SessionSnapshot {
    socket: Arc<UdpSocket>,
    peer: Option<SocketAddr>,
}

fn get_session(state: &UdpState, session_id: &str) -> Option<SessionSnapshot> {
    state.sessions.lock().ok().and_then(|m| {
        m.get(session_id).map(|e| SessionSnapshot {
            socket: Arc::clone(&e.socket),
            peer: e.peer,
        })
    })
}

fn set_peer(state: &UdpState, session_id: &str, peer: SocketAddr) {
    if let Ok(mut map) = state.sessions.lock()
        && let Some(entry) = map.get_mut(session_id)
    {
        entry.peer = Some(peer);
    }
}

/// Decide which address `send` may target.
///
/// `Ok(None)` means "use the connected peer" (`UdpSocket::send`). An explicit
/// `dst` is honoured **only** when it is byte-for-byte the peer the session
/// was connected to — otherwise a caller could bind one socket and then spray
/// datagrams at arbitrary hosts, bypassing the `connect`-time destination
/// filter entirely.
fn resolve_dst(
    peer: Option<SocketAddr>,
    dst: Option<&str>,
) -> Result<Option<SocketAddr>, (StatusCode, String)> {
    let Some(raw) = dst else {
        return Ok(None);
    };
    let addr: SocketAddr = raw
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid dst addr: {e}")))?;
    match peer {
        Some(connected) if connected == addr => Ok(Some(addr)),
        Some(connected) => Err((
            StatusCode::FORBIDDEN,
            format!("dst {addr} does not match the connected peer {connected}"),
        )),
        None => Err((
            StatusCode::FORBIDDEN,
            "dst requires a prior /host/udp/connect to the same address".to_string(),
        )),
    }
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

    /// A `dst` override may only ever name the address the session was
    /// connected to — otherwise `/host/udp/send` is an arbitrary-datagram
    /// cannon for anyone who can reach the bridge.
    #[test]
    fn dst_override_is_pinned_to_the_connected_peer() {
        let peer: SocketAddr = "203.0.113.7:9000".parse().unwrap();

        assert_eq!(resolve_dst(Some(peer), None).unwrap(), None);
        assert_eq!(
            resolve_dst(Some(peer), Some("203.0.113.7:9000")).unwrap(),
            Some(peer)
        );

        let (code, msg) = resolve_dst(Some(peer), Some("8.8.8.8:53")).unwrap_err();
        assert_eq!(code, StatusCode::FORBIDDEN);
        assert!(msg.contains("connected peer"), "{msg}");

        let (code, msg) = resolve_dst(None, Some("8.8.8.8:53")).unwrap_err();
        assert_eq!(code, StatusCode::FORBIDDEN);
        assert!(msg.contains("connect"), "{msg}");
    }

    #[test]
    fn dst_override_rejects_unparseable_addresses() {
        let (code, _) = resolve_dst(None, Some("not-an-addr")).unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    /// The production constructor is default-deny; the dev constructor is not.
    #[test]
    fn state_policy_defaults_to_deny() {
        assert!(UdpState::default().policy.denies_private());
        assert!(!UdpState::allow_private().policy.denies_private());
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
