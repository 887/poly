//! # `OpusClient` — typed client for `/host/codec/opus/*`
//!
//! Available on **all targets** including `wasm32-unknown-unknown`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use poly_host_bridge::codec_opus_client::OpusClient;
//!
//! let client = OpusClient::from_origin();
//!
//! // Create an encoder session.
//! let enc = client.encoder_create(48000, 2, "voip").await?;
//!
//! // Encode a PCM frame.
//! let packet = client.encode(&enc, &pcm_i16_samples).await?;
//!
//! // Create a decoder session.
//! let dec = client.decoder_create(48000, 2).await?;
//!
//! // Decode a packet.
//! let pcm = client.decode(&dec, &packet).await?;
//!
//! // Close when done.
//! client.close(&enc).await?;
//! client.close(&dec).await?;
//! ```

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::route::{self, HostRoute, TransportError};

// ── Route constants ────────────────────────────────────────────────────────────

pub const ROUTE_OPUS_ENCODER_CREATE: &str = "/host/codec/opus/encoder/create";
pub const ROUTE_OPUS_ENCODER_ENCODE: &str = "/host/codec/opus/encoder/encode";
pub const ROUTE_OPUS_DECODER_CREATE: &str = "/host/codec/opus/decoder/create";
pub const ROUTE_OPUS_DECODER_DECODE: &str = "/host/codec/opus/decoder/decode";
pub const ROUTE_OPUS_CLOSE: &str = "/host/codec/opus/close";

// ── Wire types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusEncoderCreateRequest {
    pub sample_rate: u32,
    pub channels: u8,
    pub application: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusSessionCreateResponse {
    pub ok: bool,
    #[serde(default)]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusEncodeRequest {
    pub session_id: String,
    /// Little-endian i16 PCM samples, base64-encoded.
    pub pcm: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusEncodeResponse {
    pub ok: bool,
    #[serde(default)]
    pub encoded: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusDecoderCreateRequest {
    pub sample_rate: u32,
    pub channels: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusDecodeRequest {
    pub session_id: String,
    pub encoded: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusDecodeResponse {
    pub ok: bool,
    #[serde(default)]
    pub pcm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusCloseRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpusCloseResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── Error type ─────────────────────────────────────────────────────────────────

/// Errors from [`OpusClient`].
#[derive(Debug, Error)]
pub enum OpusClientError {
    #[error("Opus client transport: {0}")]
    Transport(#[from] TransportError),
    #[error("Opus client server error: {0}")]
    Server(String),
}

// ── Route impls ────────────────────────────────────────────────────────────────

/// Route: `POST /host/codec/opus/encoder/create`
pub struct OpusEncoderCreateRoute;

impl HostRoute for OpusEncoderCreateRoute {
    type Req = OpusEncoderCreateRequest;
    type Resp = OpusSessionCreateResponse;
    type Err = OpusClientError;
    fn endpoint() -> &'static str {
        ROUTE_OPUS_ENCODER_CREATE
    }
}

/// Route: `POST /host/codec/opus/encoder/encode`
pub struct OpusEncoderEncodeRoute;

impl HostRoute for OpusEncoderEncodeRoute {
    type Req = OpusEncodeRequest;
    type Resp = OpusEncodeResponse;
    type Err = OpusClientError;
    fn endpoint() -> &'static str {
        ROUTE_OPUS_ENCODER_ENCODE
    }
}

/// Route: `POST /host/codec/opus/decoder/create`
pub struct OpusDecoderCreateRoute;

impl HostRoute for OpusDecoderCreateRoute {
    type Req = OpusDecoderCreateRequest;
    type Resp = OpusSessionCreateResponse;
    type Err = OpusClientError;
    fn endpoint() -> &'static str {
        ROUTE_OPUS_DECODER_CREATE
    }
}

/// Route: `POST /host/codec/opus/decoder/decode`
pub struct OpusDecoderDecodeRoute;

impl HostRoute for OpusDecoderDecodeRoute {
    type Req = OpusDecodeRequest;
    type Resp = OpusDecodeResponse;
    type Err = OpusClientError;
    fn endpoint() -> &'static str {
        ROUTE_OPUS_DECODER_DECODE
    }
}

/// Route: `POST /host/codec/opus/close`
pub struct OpusCloseRoute;

impl HostRoute for OpusCloseRoute {
    type Req = OpusCloseRequest;
    type Resp = OpusCloseResponse;
    type Err = OpusClientError;
    fn endpoint() -> &'static str {
        ROUTE_OPUS_CLOSE
    }
}

// ── Client ─────────────────────────────────────────────────────────────────────

/// Typed client for the `/host/codec/opus/*` endpoints.
#[derive(Clone, Debug)]
pub struct OpusClient {
    http: reqwest::Client,
    base_url: String,
}

impl OpusClient {
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

    /// Create an Opus encoder session.
    ///
    /// `application`: `"voip"` | `"audio"` | `"low_delay"`.
    ///
    /// # Errors
    /// Returns [`OpusClientError::Server`] if params are invalid or `audiopus` fails.
    pub async fn encoder_create(
        &self,
        sample_rate: u32,
        channels: u8,
        application: &str,
    ) -> Result<String, OpusClientError> {
        let req = OpusEncoderCreateRequest {
            sample_rate,
            channels,
            application: application.to_string(),
        };
        let resp =
            route::call::<OpusEncoderCreateRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            Ok(resp.session_id)
        } else {
            Err(OpusClientError::Server(
                resp.err.unwrap_or_else(|| "opus/encoder/create failed".into()),
            ))
        }
    }

    /// Encode PCM samples to an Opus packet.
    ///
    /// `pcm` must be stereo (if encoder was created with `channels=2`) LE i16.
    ///
    /// # Errors
    /// Returns [`OpusClientError::Server`] on encode failure.
    pub async fn encode(
        &self,
        session_id: &str,
        pcm: &[i16],
    ) -> Result<Vec<u8>, OpusClientError> {
        let mut bytes = Vec::with_capacity(pcm.len().saturating_mul(2));
        for &s in pcm {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let req = OpusEncodeRequest {
            session_id: session_id.to_string(),
            pcm: base64::engine::general_purpose::STANDARD.encode(&bytes),
        };
        let resp =
            route::call::<OpusEncoderEncodeRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            base64::engine::general_purpose::STANDARD
                .decode(resp.encoded.as_bytes())
                .map_err(|e| OpusClientError::Server(format!("base64 decode: {e}")))
        } else {
            Err(OpusClientError::Server(
                resp.err.unwrap_or_else(|| "opus/encoder/encode failed".into()),
            ))
        }
    }

    /// Create an Opus decoder session.
    ///
    /// # Errors
    /// Returns [`OpusClientError::Server`] if params are invalid.
    pub async fn decoder_create(
        &self,
        sample_rate: u32,
        channels: u8,
    ) -> Result<String, OpusClientError> {
        let req = OpusDecoderCreateRequest { sample_rate, channels };
        let resp =
            route::call::<OpusDecoderCreateRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            Ok(resp.session_id)
        } else {
            Err(OpusClientError::Server(
                resp.err.unwrap_or_else(|| "opus/decoder/create failed".into()),
            ))
        }
    }

    /// Decode an Opus packet to PCM samples (LE i16).
    ///
    /// # Errors
    /// Returns [`OpusClientError::Server`] on decode failure.
    pub async fn decode(
        &self,
        session_id: &str,
        encoded: &[u8],
    ) -> Result<Vec<i16>, OpusClientError> {
        let req = OpusDecodeRequest {
            session_id: session_id.to_string(),
            encoded: base64::engine::general_purpose::STANDARD.encode(encoded),
        };
        let resp =
            route::call::<OpusDecoderDecodeRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(resp.pcm.as_bytes())
                .map_err(|e| OpusClientError::Server(format!("base64 decode: {e}")))?;
            if bytes.len() % 2 != 0 {
                return Err(OpusClientError::Server("pcm byte length not even".into()));
            }
            // `chunks_exact(2)` guarantees each `c` has exactly 2 elements, so
            // the array conversion can't fail; fall back to 0 to stay panic-free.
            let samples: Vec<i16> = bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes(c.try_into().unwrap_or([0, 0])))
                .collect();
            Ok(samples)
        } else {
            Err(OpusClientError::Server(
                resp.err.unwrap_or_else(|| "opus/decoder/decode failed".into()),
            ))
        }
    }

    /// Close an encoder or decoder session.
    ///
    /// # Errors
    /// Returns [`OpusClientError::Server`] if the session is not found.
    pub async fn close(&self, session_id: &str) -> Result<(), OpusClientError> {
        let req = OpusCloseRequest { session_id: session_id.to_string() };
        let resp = route::call::<OpusCloseRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(OpusClientError::Server(
                resp.err.unwrap_or_else(|| "opus/close failed".into()),
            ))
        }
    }
}
