//! # `UdpClient` — typed client for `/host/udp/*`
//!
//! Available on **all targets** including `wasm32-unknown-unknown`. Use this
//! from WASM plugins or native callers that need UDP I/O through the bridge.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use poly_host_bridge::udp_client::UdpClient;
//!
//! let client = UdpClient::from_origin();
//! let resp = client.bind().await?;
//! let session_id = resp.session_id;
//!
//! client.connect(&session_id, "1.2.3.4:9000").await?;
//! client.send(&session_id, &payload_bytes, None).await?;
//!
//! // Stream incoming datagrams.
//! let stream = client.recv_stream(&session_id);
//!
//! client.close(&session_id).await?;
//! ```

use base64::Engine as _;
use futures::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Route constants ────────────────────────────────────────────────────────────

pub const ROUTE_UDP_BIND: &str = "/host/udp/bind";
pub const ROUTE_UDP_CONNECT: &str = "/host/udp/connect";
pub const ROUTE_UDP_SEND: &str = "/host/udp/send";
pub const ROUTE_UDP_RECV_STREAM_PATTERN: &str = "/host/udp/recv_stream/:id";
pub const ROUTE_UDP_CLOSE: &str = "/host/udp/close";

// ── Wire types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpBindResponse {
    pub ok: bool,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub local_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpConnectRequest {
    pub session_id: String,
    pub peer_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpConnectResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpSendRequest {
    pub session_id: String,
    pub data: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dst: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpSendResponse {
    pub ok: bool,
    #[serde(default)]
    pub bytes_sent: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// One datagram delivered over the SSE recv stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpDatagram {
    pub data: String,
    pub src_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpCloseRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpCloseResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

/// Errors from [`UdpClient`].
#[derive(Debug, Error)]
pub enum UdpClientError {
    #[error("UDP client transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("UDP client JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("UDP client server error: {0}")]
    Server(String),
}

/// Typed client for the `/host/udp/*` endpoints.
#[derive(Clone, Debug)]
pub struct UdpClient {
    http: reqwest::Client,
    base_url: String,
}

impl UdpClient {
    /// Construct targeting `base_url`.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { http: reqwest::Client::new(), base_url: base_url.into() }
    }

    #[must_use]
    pub fn default_local() -> Self {
        Self::new(crate::BRIDGE_BASE_URL)
    }

    #[cfg(target_arch = "wasm32")]
    #[must_use]
    pub fn from_origin() -> Self {
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| crate::BRIDGE_BASE_URL.to_string());
        Self::new(origin)
    }

    // ── Endpoints ──────────────────────────────────────────────────────────────

    /// `POST /host/udp/bind` — bind a new UDP socket.
    ///
    /// Returns `session_id` and the OS-assigned `local_port`.
    ///
    /// # Errors
    /// Returns [`UdpClientError::Server`] if the native bind fails.
    pub async fn bind(&self) -> Result<UdpBindResponse, UdpClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_UDP_BIND);
        let resp: UdpBindResponse = self.post_json(&url, &serde_json::json!({})).await?;
        if resp.ok {
            Ok(resp)
        } else {
            Err(UdpClientError::Server(resp.err.unwrap_or_else(|| "udp/bind failed".into())))
        }
    }

    /// `POST /host/udp/connect` — connect a UDP socket to a fixed peer.
    ///
    /// # Errors
    /// Returns [`UdpClientError::Server`] if the session is not found or connect fails.
    pub async fn connect(
        &self,
        session_id: &str,
        peer_addr: &str,
    ) -> Result<(), UdpClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_UDP_CONNECT);
        let req = UdpConnectRequest {
            session_id: session_id.to_string(),
            peer_addr: peer_addr.to_string(),
        };
        let resp: UdpConnectResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(UdpClientError::Server(resp.err.unwrap_or_else(|| "udp/connect failed".into())))
        }
    }

    /// `POST /host/udp/send` — send a datagram.
    ///
    /// `dst` is optional; omit if the socket is already connected.
    ///
    /// # Errors
    /// Returns [`UdpClientError::Server`] on send failure.
    pub async fn send(
        &self,
        session_id: &str,
        data: &[u8],
        dst: Option<&str>,
    ) -> Result<UdpSendResponse, UdpClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_UDP_SEND);
        let req = UdpSendRequest {
            session_id: session_id.to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(data),
            dst: dst.map(str::to_string),
        };
        let resp: UdpSendResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(resp)
        } else {
            Err(UdpClientError::Server(resp.err.unwrap_or_else(|| "udp/send failed".into())))
        }
    }

    /// `GET /host/udp/recv_stream/:id` — subscribe to the SSE datagram stream.
    ///
    /// Returns a boxed `Stream<Item = UdpDatagram>` that is `'static`. Using
    /// `Pin<Box<...>>` avoids Rust 2024 lifetime-capture issues with opaque
    /// return types; callers can use the stream with `.filter_map` etc.
    ///
    /// Uses `LocalBoxStream` (no `Send` bound) so it compiles on `wasm32`
    /// where `reqwest` futures are not `Send`.
    pub fn recv_stream_boxed(
        &self,
        session_id: impl Into<String>,
    ) -> futures::stream::LocalBoxStream<'static, UdpDatagram> {
        let url = format!("{}/host/udp/recv_stream/{}", self.base_url, session_id.into());
        let http = self.http.clone();
        Box::pin(make_dgram_stream(http, url))
    }

    /// Unboxed version — may capture `self` lifetime in Rust 2024.
    /// Prefer [`recv_stream_boxed`][Self::recv_stream_boxed] for `'static` contexts.
    pub fn recv_stream(&self, session_id: &str) -> impl Stream<Item = UdpDatagram> + '_ {
        let url = format!("{}/host/udp/recv_stream/{}", self.base_url, session_id);
        let http = self.http.clone();
        make_dgram_stream(http, url)
    }

    /// `POST /host/udp/close` — close and drop a UDP session.
    ///
    /// # Errors
    /// Returns [`UdpClientError::Server`] if the session is not found.
    pub async fn close(&self, session_id: &str) -> Result<(), UdpClientError> {
        let url = format!("{}{}", self.base_url, ROUTE_UDP_CLOSE);
        let req = UdpCloseRequest { session_id: session_id.to_string() };
        let resp: UdpCloseResponse = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(UdpClientError::Server(resp.err.unwrap_or_else(|| "udp/close failed".into())))
        }
    }

    // ── private helper ─────────────────────────────────────────────────────────

    async fn post_json<T, B>(&self, url: &str, body: &B) -> Result<T, UdpClientError>
    where
        T: serde::de::DeserializeOwned,
        B: serde::Serialize,
    {
        let text = self.http.post(url).json(body).send().await?.text().await?;
        let v: T = serde_json::from_str(&text)?;
        Ok(v)
    }
}

// ── SSE stream ─────────────────────────────────────────────────────────────────

fn make_dgram_stream(http: reqwest::Client, url: String) -> impl Stream<Item = UdpDatagram> {
    async_stream::stream! {
        let resp = match http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(target: "poly_host_bridge::udp_client", error = %e, "SSE connect failed");
                return;
            }
        };

        use futures::StreamExt;
        let mut bytes_stream = resp.bytes_stream();
        let mut line_buf = String::new();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(target: "poly_host_bridge::udp_client", error = %e, "SSE read error");
                    break;
                }
            };
            let text = match std::str::from_utf8(&chunk) {
                Ok(t) => t,
                Err(_) => continue,
            };
            line_buf.push_str(text);

            while let Some(pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=pos).collect();
                let line = line.trim_end_matches(['\n', '\r']);
                if let Some(data) = line.strip_prefix("data: ") {
                    match serde_json::from_str::<UdpDatagram>(data) {
                        Ok(dgram) => yield dgram,
                        Err(e) => {
                            tracing::warn!(
                                target: "poly_host_bridge::udp_client",
                                error = %e,
                                "failed to parse SSE UdpDatagram"
                            );
                        }
                    }
                }
            }
        }
    }
}
