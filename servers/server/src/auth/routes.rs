use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{delete, get, post},
};
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
        .route("/auth/signin", post(signin))
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

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SigninRequest {
    pub username: String,
    pub password: String,
    pub device_name: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: &'static str,
    pub invite_only: bool,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /auth/signup` — create a new account + return token.
async fn signup(
    State(state): State<AppState>,
    Json(req): Json<SignupRequest>,
) -> Result<Json<AuthResponse>> {
    // Reject empty username/password.
    if req.username.trim().is_empty() || req.password.is_empty() {
        return Err(AppError::BadRequest(
            "username and password required".into(),
        ));
    }

    // Check username uniqueness.
    let existing: Option<serde_json::Value> = state
        .db
        .query("SELECT id FROM user WHERE username = $u")
        .bind(("u", req.username.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if existing.is_some() {
        return Err(AppError::Conflict("username already taken".into()));
    }

    // Hash password.
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("hash error: {e}")))?
        .to_string();

    let display_name = req.display_name.unwrap_or_else(|| req.username.clone());

    // Create user record.
    let created: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE user CONTENT { \
              username: $u, display_name: $d, \
              password_hash: $h, created_at: time::now() \
            } RETURN id",
        )
        .bind(("u", req.username.clone()))
        .bind(("d", display_name))
        .bind(("h", hash))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let user_id = created
        .first()
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| AppError::Internal("failed to get user id".into()))?;

    let device_id = create_device(&state, &user_id, req.device_name.as_deref(), None, None).await?;
    let token = Claims::encode(
        &user_id,
        &device_id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_secs,
    )?;

    info!("New user signed up: {}", req.username);
    Ok(Json(AuthResponse {
        token,
        user_id,
        device_id,
    }))
}

/// `POST /auth/signin` — authenticate + return token.
async fn signin(
    State(state): State<AppState>,
    Json(req): Json<SigninRequest>,
) -> Result<Json<AuthResponse>> {
    // Fetch user by username.
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM user WHERE username = $u LIMIT 1")
        .bind(("u", req.username.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let user: UserRecord = raw
        .map(|v| {
            serde_json::from_value::<UserRecord>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
        .ok_or(AppError::Unauthorized)?;

    // Verify password.
    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|e| AppError::Internal(format!("hash parse: {e}")))?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::Unauthorized)?;

    let user_id = user
        .id
        .clone()
        .ok_or_else(|| AppError::Internal("missing user id".into()))?;

    let device_id = create_device(
        &state,
        &user_id,
        req.device_name.as_deref(),
        req.user_agent.as_deref(),
        None,
    )
    .await?;

    let token = Claims::encode(
        &user_id,
        &device_id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_secs,
    )?;

    info!("User signed in: {}", req.username);
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
    let raw: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM device WHERE owner = type::thing($id) ORDER BY last_seen DESC")
        .bind(("id", auth.user_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let devices = raw
        .into_iter()
        .map(|v| serde_json::from_value::<Device>(v).map_err(|e| AppError::Internal(e.to_string())))
        .collect::<Result<Vec<_>>>()?;

    Ok(Json(devices))
}

/// `DELETE /auth/devices/:device_id` — revoke a specific device.
async fn revoke_device(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // Verify ownership.
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($id) LIMIT 1")
        .bind(("id", device_id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let device: Device = raw
        .map(|v| serde_json::from_value::<Device>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Insert a new `device` record and return its string ID.
async fn create_device(
    state: &AppState,
    user_id: &str,
    device_name: Option<&str>,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> Result<String> {
    let name = device_name.unwrap_or("Unknown device");
    let created: Vec<serde_json::Value> = state
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
            } RETURN id",
        )
        .bind(("uid", user_id.to_owned()))
        .bind(("name", name.to_owned()))
        .bind(("ua", user_agent.map(str::to_owned)))
        .bind(("ip", ip.map(str::to_owned)))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    extract_id(&created)
}

/// Extract a record ID string from a `RETURN id` SurrealQL result.
fn extract_id(rows: &[serde_json::Value]) -> Result<String> {
    rows.first()
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| AppError::Internal("failed to get record id".into()))
}
