use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
use chrono::Utc;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    AppState,
    auth::{AuthUser, Claims},
    error::{AppError, Result},
    models::{Device, UserRecord},
};

// ── Router ───────────────────────────────────────────────────────────────────

/// Routes accessible without a token.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/accounts", post(list_accounts))
        .route("/auth/challenge", post(challenge))
        .route("/auth/verify", post(verify))
        .route("/server-info", get(server_info))
}

/// Routes that require auth (merged into the protected router in main.rs).
pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/auth/signout", post(signout))
        .route("/auth/devices", get(list_devices))
        .route("/auth/devices/{device_id}", delete(revoke_device))
}

// ── Request / response types ──────────────────────────────────────────────────

/// Ed25519 key-based signup — no password required.
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    /// Hex-encoded Ed25519 public key (64 hex chars = 32 bytes).
    pub public_key: String,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub device_name: Option<String>,
}

/// Request a challenge nonce for Ed25519 signin.
#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    /// Hex-encoded Ed25519 public key.
    pub public_key: String,
    /// Optional target account when multiple server accounts share one key.
    pub user_id: Option<String>,
}

/// Request all accounts linked to an Ed25519 public key.
#[derive(Debug, Deserialize)]
pub struct AccountsRequest {
    /// Hex-encoded Ed25519 public key.
    pub public_key: String,
}

/// Public summary of an account linked to an identity key.
#[derive(Debug, Serialize)]
pub struct IdentityAccount {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Response for account lookup by public key.
#[derive(Debug, Serialize)]
pub struct AccountsResponse {
    pub accounts: Vec<IdentityAccount>,
}

/// Challenge response from the server.
#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    /// Hex-encoded 32-byte random nonce.
    pub challenge: String,
    /// When this challenge expires (ISO 8601).
    pub expires_at: String,
}

/// Client submits signature over the challenge nonce.
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    /// Hex-encoded Ed25519 public key.
    pub public_key: String,
    /// Optional target account when multiple server accounts share one key.
    pub user_id: Option<String>,
    /// Hex-encoded challenge nonce (received from /auth/challenge).
    pub challenge: String,
    /// Hex-encoded Ed25519 signature over the raw challenge bytes.
    pub signature: String,
    pub device_name: Option<String>,
}

/// Successful auth response (used by both signup and verify).
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub device_id: String,
}

/// Public server info (no auth required).
#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: &'static str,
    pub invite_only: bool,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Decode a hex-encoded Ed25519 public key into a `VerifyingKey`.
fn decode_public_key(hex_key: &str) -> Result<VerifyingKey> {
    let bytes = hex::decode(hex_key)
        .map_err(|_e| AppError::BadRequest("invalid hex in public_key".into()))?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_e| AppError::BadRequest("public_key must be 32 bytes (64 hex chars)".into()))?;
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_e| AppError::BadRequest("invalid Ed25519 public key".into()))
}

/// Generate 32 random bytes, hex-encoded.
fn random_nonce() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let buf: [u8; 32] = rng.random();
    hex::encode(buf)
}

/// Resolve the target user for signin based on the supplied key and user ID.
async fn resolve_user_for_signin(
    state: &AppState,
    public_key: &str,
    requested_user_id: Option<&str>,
) -> Result<UserRecord> {
    let users = state.db.get_users_by_pubkey(public_key).await?;
    if users.is_empty() {
        return Err(AppError::NotFound);
    }

    if let Some(user_id) = requested_user_id {
        let Some(user) = users
            .into_iter()
            .find(|user| user.id.as_deref() == Some(user_id))
        else {
            return Err(AppError::Unauthorized);
        };
        return Ok(user);
    }

    if users.len() > 1 {
        return Err(AppError::BadRequest(
            "multiple accounts found for this identity key; user_id required".into(),
        ));
    }

    users.into_iter().next().ok_or(AppError::NotFound)
}

/// Small signup sanity check for email addresses.
fn is_valid_email(email: &str) -> bool {
    let trimmed = email.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return false;
    };

    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /auth/signup` — register a new account using Ed25519 public key.
async fn signup(
    State(state): State<AppState>,
    Json(req): Json<SignupRequest>,
) -> Result<Json<AuthResponse>> {
    if req.username.trim().is_empty() {
        return Err(AppError::BadRequest("username required".into()));
    }
    if req.email.trim().is_empty() {
        return Err(AppError::BadRequest("email required".into()));
    }
    if req.public_key.trim().is_empty() {
        return Err(AppError::BadRequest("public_key required".into()));
    }
    if !is_valid_email(&req.email) {
        return Err(AppError::BadRequest("invalid email address".into()));
    }

    let _vk = decode_public_key(&req.public_key)?;

    // Check username uniqueness.
    if state.db.get_user_by_username(&req.username).await?.is_some() {
        return Err(AppError::Conflict("username already taken".into()));
    }

    // Check email uniqueness.
    if state.db.get_user_by_email(req.email.trim()).await?.is_some() {
        return Err(AppError::Conflict("email already registered".into()));
    }

    let display_name = req.display_name.unwrap_or_else(|| req.username.clone());

    let created = state
        .db
        .create_user(&req.username, req.email.trim(), &display_name, &req.public_key)
        .await?;

    let user_id = created
        .and_then(|u| u.id)
        .ok_or_else(|| AppError::Internal("failed to create user".into()))?;

    let name = req.device_name.as_deref().unwrap_or("Unknown device");
    let device = state
        .db
        .create_device(&user_id, name, None, None)
        .await?;
    let device_id = device
        .and_then(|d| d.id)
        .ok_or_else(|| AppError::Internal("failed to create device".into()))?;

    let token = Claims::encode(
        &user_id,
        &device_id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_secs,
    )?;

    info!(
        "New user signed up: {} (key: {}…)",
        req.username,
        &req.public_key.get(..8).unwrap_or("?")
    );
    Ok(Json(AuthResponse {
        token,
        user_id,
        device_id,
    }))
}

/// `POST /auth/accounts` — list all existing accounts for an Ed25519 key.
async fn list_accounts(
    State(state): State<AppState>,
    Json(req): Json<AccountsRequest>,
) -> Result<Json<AccountsResponse>> {
    if req.public_key.trim().is_empty() {
        return Err(AppError::BadRequest("public_key required".into()));
    }

    let _vk = decode_public_key(&req.public_key)?;
    let users = state.db.get_users_by_pubkey(&req.public_key).await?;
    let accounts = users
        .into_iter()
        .filter_map(|user| {
            let user_id = user.id?;
            Some(IdentityAccount {
                user_id,
                username: user.username,
                display_name: user.display_name,
                avatar_url: user.avatar_url,
            })
        })
        .collect();
    Ok(Json(AccountsResponse { accounts }))
}

/// `POST /auth/challenge` — request a random nonce for Ed25519 signin.
async fn challenge(
    State(state): State<AppState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>> {
    if req.public_key.trim().is_empty() {
        return Err(AppError::BadRequest("public_key required".into()));
    }

    let _vk = decode_public_key(&req.public_key)?;
    let _user = resolve_user_for_signin(&state, &req.public_key, req.user_id.as_deref()).await?;

    let nonce = random_nonce();

    let created = state
        .db
        .create_auth_challenge(&req.public_key, &nonce)
        .await?
        .ok_or(AppError::Internal("failed to create challenge".into()))?;

    Ok(Json(ChallengeResponse {
        challenge: nonce,
        expires_at: created.expires_at.to_rfc3339(),
    }))
}

/// `POST /auth/verify` — complete Ed25519 challenge-response signin.
async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<AuthResponse>> {
    if req.public_key.trim().is_empty() || req.challenge.is_empty() || req.signature.is_empty() {
        return Err(AppError::BadRequest(
            "public_key, challenge, and signature required".into(),
        ));
    }

    let vk = decode_public_key(&req.public_key)?;

    let challenge_record = state
        .db
        .get_auth_challenge(&req.public_key, &req.challenge)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if Utc::now() > challenge_record.expires_at {
        return Err(AppError::Unauthorized);
    }

    // Verify the Ed25519 signature over the raw challenge bytes.
    let challenge_bytes = hex::decode(&req.challenge)
        .map_err(|_e| AppError::BadRequest("invalid hex in challenge".into()))?;
    let sig_bytes = hex::decode(&req.signature)
        .map_err(|_e| AppError::BadRequest("invalid hex in signature".into()))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_e| AppError::BadRequest("signature must be 64 bytes (128 hex chars)".into()))?;
    let signature = Signature::from_bytes(&sig_arr);

    vk.verify_strict(&challenge_bytes, &signature)
        .map_err(|_e| AppError::Unauthorized)?;

    // Mark the challenge as used.
    let challenge_id = challenge_record
        .id
        .ok_or(AppError::Internal("missing challenge id".into()))?;
    state.db.mark_challenge_used(&challenge_id).await?;

    let user = resolve_user_for_signin(&state, &req.public_key, req.user_id.as_deref()).await?;

    let user_id = user
        .id
        .ok_or_else(|| AppError::Internal("missing user id".into()))?;

    let name = req.device_name.as_deref().unwrap_or("Unknown device");
    let device = state
        .db
        .create_device(&user_id, name, None, None)
        .await?;
    let device_id = device
        .and_then(|d| d.id)
        .ok_or_else(|| AppError::Internal("failed to create device".into()))?;

    let token = Claims::encode(
        &user_id,
        &device_id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_secs,
    )?;

    info!(
        "User signed in via challenge-response (key: {}…)",
        &req.public_key.get(..8).unwrap_or("?")
    );
    Ok(Json(AuthResponse {
        token,
        user_id,
        device_id,
    }))
}

/// `POST /auth/signout` — revoke current device.
async fn signout(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<serde_json::Value>> {
    state.db.revoke_device(&auth.device_id).await?;

    // Push DeviceRevoked event so the WS closes immediately.
    state
        .ws
        .send_to_user(&auth.user_id, crate::ws::ServerEvent::DeviceRevoked)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /auth/devices` — list current user's devices.
async fn list_devices(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<Vec<Device>>> {
    let devices = state.db.list_devices(&auth.user_id).await?;
    Ok(Json(devices))
}

/// `DELETE /auth/devices/:device_id` — revoke a specific device.
async fn revoke_device(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // Verify ownership.
    let device: Device = state
        .db
        .get_device(&device_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if device.owner != auth.user_id {
        return Err(AppError::Forbidden);
    }

    state.db.revoke_device(&device_id).await?;

    // Push DeviceRevoked to the owner's WS so they get logged out on that device.
    state
        .ws
        .send_to_user(&device.owner, crate::ws::ServerEvent::DeviceRevoked)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `GET /server-info` — public endpoint.
async fn server_info(State(state): State<AppState>) -> Json<ServerInfo> {
    Json(ServerInfo {
        name: state.config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION"),
        invite_only: state.config.invite_only,
    })
}
