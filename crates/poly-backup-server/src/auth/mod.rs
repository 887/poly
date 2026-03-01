//! Authentication endpoints — PoW challenge + passphrase verification.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::AppState;

/// PoW challenge response.
#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub id: String,
    pub data: String,
    pub difficulty: u32,
}

/// Auth request body.
#[derive(Debug, Deserialize)]
pub struct AuthRequestBody {
    pub pow_solution: PowSolution,
    pub passphrase: String,
    pub public_key: String,
    pub device_info: DeviceInfo,
}

/// PoW solution from client.
#[derive(Debug, Deserialize)]
pub struct PowSolution {
    pub challenge_id: String,
    pub nonce: u64,
}

/// Device information.
#[derive(Debug, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub platform: String,
    pub version: String,
}

/// Auth response.
#[derive(Debug, Serialize)]
pub struct AuthResponseBody {
    pub token: String,
    pub expires_at: Option<String>,
}

/// Request a PoW challenge.
pub async fn request_challenge(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Json<ChallengeResponse> {
    let state = state.read().await;
    let challenge_id = uuid::Uuid::new_v4().to_string();
    let challenge_data = format!(
        "poly-challenge-{}-{}",
        challenge_id,
        chrono::Utc::now().timestamp()
    );

    Json(ChallengeResponse {
        id: challenge_id,
        data: challenge_data,
        difficulty: state.config.pow_difficulty,
    })
}

/// Authenticate with PoW solution + passphrase.
pub async fn authenticate(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(body): Json<AuthRequestBody>,
) -> Result<Json<AuthResponseBody>, axum::http::StatusCode> {
    let state = state.read().await;

    // 1. Verify passphrase
    if body.passphrase != state.config.passphrase {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    // 2. Verify PoW (simplified — in production, store challenges and verify)
    // TODO(phase-2.8.3): Store challenges server-side for proper verification
    let mut hasher = Sha256::new();
    // We'd need the original challenge data here
    // For now, accept any valid solution
    hasher.update(body.pow_solution.nonce.to_le_bytes());
    let _hash = hasher.finalize();

    // 3. Generate session token
    let token = generate_token();

    // TODO(phase-2.8.7): Store token with device info, track sessions

    Ok(Json(AuthResponseBody {
        token,
        expires_at: None,
    }))
}

/// Generate a random session token (128+ hex characters).
fn generate_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
