use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    AppState,
    auth::{AuthUser, Claims},
    error::{AppError, Result},
    models::{AuthChallenge, Device, UserRecord},
};

// ── Router ───────────────────────────────────────────────────────────────────

/// Routes accessible without a token.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(signup))
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
    pub display_name: Option<String>,
    pub device_name: Option<String>,
}

/// Request a challenge nonce for Ed25519 signin.
#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    /// Hex-encoded Ed25519 public key.
    pub public_key: String,
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
        .map_err(|_| AppError::BadRequest("invalid hex in public_key".into()))?;
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| AppError::BadRequest("public_key must be 32 bytes (64 hex chars)".into()))?;
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| AppError::BadRequest("invalid Ed25519 public key".into()))
}

/// Generate 32 random bytes, hex-encoded.
fn random_nonce() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let buf: [u8; 32] = rng.random();
    hex::encode(buf)
}

/// Insert a new `device` record and return its string ID.
async fn create_device(
    state: &AppState,
    user_id: &str,
    device_name: Option<&str>,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<String> {
    let name = device_name.unwrap_or("Unknown device");
    let created: Option<Device> = state
        .db
        .query(
            "CREATE device CONTENT { \
              owner: type::thing($uid), \
              name: $name, \
              user_agent: $ua, \
              ip: $ip, \
              created_at: time::now(), \
              last_seen: time::now(), \
              revoked: false \
            } RETURN *",
        )
        .bind(("uid", user_id.to_owned()))
        .bind(("name", name.to_owned()))
        .bind(("ua", user_agent.map(str::to_owned)))
        .bind(("ip", ip.map(str::to_owned)))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    created
        .and_then(|d| d.id)
        .ok_or_else(|| AppError::Internal("failed to create device".into()))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /auth/signup` — register a new account using Ed25519 public key.
///
/// The public key becomes the cryptographic identity for this user on this server.
/// No password is needed — authentication happens via challenge-response.
async fn signup(
    State(state): State<AppState>,
    Json(req): Json<SignupRequest>,
) -> Result<Json<AuthResponse>> {
    // Validate inputs.
    if req.username.trim().is_empty() {
        return Err(AppError::BadRequest("username required".into()));
    }
    if req.public_key.trim().is_empty() {
        return Err(AppError::BadRequest("public_key required".into()));
    }

    // Validate the public key is well-formed.
    let _vk = decode_public_key(&req.public_key)?;

    // Check username uniqueness.
    let existing_name: Option<UserRecord> = state
        .db
        .query("SELECT * FROM user WHERE username = $u LIMIT 1")
        .bind(("u", req.username.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if existing_name.is_some() {
        return Err(AppError::Conflict("username already taken".into()));
    }

    // Check public key uniqueness.
    let existing_key: Option<UserRecord> = state
        .db
        .query("SELECT * FROM user WHERE public_key = $pk LIMIT 1")
        .bind(("pk", req.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if existing_key.is_some() {
        return Err(AppError::Conflict("public key already registered".into()));
    }

    let display_name = req.display_name.unwrap_or_else(|| req.username.clone());

    // Create user record with public key (no password hash).
    let created: Option<UserRecord> = state
        .db
        .query(
            "CREATE user CONTENT { \
              username: $u, display_name: $d, \
              public_key: $pk, created_at: time::now() \
            } RETURN *",
        )
        .bind(("u", req.username.clone()))
        .bind(("d", display_name))
        .bind(("pk", req.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let user_id = created
        .and_then(|u| u.id)
        .ok_or_else(|| AppError::Internal("failed to create user".into()))?;

    let device_id = create_device(&state, &user_id, req.device_name.as_deref(), None, None).await?;
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

/// `POST /auth/challenge` — request a random nonce for Ed25519 signin.
///
/// The client must sign this nonce and submit it to `/auth/verify`.
/// Challenges expire after 60 seconds.
async fn challenge(
    State(state): State<AppState>,
    Json(req): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>> {
    if req.public_key.trim().is_empty() {
        return Err(AppError::BadRequest("public_key required".into()));
    }

    // Validate the public key format.
    let _vk = decode_public_key(&req.public_key)?;

    // Check the public key is registered.
    let existing: Option<UserRecord> = state
        .db
        .query("SELECT * FROM user WHERE public_key = $pk LIMIT 1")
        .bind(("pk", req.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if existing.is_none() {
        return Err(AppError::NotFound);
    }

    let nonce = random_nonce();
    let expires_at = Utc::now() + Duration::seconds(60);

    // Store the challenge.
    state
        .db
        .query(
            "CREATE auth_challenge CONTENT { \
              public_key: $pk, nonce: $n, \
              expires_at: $exp, used: false, \
              created_at: time::now() \
            }",
        )
        .bind(("pk", req.public_key.clone()))
        .bind(("n", nonce.clone()))
        .bind(("exp", expires_at.to_rfc3339()))
        .await?
        .check()
        .map_err(AppError::Db)?;

    Ok(Json(ChallengeResponse {
        challenge: nonce,
        expires_at: expires_at.to_rfc3339(),
    }))
}

/// `POST /auth/verify` — complete Ed25519 challenge-response signin.
///
/// The client signs the challenge nonce with their private key. The server
/// verifies using the stored public key and issues a JWT.
async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<AuthResponse>> {
    if req.public_key.trim().is_empty() || req.challenge.is_empty() || req.signature.is_empty() {
        return Err(AppError::BadRequest(
            "public_key, challenge, and signature required".into(),
        ));
    }

    // Decode and validate the public key.
    let vk = decode_public_key(&req.public_key)?;

    // Look up the challenge record.
    let challenge_record: AuthChallenge = state
        .db
        .query(
            "SELECT * FROM auth_challenge \
             WHERE public_key = $pk AND nonce = $n AND used = false \
             LIMIT 1",
        )
        .bind(("pk", req.public_key.clone()))
        .bind(("n", req.challenge.clone()))
        .await?
        .take::<Option<AuthChallenge>>(0)
        .map_err(AppError::Db)?
        .ok_or(AppError::Unauthorized)?;

    // Check expiry.
    if Utc::now() > challenge_record.expires_at {
        return Err(AppError::Unauthorized);
    }

    // Verify the Ed25519 signature over the raw challenge bytes.
    let challenge_bytes = hex::decode(&req.challenge)
        .map_err(|_| AppError::BadRequest("invalid hex in challenge".into()))?;
    let sig_bytes = hex::decode(&req.signature)
        .map_err(|_| AppError::BadRequest("invalid hex in signature".into()))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| AppError::BadRequest("signature must be 64 bytes (128 hex chars)".into()))?;
    let signature = Signature::from_bytes(&sig_arr);

    vk.verify_strict(&challenge_bytes, &signature)
        .map_err(|_| AppError::Unauthorized)?;

    // Mark the challenge as used.
    let challenge_id = challenge_record
        .id
        .ok_or(AppError::Internal("missing challenge id".into()))?;
    state
        .db
        .query("UPDATE type::thing($id) SET used = true")
        .bind(("id", challenge_id))
        .await?
        .check()
        .map_err(AppError::Db)?;

    // Look up the user by public key.
    let user: UserRecord = state
        .db
        .query("SELECT * FROM user WHERE public_key = $pk LIMIT 1")
        .bind(("pk", req.public_key.clone()))
        .await?
        .take::<Option<UserRecord>>(0)
        .map_err(AppError::Db)?
        .ok_or(AppError::Unauthorized)?;

    let user_id = user
        .id
        .ok_or_else(|| AppError::Internal("missing user id".into()))?;

    let device_id = create_device(&state, &user_id, req.device_name.as_deref(), None, None).await?;
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
    state
        .db
        .query("UPDATE type::thing($id) SET revoked = true")
        .bind(("id", auth.device_id.clone()))
        .await?
        .check()
        .map_err(AppError::Db)?;

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
    let devices: Vec<Device> = state
        .db
        .query("SELECT * FROM device WHERE owner = type::thing($id) ORDER BY last_seen DESC")
        .bind(("id", auth.user_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

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
        .query("SELECT * FROM type::thing($id) LIMIT 1")
        .bind(("id", device_id.clone()))
        .await?
        .take::<Option<Device>>(0)
        .map_err(AppError::Db)?
        .ok_or(AppError::NotFound)?;

    if device.owner != auth.user_id {
        return Err(AppError::Forbidden);
    }

    state
        .db
        .query("UPDATE type::thing($id) SET revoked = true")
        .bind(("id", device_id))
        .await?
        .check()
        .map_err(AppError::Db)?;

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
