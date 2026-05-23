//! # `AeadClient` — typed client for `/host/aead/*`
//!
//! Available on **all targets** including `wasm32-unknown-unknown`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use poly_host_bridge::aead_client::AeadClient;
//!
//! let client = AeadClient::from_origin();
//!
//! // Create a keyed session.
//! let session = client.create("xchacha20poly1305", &secret_key_32_bytes).await?;
//!
//! // Encrypt.
//! let ct = client.encrypt(&session, &nonce_24_bytes, &plaintext, None).await?;
//!
//! // Decrypt.
//! let pt = client.decrypt(&session, &nonce_24_bytes, &ct, None).await?;
//!
//! // Close the session.
//! client.close(&session).await?;
//! ```

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::route::{self, HostRoute, TransportError};

// ── Route constants ────────────────────────────────────────────────────────────

pub const ROUTE_AEAD_CREATE: &str = "/host/aead/create";
pub const ROUTE_AEAD_ENCRYPT: &str = "/host/aead/encrypt";
pub const ROUTE_AEAD_DECRYPT: &str = "/host/aead/decrypt";
pub const ROUTE_AEAD_CLOSE: &str = "/host/aead/close";

// ── Wire types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadCreateRequest {
    pub algorithm: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadCreateResponse {
    pub ok: bool,
    #[serde(default)]
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadEncryptRequest {
    pub session_id: String,
    pub nonce: String,
    pub plaintext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aad: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadEncryptResponse {
    pub ok: bool,
    #[serde(default)]
    pub ciphertext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadDecryptRequest {
    pub session_id: String,
    pub nonce: String,
    pub ciphertext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aad: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadDecryptResponse {
    pub ok: bool,
    #[serde(default)]
    pub plaintext: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadCloseRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AeadCloseResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
}

// ── Error type ─────────────────────────────────────────────────────────────────

/// Errors from [`AeadClient`].
#[derive(Debug, Error)]
pub enum AeadClientError {
    #[error("AEAD client transport: {0}")]
    Transport(#[from] TransportError),
    #[error("AEAD client server error: {0}")]
    Server(String),
}

// ── Route impls ────────────────────────────────────────────────────────────────

/// Route: `POST /host/aead/create`
pub struct AeadCreateRoute;

impl HostRoute for AeadCreateRoute {
    type Req = AeadCreateRequest;
    type Resp = AeadCreateResponse;
    type Err = AeadClientError;
    fn endpoint() -> &'static str {
        ROUTE_AEAD_CREATE
    }
}

/// Route: `POST /host/aead/encrypt`
pub struct AeadEncryptRoute;

impl HostRoute for AeadEncryptRoute {
    type Req = AeadEncryptRequest;
    type Resp = AeadEncryptResponse;
    type Err = AeadClientError;
    fn endpoint() -> &'static str {
        ROUTE_AEAD_ENCRYPT
    }
}

/// Route: `POST /host/aead/decrypt`
pub struct AeadDecryptRoute;

impl HostRoute for AeadDecryptRoute {
    type Req = AeadDecryptRequest;
    type Resp = AeadDecryptResponse;
    type Err = AeadClientError;
    fn endpoint() -> &'static str {
        ROUTE_AEAD_DECRYPT
    }
}

/// Route: `POST /host/aead/close`
pub struct AeadCloseRoute;

impl HostRoute for AeadCloseRoute {
    type Req = AeadCloseRequest;
    type Resp = AeadCloseResponse;
    type Err = AeadClientError;
    fn endpoint() -> &'static str {
        ROUTE_AEAD_CLOSE
    }
}

// ── Client ─────────────────────────────────────────────────────────────────────

/// Typed client for the `/host/aead/*` endpoints.
#[derive(Clone, Debug)]
pub struct AeadClient {
    http: reqwest::Client,
    base_url: String,
}

impl AeadClient {
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

    /// `POST /host/aead/create` — create a keyed AEAD session.
    ///
    /// `algorithm`: `"xchacha20poly1305"` or `"aes256gcm"`.
    /// `key`: raw 32-byte key (not base64 — this method encodes for you).
    ///
    /// # Errors
    /// Returns [`AeadClientError::Server`] if the key length or algorithm is invalid.
    pub async fn create(
        &self,
        algorithm: &str,
        key: &[u8],
    ) -> Result<String, AeadClientError> {
        let req = AeadCreateRequest {
            algorithm: algorithm.to_string(),
            key: base64::engine::general_purpose::STANDARD.encode(key),
        };
        let resp = route::call::<AeadCreateRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            Ok(resp.session_id)
        } else {
            Err(AeadClientError::Server(
                resp.err.unwrap_or_else(|| "aead/create failed".into()),
            ))
        }
    }

    /// `POST /host/aead/encrypt` — encrypt plaintext with optional AAD.
    ///
    /// Returns ciphertext bytes (with appended authentication tag).
    ///
    /// # Errors
    /// Returns [`AeadClientError::Server`] on encrypt failure.
    pub async fn encrypt(
        &self,
        session_id: &str,
        nonce: &[u8],
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadClientError> {
        let req = AeadEncryptRequest {
            session_id: session_id.to_string(),
            nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
            plaintext: base64::engine::general_purpose::STANDARD.encode(plaintext),
            aad: aad.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        };
        let resp = route::call::<AeadEncryptRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            base64::engine::general_purpose::STANDARD
                .decode(resp.ciphertext.as_bytes())
                .map_err(|e| AeadClientError::Server(format!("base64 decode: {e}")))
        } else {
            Err(AeadClientError::Server(
                resp.err.unwrap_or_else(|| "aead/encrypt failed".into()),
            ))
        }
    }

    /// `POST /host/aead/decrypt` — decrypt ciphertext with optional AAD.
    ///
    /// Returns plaintext bytes.
    ///
    /// # Errors
    /// Returns [`AeadClientError::Server`] if decryption fails (bad tag, wrong key, etc.).
    pub async fn decrypt(
        &self,
        session_id: &str,
        nonce: &[u8],
        ciphertext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadClientError> {
        let req = AeadDecryptRequest {
            session_id: session_id.to_string(),
            nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
            ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
            aad: aad.map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        };
        let resp = route::call::<AeadDecryptRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            base64::engine::general_purpose::STANDARD
                .decode(resp.plaintext.as_bytes())
                .map_err(|e| AeadClientError::Server(format!("base64 decode: {e}")))
        } else {
            Err(AeadClientError::Server(
                resp.err.unwrap_or_else(|| "aead/decrypt failed".into()),
            ))
        }
    }

    /// `POST /host/aead/close` — drop a keyed session.
    ///
    /// # Errors
    /// Returns [`AeadClientError::Server`] if the session is not found.
    pub async fn close(&self, session_id: &str) -> Result<(), AeadClientError> {
        let req = AeadCloseRequest { session_id: session_id.to_string() };
        let resp = route::call::<AeadCloseRoute>(&self.http, &self.base_url, req).await?;
        if resp.ok {
            Ok(())
        } else {
            Err(AeadClientError::Server(
                resp.err.unwrap_or_else(|| "aead/close failed".into()),
            ))
        }
    }
}
