use axum::http::header;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::{AppError, Result},
};

pub mod routes;

// ── JWT Claims ────────────────────────────────────────────────────────────────

/// Claims embedded in the JWT token issued on sign-in/sign-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// User ID (SurrealDB record ID, e.g. `user:abc123`).
    pub sub: String,
    /// Device ID (e.g. `device:xyz789`).
    pub device_id: String,
    /// Expiry (Unix timestamp).
    pub exp: u64,
    /// Issued-at (Unix timestamp).
    pub iat: u64,
}

impl Claims {
    /// Encode a new JWT.
    pub fn encode(
        user_id: &str,
        device_id: &str,
        secret: &str,
        expiry_secs: u64,
    ) -> Result<String> {
        let now = Utc::now().timestamp() as u64;
        let claims = Self {
            sub: user_id.to_owned(),
            device_id: device_id.to_owned(),
            exp: now + expiry_secs,
            iat: now,
        };
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .map_err(|e| AppError::Internal(format!("JWT encode error: {e}")))
    }

    /// Decode and validate a JWT.
    pub fn decode(token: &str, secret: &str) -> Result<Self> {
        decode::<Self>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &Validation::default(),
        )
        .map(|d| d.claims)
        .map_err(|_| AppError::Unauthorized)
    }
}

// ── Axum extractor ────────────────────────────────────────────────────────────

/// Axum extension type — inserted by `auth_middleware`, available in handlers.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub device_id: String,
}

/// Axum middleware that extracts and validates the `Authorization: Bearer` token.
/// Rejects with 401 if missing or invalid.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> std::result::Result<Response, AppError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let claims = Claims::decode(token, &state.config.jwt_secret)?;

    // Check that the device has not been revoked.
    let device_id = claims.device_id.clone();
    let revoked: Option<serde_json::Value> = state
        .db
        .query("SELECT revoked FROM type::thing($id)")
        .bind(("id", device_id))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let is_revoked = revoked
        .as_ref()
        .and_then(|v| v.get("revoked"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_revoked {
        return Err(AppError::Unauthorized);
    }

    req.extensions_mut().insert(AuthUser {
        user_id: claims.sub,
        device_id: claims.device_id,
    });
    Ok(next.run(req).await)
}

/// Helper to extract `AuthUser` from request extensions (panics cleanly if
/// middleware was not applied — should never happen in practice).
pub fn require_auth(ext: axum::extract::Extension<AuthUser>) -> AuthUser {
    ext.0
}
