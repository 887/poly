//! # `VideoBridgeClient` — typed client for `/host/video/*`
//!
//! Convenience wrapper around `reqwest` that `NativeVideoBackend`
//! (and any other native caller) can use instead of hand-rolling JSON.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use poly_host_bridge::video_client::{VideoBridgeClient, EncodeH264Request};
//!
//! let client = VideoBridgeClient::new("http://127.0.0.1:9333");
//! let resp = client.encode(EncodeH264Request {
//!     width: 1280, height: 720,
//!     format: "bgra".into(),
//!     data_b64: base64::encode(&frame_bytes),
//!     force_keyframe: false,
//!     session_id: "stream-0".into(),
//! }).await?;
//! ```
//!
//! The client is `Clone + Send + Sync` — share a single instance across tasks.

use thiserror::Error;

// Re-export the wire types so callers only import this module.
pub use crate::video::{
    CloseSessionRequest, CloseSessionResponse, DecodeH264Request, DecodeH264Response, DecodedFrame,
    EncodeH264Request, EncodeH264Response,
};

/// Errors from [`VideoBridgeClient`].
#[derive(Debug, Error)]
pub enum VideoClientError {
    /// HTTP transport failure (no connection, timeout, etc.).
    #[error("video bridge transport: {0}")]
    Transport(#[from] reqwest::Error),
    /// JSON serialization/deserialization failed.
    #[error("video bridge JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The server returned `ok: false` with an error message.
    #[error("video bridge server error: {0}")]
    Server(String),
}

/// Typed client for the `/host/video/*` endpoints.
///
/// Both native and test callers use this. WASM callers call the same endpoint
/// via their normal HTTP stack (the module is cfg'd out of WASM targets).
#[derive(Clone, Debug)]
pub struct VideoBridgeClient {
    http: reqwest::Client,
    base_url: String,
}

impl VideoBridgeClient {
    /// Construct a client pointing at `base_url` (e.g. `"http://127.0.0.1:9333"`).
    /// No trailing slash needed.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Construct a client pointing at the default loopback bridge port.
    #[must_use]
    pub fn default_local() -> Self {
        Self::new(crate::BRIDGE_BASE_URL)
    }

    /// `POST /host/video/encode_h264` — encode one frame.
    ///
    /// # Errors
    ///
    /// Returns [`VideoClientError::Server`] when the bridge reports `ok: false`.
    pub async fn encode(
        &self,
        req: EncodeH264Request,
    ) -> Result<EncodeH264Response, VideoClientError> {
        let url = format!("{}{}", self.base_url, crate::video::ROUTE_VIDEO_ENCODE);
        let resp: EncodeH264Response = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(resp)
        } else {
            Err(VideoClientError::Server(
                resp.err.unwrap_or_else(|| "encode_h264 failed".into()),
            ))
        }
    }

    /// `POST /host/video/decode_h264` — decode NAL unit(s).
    ///
    /// # Errors
    ///
    /// Returns [`VideoClientError::Server`] when the bridge reports `ok: false`.
    pub async fn decode(
        &self,
        req: DecodeH264Request,
    ) -> Result<DecodeH264Response, VideoClientError> {
        let url = format!("{}{}", self.base_url, crate::video::ROUTE_VIDEO_DECODE);
        let resp: DecodeH264Response = self.post_json(&url, &req).await?;
        if resp.ok {
            Ok(resp)
        } else {
            Err(VideoClientError::Server(
                resp.err.unwrap_or_else(|| "decode_h264 failed".into()),
            ))
        }
    }

    /// `POST /host/video/close_session` — release encoder + decoder for a session.
    ///
    /// Idempotent — safe to call even if the session was never opened.
    ///
    /// # Errors
    ///
    /// Only returns `Err` on transport / JSON failure, not for "session not found".
    pub async fn close_session(&self, session_id: &str) -> Result<(), VideoClientError> {
        let url = format!(
            "{}{}",
            self.base_url,
            crate::video::ROUTE_VIDEO_CLOSE_SESSION
        );
        let req = CloseSessionRequest {
            session_id: session_id.to_string(),
        };
        let _resp: CloseSessionResponse = self.post_json(&url, &req).await?;
        Ok(())
    }

    // ── private helper ─────────────────────────────────────────────────────────

    async fn post_json<T, B>(&self, url: &str, body: &B) -> Result<T, VideoClientError>
    where
        T: serde::de::DeserializeOwned,
        B: serde::Serialize,
    {
        let text = self
            .http
            .post(url)
            .json(body)
            .send()
            .await?
            .text()
            .await?;
        let v: T = serde_json::from_str(&text)?;
        Ok(v)
    }
}
