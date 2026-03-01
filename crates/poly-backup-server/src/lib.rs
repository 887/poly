//! poly-backup-server — Encrypted backup sync server for Poly.
//!
//! An Axum-based REST API server that stores encrypted settings blobs
//! identified by Ed25519 public keys. The server never sees plaintext data.
//!
//! ## API Endpoints
//! - `GET /api/challenge` — Request PoW challenge
//! - `POST /api/auth` — Authenticate with PoW solution + passphrase
//! - `POST /api/sync/push` — Push encrypted settings
//! - `GET /api/sync/pull?since=N` — Pull settings changes since sequence N

pub mod auth;
pub mod sync;
pub mod web;

use axum::{
    Json, Router,
    routing::{get, post},
};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Server configuration from environment variables.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server-wide access passphrase.
    pub passphrase: String,
    /// Maximum number of user accounts (0 = unlimited).
    pub max_accounts: u32,
    /// Token expiry in days of inactivity.
    pub token_expiry_days: u32,
    /// PoW challenge difficulty (number of leading zero bits).
    pub pow_difficulty: u32,
    /// Server bind address.
    pub bind_addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            passphrase: "changeme".to_string(),
            max_accounts: 0,
            token_expiry_days: 365,
            pow_difficulty: 16,
            bind_addr: "0.0.0.0:3000".to_string(),
        }
    }
}

impl ServerConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        Self {
            passphrase: std::env::var("POLY_PASSPHRASE").unwrap_or_else(|_| "changeme".to_string()),
            max_accounts: std::env::var("POLY_MAX_ACCOUNTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            token_expiry_days: std::env::var("POLY_TOKEN_EXPIRY_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(365),
            pow_difficulty: std::env::var("POLY_POW_DIFFICULTY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(16),
            bind_addr: std::env::var("POLY_BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
        }
    }
}

/// Shared server state.
pub struct AppState {
    pub config: ServerConfig,
    // TODO(phase-2.8.11): Add SurrealDB handle
    // TODO(phase-2.8.7): Add token store
}

/// Create the Axum router with all API routes.
pub fn create_router(state: Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .route("/api/health", get(health_check))
        .route("/api/challenge", get(auth::request_challenge))
        .route("/api/auth", post(auth::authenticate))
        .route("/api/sync/push", post(sync::push))
        .route("/api/sync/pull", get(sync::pull))
        .with_state(state)
}

/// Health check endpoint.
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
