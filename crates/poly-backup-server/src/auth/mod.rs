//! Authentication module — PoW challenges, passphrase verification, session tokens.
//!
//! ## Auth flow
//! 1. Client: `POST /api/challenge` with `{ "public_key": "<hex>" }`
//! 2. Server: generate random nonce, store in DB, return `{ nonce, difficulty, expires_at }`
//! 3. Client: mine SHA-256(nonce + counter.to_string()) until leading `difficulty` bits are zero
//! 4. Client: `POST /api/auth` with `{ public_key, nonce, counter, passphrase, device_name }`
//! 5. Server: verify PoW, verify passphrase via SHA-256 constant-time, issue 128-char token
//! 6. Client: use `Authorization: Bearer <token>` for all subsequent requests
//!
//! ## Token storage
//! Raw token is returned once. Server stores `SHA-256(token)` — never the raw value.

use axum::{
    Json,
    extract::{FromRef, FromRequestParts, State},
    http::{StatusCode, request::Parts},
};
use chrono::Utc;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use utoipa::ToSchema;

use crate::{
    AppState,
    error::{AppError, Result},
};

// ── Request / response types ─────────────────────────────────────────────────

/// Request body for `POST /api/challenge`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ChallengeRequest {
    /// Client's Ed25519 public key (64-char lowercase hex).
    pub public_key: String,
}

/// Response from `POST /api/challenge`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChallengeResponse {
    /// Random nonce to include in the PoW hash input.
    pub nonce: String,
    /// Number of leading zero bits required in SHA-256(nonce + counter).
    pub difficulty: u32,
    /// ISO-8601 UTC timestamp when the challenge expires (60 seconds from now).
    pub expires_at: String,
}

/// Request body for `POST /api/auth`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthRequest {
    /// Client's Ed25519 public key — must match the key used in `/api/challenge`.
    pub public_key: String,
    /// Nonce from the challenge response.
    pub nonce: String,
    /// Counter value that satisfies `PoW(nonce, counter, difficulty)`.
    pub counter: u64,
    /// Server-wide passphrase (shared out-of-band).
    pub passphrase: String,
    /// Human-readable device name for session tracking (e.g. "Linux Desktop").
    pub device_name: String,
}

/// Response from `POST /api/auth`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    /// 128-character alphanumeric session token. Use as `Authorization: Bearer <token>`.
    /// This is the only time the raw token is sent — store it securely.
    pub token: String,
    /// ISO-8601 UTC timestamp when the token expires if inactive.
    pub expires_at: String,
}

/// Extracted authenticated principal — injected into handlers via Axum extractor.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// The authenticated account's Ed25519 public key.
    pub public_key: String,
    /// SHA-256 hash of the raw session token.
    pub token_hash: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /api/challenge` — issue a PoW nonce for a given public key.
#[utoipa::path(
    post,
    path = "/api/challenge",
    request_body = ChallengeRequest,
    responses(
        (status = 200, description = "Challenge issued", body = ChallengeResponse),
        (status = 400, description = "Invalid public key format"),
        (status = 429, description = "Too many failed attempts from this IP"),
    ),
    tag = "auth"
)]
pub async fn request_challenge(
    State(state): State<AppState>,
    Json(body): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>> {
    if body.public_key.len() != 64 || !body.public_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest(
            "public_key must be 64 lowercase hex characters".into(),
        ));
    }

    let nonce = Alphanumeric.sample_string(&mut rand::thread_rng(), 64);
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(60);

    // Delete any previous pending challenge for this public key.
    state
        .db
        .query("DELETE challenge WHERE public_key = $pk")
        .bind(("pk", body.public_key.clone()))
        .await?
        .check()
        .map_err(AppError::from)?;

    state
        .db
        .query(
            "CREATE challenge CONTENT { \
              nonce: $nonce, public_key: $pk, difficulty: $diff, \
              created_at: time::now(), expires_at: $exp \
            }",
        )
        .bind(("nonce", nonce.clone()))
        .bind(("pk", body.public_key))
        .bind(("diff", state.config.pow_difficulty as i64))
        .bind(("exp", expires_at.to_rfc3339()))
        .await?
        .check()
        .map_err(AppError::from)?;

    Ok(Json(ChallengeResponse {
        nonce,
        difficulty: state.config.pow_difficulty,
        expires_at: expires_at.to_rfc3339(),
    }))
}

/// `POST /api/auth` — verify PoW + passphrase, issue session token.
#[utoipa::path(
    post,
    path = "/api/auth",
    request_body = AuthRequest,
    responses(
        (status = 200, description = "Authenticated; session token returned", body = AuthResponse),
        (status = 400, description = "Missing or malformed fields"),
        (status = 401, description = "Invalid passphrase or PoW solution"),
        (status = 403, description = "Account limit reached"),
        (status = 429, description = "Too many failed auth attempts"),
    ),
    tag = "auth"
)]
pub async fn authenticate(
    State(state): State<AppState>,
    Json(body): Json<AuthRequest>,
) -> Result<Json<AuthResponse>> {
    if body.public_key.len() != 64 || !body.public_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("invalid public_key".into()));
    }
    if body.device_name.trim().is_empty() {
        return Err(AppError::BadRequest("device_name is required".into()));
    }

    // Look up the pending challenge.
    let challenge: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM challenge \
             WHERE nonce = $nonce AND public_key = $pk \
             AND expires_at > time::now() \
             LIMIT 1",
        )
        .bind(("nonce", body.nonce.clone()))
        .bind(("pk", body.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::from)?;

    let challenge = challenge.ok_or(AppError::Unauthorized)?;
    let difficulty = challenge
        .get("difficulty")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(state.config.pow_difficulty as u64) as u32;

    // Verify PoW.
    if !verify_pow(&body.nonce, body.counter, difficulty) {
        tracing::warn!("PoW verification failed for pk={}", body.public_key);
        return Err(AppError::Unauthorized);
    }

    // Constant-time passphrase check.
    if !ct_passphrase_eq(&body.passphrase, &state.config.passphrase) {
        tracing::warn!("Passphrase mismatch for pk={}", body.public_key);
        return Err(AppError::Unauthorized);
    }

    // Consume the challenge.
    state
        .db
        .query("DELETE challenge WHERE nonce = $nonce")
        .bind(("nonce", body.nonce))
        .await?
        .check()
        .map_err(AppError::from)?;

    // Enforce account limit.
    if state.config.max_accounts > 0 {
        let existing: Option<serde_json::Value> = state
            .db
            .query("SELECT * FROM account WHERE public_key = $pk LIMIT 1")
            .bind(("pk", body.public_key.clone()))
            .await?
            .take(0)
            .map_err(AppError::from)?;

        if existing.is_none() {
            let count: Option<serde_json::Value> = state
                .db
                .query("SELECT count() AS n FROM account GROUP ALL")
                .await?
                .take(0)
                .map_err(AppError::from)?;
            let n = count
                .as_ref()
                .and_then(|v| v.get("n"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            if n >= state.config.max_accounts {
                return Err(AppError::Forbidden("account limit reached".into()));
            }
        }
    }

    // Upsert account.
    state
        .db
        .query(
            "IF (SELECT id FROM account WHERE public_key = $pk LIMIT 1) THEN \
               (UPDATE account SET last_seen_at = time::now() WHERE public_key = $pk) \
             ELSE \
               (CREATE account CONTENT { public_key: $pk, registered_at: time::now(), last_seen_at: time::now() }) \
             END",
        )
        .bind(("pk", body.public_key.clone()))
        .await?
        .check()
        .map_err(AppError::from)?;

    // Generate and store session token.
    let raw_token = Alphanumeric.sample_string(&mut rand::thread_rng(), 128);
    let token_hash = hash_token(&raw_token);
    let expires_at = Utc::now() + chrono::Duration::days(state.config.token_expiry_days as i64);

    state
        .db
        .query(
            "CREATE token CONTENT { \
               token_hash: $hash, public_key: $pk, device_name: $dev, \
               created_at: time::now(), last_seen_at: time::now(), \
               expires_at: $exp \
             }",
        )
        .bind(("hash", token_hash))
        .bind(("pk", body.public_key.clone()))
        .bind(("dev", body.device_name.clone()))
        .bind(("exp", expires_at.to_rfc3339()))
        .await?
        .check()
        .map_err(AppError::from)?;

    tracing::info!(
        "Authenticated: pk={} device={}",
        &body.public_key[..8],
        body.device_name
    );

    Ok(Json(AuthResponse {
        token: raw_token,
        expires_at: expires_at.to_rfc3339(),
    }))
}

// ── Bearer token extractor ────────────────────────────────────────────────────

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let raw_token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "missing Bearer token" })),
                )
            })?
            .to_owned();

        let token_hash = hash_token(&raw_token);

        let record: Option<serde_json::Value> = app_state
            .db
            .query(
                "SELECT * FROM token \
                 WHERE token_hash = $hash AND expires_at > time::now() \
                 LIMIT 1",
            )
            .bind(("hash", token_hash.clone()))
            .await
            .map_err(|e: surrealdb::Error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?
            .take(0)
            .map_err(|e: surrealdb::Error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;

        let record = record.ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid or expired token" })),
            )
        })?;

        let public_key = record
            .get("public_key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "db record missing public_key" })),
                )
            })?
            .to_owned();

        // Roll expiry forward on every use.
        let new_expiry =
            Utc::now() + chrono::Duration::days(app_state.config.token_expiry_days as i64);
        let _ = app_state
            .db
            .query(
                "UPDATE token SET last_seen_at = time::now(), expires_at = $exp \
                 WHERE token_hash = $hash",
            )
            .bind(("exp", new_expiry.to_rfc3339()))
            .bind(("hash", token_hash.clone()))
            .await;

        let _ = app_state
            .db
            .query("UPDATE account SET last_seen_at = time::now() WHERE public_key = $pk")
            .bind(("pk", public_key.clone()))
            .await;

        Ok(AuthUser {
            public_key,
            token_hash,
        })
    }
}

// ── Utility functions ─────────────────────────────────────────────────────────

/// Hash a raw token with SHA-256. This is what gets stored in the database.
pub fn hash_token(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex::encode(digest)
}

/// Constant-time passphrase comparison.
///
/// Both sides are SHA-256-hashed first so byte lengths are always equal,
/// preventing timing leakage from different-length inputs.
fn ct_passphrase_eq(submitted: &str, stored: &str) -> bool {
    let a = Sha256::digest(submitted.as_bytes());
    let b = Sha256::digest(stored.as_bytes());
    a.ct_eq(&b).into()
}

/// Verify a PoW solution: SHA-256(`nonce` + `counter`) must have at least
/// `difficulty` leading zero bits.
pub fn verify_pow(nonce: &str, counter: u64, difficulty: u32) -> bool {
    if difficulty == 0 {
        return true;
    }
    let input = format!("{nonce}{counter}");
    let hash = Sha256::digest(input.as_bytes());
    let full_bytes = (difficulty / 8) as usize;
    let remaining_bits = difficulty % 8;
    for byte in hash.iter().take(full_bytes) {
        if *byte != 0 {
            return false;
        }
    }
    if remaining_bits > 0 {
        let mask = 0xFFu8 << (8 - remaining_bits);
        if hash.get(full_bytes).map_or(false, |b| b & mask != 0) {
            return false;
        }
    }
    true
}
