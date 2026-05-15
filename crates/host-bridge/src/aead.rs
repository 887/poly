//! # `/host/aead/*` — generic AEAD encrypt/decrypt service
//!
//! Exposes keyed-session AEAD (Authenticated Encryption with Associated Data)
//! over HTTP. Supports `xchacha20poly1305` and `aes256gcm`. The session model
//! avoids re-deriving the cipher per frame.
//!
//! ## Routes
//!
//! ```text
//! POST /host/aead/create   { algorithm, key: base64 } -> { session_id }
//! POST /host/aead/encrypt  { session_id, nonce: base64, plaintext: base64, aad?: base64 } -> { ciphertext: base64 }
//! POST /host/aead/decrypt  { session_id, nonce: base64, ciphertext: base64, aad?: base64 } -> { plaintext: base64 }
//! POST /host/aead/close    { session_id }
//! ```
//!
//! ## WASM safety
//!
//! `#[cfg(all(not(target_arch = "wasm32"), feature = "aead"))]`

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use aes_gcm::{Aes256Gcm, Nonce as GcmNonce, aead::{Aead as GcmAead, KeyInit as GcmKeyInit}};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use base64::Engine as _;
use chacha20poly1305::{XChaCha20Poly1305, XNonce, aead::{Aead, Payload}};
use uuid::Uuid;

// Wire types and route constants are defined in aead_client (always compiled).
pub use crate::aead_client::{
    AeadCloseRequest, AeadCloseResponse, AeadCreateRequest, AeadCreateResponse,
    AeadDecryptRequest, AeadDecryptResponse, AeadEncryptRequest, AeadEncryptResponse,
    ROUTE_AEAD_CLOSE, ROUTE_AEAD_CREATE, ROUTE_AEAD_DECRYPT, ROUTE_AEAD_ENCRYPT,
};

// ── Session state ──────────────────────────────────────────────────────────────

enum AeadSession {
    XChaCha20(XChaCha20Poly1305),
    Aes256Gcm(Aes256Gcm),
}

/// Shared state for the AEAD service.
#[derive(Clone, Default)]
pub struct AeadState {
    sessions: Arc<Mutex<HashMap<String, AeadSession>>>,
}

impl AeadState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ── Router ─────────────────────────────────────────────────────────────────────

#[must_use]
pub fn router(state: AeadState) -> axum::Router {
    use axum::routing::post;
    axum::Router::new()
        .route(ROUTE_AEAD_CREATE, post(handle_create))
        .route(ROUTE_AEAD_ENCRYPT, post(handle_encrypt))
        .route(ROUTE_AEAD_DECRYPT, post(handle_decrypt))
        .route(ROUTE_AEAD_CLOSE, post(handle_close))
        .with_state(state)
}

// ── Handlers ───────────────────────────────────────────────────────────────────

async fn handle_create(
    State(state): State<AeadState>,
    Json(req): Json<AeadCreateRequest>,
) -> impl IntoResponse {
    let key_bytes = match b64_decode(&req.key) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AeadCreateResponse {
                    ok: false,
                    session_id: String::new(),
                    err: Some(format!("invalid key base64: {e}")),
                }),
            );
        }
    };

    let session = match req.algorithm.as_str() {
        "xchacha20poly1305" => {
            match XChaCha20Poly1305::new_from_slice(&key_bytes) {
                Ok(c) => AeadSession::XChaCha20(c),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(AeadCreateResponse {
                            ok: false,
                            session_id: String::new(),
                            err: Some(format!("XChaCha20Poly1305::new: {e}")),
                        }),
                    );
                }
            }
        }
        "aes256gcm" => {
            match Aes256Gcm::new_from_slice(&key_bytes) {
                Ok(c) => AeadSession::Aes256Gcm(c),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(AeadCreateResponse {
                            ok: false,
                            session_id: String::new(),
                            err: Some(format!("Aes256Gcm::new: {e}")),
                        }),
                    );
                }
            }
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(AeadCreateResponse {
                    ok: false,
                    session_id: String::new(),
                    err: Some(format!("unsupported algorithm: {}", req.algorithm)),
                }),
            );
        }
    };

    let session_id = Uuid::new_v4().to_string();
    state.sessions.lock().unwrap().insert(session_id.clone(), session);

    (StatusCode::OK, Json(AeadCreateResponse { ok: true, session_id, err: None }))
}

async fn handle_encrypt(
    State(state): State<AeadState>,
    Json(req): Json<AeadEncryptRequest>,
) -> impl IntoResponse {
    let nonce_bytes = match b64_decode(&req.nonce) {
        Ok(b) => b,
        Err(e) => {
            return err_encrypt(format!("invalid nonce base64: {e}"));
        }
    };
    let plaintext = match b64_decode(&req.plaintext) {
        Ok(b) => b,
        Err(e) => {
            return err_encrypt(format!("invalid plaintext base64: {e}"));
        }
    };
    let aad = match req.aad.as_deref().map(b64_decode) {
        Some(Ok(b)) => b,
        Some(Err(e)) => return err_encrypt(format!("invalid aad base64: {e}")),
        None => Vec::new(),
    };

    let map = state.sessions.lock().unwrap();
    let result = match map.get(&req.session_id) {
        Some(AeadSession::XChaCha20(c)) => {
            if nonce_bytes.len() != 24 {
                return err_encrypt("XChaCha20 nonce must be 24 bytes".into());
            }
            let nonce = XNonce::from_slice(&nonce_bytes);
            c.encrypt(nonce, Payload { msg: &plaintext, aad: &aad })
                .map_err(|_| "AEAD encrypt failed".to_string())
        }
        Some(AeadSession::Aes256Gcm(c)) => {
            if nonce_bytes.len() != 12 {
                return err_encrypt("AES-256-GCM nonce must be 12 bytes".into());
            }
            let nonce = GcmNonce::from_slice(&nonce_bytes);
            aes_gcm::aead::AeadInPlace::encrypt_in_place_detached(c, nonce, &aad, &mut plaintext.clone())
                .map(|tag| {
                    let mut out = plaintext.clone();
                    out.extend_from_slice(&tag);
                    out
                })
                .map_err(|_| "AES-GCM encrypt failed".to_string())
        }
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(AeadEncryptResponse {
                    ok: false,
                    ciphertext: String::new(),
                    err: Some(format!("session {} not found", req.session_id)),
                }),
            );
        }
    };

    match result {
        Ok(ct) => (
            StatusCode::OK,
            Json(AeadEncryptResponse {
                ok: true,
                ciphertext: b64_encode(&ct),
                err: None,
            }),
        ),
        Err(e) => err_encrypt(e),
    }
}

async fn handle_decrypt(
    State(state): State<AeadState>,
    Json(req): Json<AeadDecryptRequest>,
) -> impl IntoResponse {
    let nonce_bytes = match b64_decode(&req.nonce) {
        Ok(b) => b,
        Err(e) => return err_decrypt(format!("invalid nonce base64: {e}")),
    };
    let ciphertext = match b64_decode(&req.ciphertext) {
        Ok(b) => b,
        Err(e) => return err_decrypt(format!("invalid ciphertext base64: {e}")),
    };
    let aad = match req.aad.as_deref().map(b64_decode) {
        Some(Ok(b)) => b,
        Some(Err(e)) => return err_decrypt(format!("invalid aad base64: {e}")),
        None => Vec::new(),
    };

    let map = state.sessions.lock().unwrap();
    let result = match map.get(&req.session_id) {
        Some(AeadSession::XChaCha20(c)) => {
            if nonce_bytes.len() != 24 {
                return err_decrypt("XChaCha20 nonce must be 24 bytes".into());
            }
            let nonce = XNonce::from_slice(&nonce_bytes);
            c.decrypt(nonce, Payload { msg: &ciphertext, aad: &aad })
                .map_err(|_| "AEAD decrypt failed".to_string())
        }
        Some(AeadSession::Aes256Gcm(c)) => {
            if nonce_bytes.len() != 12 {
                return err_decrypt("AES-256-GCM nonce must be 12 bytes".into());
            }
            let nonce = GcmNonce::from_slice(&nonce_bytes);
            c.decrypt(nonce, aes_gcm::aead::Payload { msg: &ciphertext, aad: &aad })
                .map_err(|_| "AES-GCM decrypt failed".to_string())
        }
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(AeadDecryptResponse {
                    ok: false,
                    plaintext: String::new(),
                    err: Some(format!("session {} not found", req.session_id)),
                }),
            );
        }
    };

    match result {
        Ok(pt) => (
            StatusCode::OK,
            Json(AeadDecryptResponse { ok: true, plaintext: b64_encode(&pt), err: None }),
        ),
        Err(e) => err_decrypt(e),
    }
}

async fn handle_close(
    State(state): State<AeadState>,
    Json(req): Json<AeadCloseRequest>,
) -> impl IntoResponse {
    let removed = state.sessions.lock().unwrap().remove(&req.session_id);
    if removed.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(AeadCloseResponse {
                ok: false,
                err: Some(format!("session {} not found", req.session_id)),
            }),
        );
    }
    (StatusCode::OK, Json(AeadCloseResponse { ok: true, err: None }))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

fn err_encrypt(msg: String) -> (StatusCode, Json<AeadEncryptResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(AeadEncryptResponse { ok: false, ciphertext: String::new(), err: Some(msg) }),
    )
}

fn err_decrypt(msg: String) -> (StatusCode, Json<AeadDecryptResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(AeadDecryptResponse { ok: false, plaintext: String::new(), err: Some(msg) }),
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn aead_wire_types_serialize() {
        let r = AeadCreateResponse {
            ok: true,
            session_id: "sess-1".into(),
            err: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"session_id\":\"sess-1\""));
    }

    #[test]
    fn aead_encrypt_request_optional_aad() {
        let r = AeadEncryptRequest {
            session_id: "s".into(),
            nonce: "AA==".into(),
            plaintext: "AA==".into(),
            aad: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("aad"), "aad should be skipped when None");
    }
}
