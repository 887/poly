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
use rand::distr::{Alphanumeric, SampleString};
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

    let nonce = Alphanumeric.sample_string(&mut rand::rng(), 64);
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
              created_at: $now, expires_at: $exp \
            }",
        )
        .bind(("nonce", nonce.clone()))
        .bind(("pk", body.public_key))
        .bind(("diff", state.config.pow_difficulty as i64))
        .bind(("exp", expires_at.to_rfc3339()))
        .bind(("now", now.to_rfc3339()))
        .await?
        .check()
        .map_err(AppError::from)?;

    Ok(Json(ChallengeResponse {
        nonce,
        difficulty: state.config.pow_difficulty,
        expires_at: expires_at.to_rfc3339(),
    }))
}

// ── Helpers for `authenticate` ───────────────────────────────────────────────

/// Validate mandatory fields in an [`AuthRequest`].
fn validate_auth_input(body: &AuthRequest) -> Result<()> {
    if body.public_key.len() != 64 || !body.public_key.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("invalid public_key".into()));
    }
    if body.device_name.trim().is_empty() {
        return Err(AppError::BadRequest("device_name is required".into()));
    }
    Ok(())
}

/// Look up the challenge for `(nonce, public_key)`, check expiry, verify PoW,
/// verify passphrase, then delete the challenge from the database.
async fn verify_and_consume_challenge(
    db: &crate::Db,
    body: &AuthRequest,
    config: &crate::Config,
) -> Result<()> {
    let challenge: Option<serde_json::Value> = db
        .query(
            "SELECT nonce, public_key, difficulty, expires_at FROM challenge \
             WHERE nonce = $nonce AND public_key = $pk \
             LIMIT 1",
        )
        .bind(("nonce", body.nonce.clone()))
        .bind(("pk", body.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::from)?;
    let challenge = challenge.ok_or(AppError::Unauthorized)?;

    let expires_at_str = challenge
        .get("expires_at")
        .and_then(serde_json::Value::as_str)
        .ok_or(AppError::Unauthorized)?;
    if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at_str) {
        if exp <= Utc::now() {
            return Err(AppError::Unauthorized);
        }
    } else {
        return Err(AppError::Unauthorized);
    }

    let difficulty = challenge
        .get("difficulty")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(config.pow_difficulty as u64) as u32;

    if !verify_pow(&body.nonce, body.counter, difficulty) {
        tracing::warn!("PoW verification failed for pk={}", body.public_key);
        return Err(AppError::Unauthorized);
    }
    if !ct_passphrase_eq(&body.passphrase, &config.passphrase) {
        tracing::warn!("Passphrase mismatch for pk={}", body.public_key);
        return Err(AppError::Unauthorized);
    }

    db.query("DELETE challenge WHERE nonce = $nonce")
        .bind(("nonce", body.nonce.clone()))
        .await?
        .check()
        .map_err(AppError::from)?;
    Ok(())
}

/// Enforce the server's account limit (if non-zero) and upsert the account record.
async fn enforce_limit_and_upsert_account(
    db: &crate::Db,
    public_key: &str,
    max_accounts: usize,
    now_str: &str,
) -> Result<()> {
    if max_accounts > 0 {
        let existing: Option<serde_json::Value> = db
            .query("SELECT public_key FROM account WHERE public_key = $pk LIMIT 1")
            .bind(("pk", public_key.to_owned()))
            .await?
            .take(0)
            .map_err(AppError::from)?;
        if existing.is_none() {
            let count: Option<serde_json::Value> = db
                .query("SELECT count() AS n FROM account GROUP ALL")
                .await?
                .take(0)
                .map_err(AppError::from)?;
            let n = count
                .as_ref()
                .and_then(|v| v.get("n"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as usize;
            if n >= max_accounts {
                return Err(AppError::Forbidden("account limit reached".into()));
            }
        }
    }
    db.query(
        "IF (SELECT id FROM account WHERE public_key = $pk LIMIT 1) THEN \
           (UPDATE account SET last_seen_at = $now WHERE public_key = $pk) \
         ELSE \
           (CREATE account CONTENT { public_key: $pk, registered_at: $now, last_seen_at: $now }) \
         END",
    )
    .bind(("pk", public_key.to_owned()))
    .bind(("now", now_str.to_owned()))
    .await?
    .check()
    .map_err(AppError::from)?;
    Ok(())
}

/// Generate a 128-char session token, persist its SHA-256 hash, and return
/// `(raw_token, expires_at_rfc3339)`.
async fn issue_session_token(
    db: &crate::Db,
    public_key: &str,
    device_name: &str,
    token_expiry_days: i64,
    now_str: &str,
) -> Result<(String, String)> {
    let raw_token = Alphanumeric.sample_string(&mut rand::rng(), 128);
    let token_hash = hash_token(&raw_token);
    let expires_at = Utc::now() + chrono::Duration::days(token_expiry_days);
    db.query(
        "CREATE token CONTENT { \
           token_hash: $hash, public_key: $pk, device_name: $dev, \
           created_at: $now, last_seen_at: $now, \
           expires_at: $exp \
         }",
    )
    .bind(("hash", token_hash))
    .bind(("pk", public_key.to_owned()))
    .bind(("dev", device_name.to_owned()))
    .bind(("exp", expires_at.to_rfc3339()))
    .bind(("now", now_str.to_owned()))
    .await?
    .check()
    .map_err(AppError::from)?;
    Ok((raw_token, expires_at.to_rfc3339()))
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
    validate_auth_input(&body)?;
    verify_and_consume_challenge(&state.db, &body, &state.config).await?;
    let now_str = Utc::now().to_rfc3339();
    enforce_limit_and_upsert_account(
        &state.db,
        &body.public_key,
        state.config.max_accounts,
        &now_str,
    )
    .await?;
    let (token, expires_at) = issue_session_token(
        &state.db,
        &body.public_key,
        &body.device_name,
        state.config.token_expiry_days as i64,
        &now_str,
    )
    .await?;
    tracing::info!(
        "Authenticated: pk={} device={}",
        &body.public_key[..8],
        body.device_name
    );
    Ok(Json(AuthResponse { token, expires_at }))
}

// ── Helpers for `FromRequestParts` ───────────────────────────────────────────

type AuthRejection = (StatusCode, Json<serde_json::Value>);

/// Extract the raw Bearer token string from request headers.
fn extract_bearer_token(parts: &Parts) -> std::result::Result<String, AuthRejection> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_owned)
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing Bearer token" })),
            )
        })
}

/// Look up a session-token record from the database by its SHA-256 hash.
async fn fetch_token_record(
    db: &crate::Db,
    token_hash: &str,
) -> std::result::Result<serde_json::Value, AuthRejection> {
    let record: Option<serde_json::Value> = db
        .query(
            "SELECT token_hash, public_key, expires_at FROM token \
             WHERE token_hash = $hash \
             LIMIT 1",
        )
        .bind(("hash", token_hash.to_owned()))
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
    record.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid or expired token" })),
        )
    })
}

/// Return `Ok(())` if the token record's `expires_at` is still in the future.
fn validate_token_expiry(record: &serde_json::Value) -> std::result::Result<(), AuthRejection> {
    let expires_at_str = record
        .get("expires_at")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at_str) {
        if exp <= Utc::now() {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "token expired" })),
            ));
        }
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid token expiry" })),
        ))
    }
}

/// Roll the token's expiry forward and update `last_seen_at` on account and token.
async fn refresh_token_and_account(
    db: &crate::Db,
    token_hash: &str,
    public_key: &str,
    token_expiry_days: i64,
) {
    let new_expiry = (Utc::now() + chrono::Duration::days(token_expiry_days)).to_rfc3339();
    let now_str = Utc::now().to_rfc3339();
    let _ = db
        .query(
            "UPDATE token SET last_seen_at = $now, expires_at = $exp \
             WHERE token_hash = $hash",
        )
        .bind(("exp", new_expiry))
        .bind(("now", now_str.clone()))
        .bind(("hash", token_hash.to_owned()))
        .await;
    let _ = db
        .query("UPDATE account SET last_seen_at = $now WHERE public_key = $pk")
        .bind(("now", now_str))
        .bind(("pk", public_key.to_owned()))
        .await;
}

// ── Bearer token extractor ────────────────────────────────────────────────────

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = AuthRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let raw_token = extract_bearer_token(parts)?;
        let token_hash = hash_token(&raw_token);
        let record = fetch_token_record(&app_state.db, &token_hash).await?;
        validate_token_expiry(&record)?;
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
        refresh_token_and_account(
            &app_state.db,
            &token_hash,
            &public_key,
            app_state.config.token_expiry_days as i64,
        )
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
        if hash.get(full_bytes).is_some_and(|b| b & mask != 0) {
            return false;
        }
    }
    true
}
