//! Shared infrastructure for Poly test server suite.
//!
//! Provides dynamic port binding, CLI args, health/reset/seed route builders,
//! and simple opaque token auth helpers used by all mock test servers.

mod auth;
mod cli;
mod server;

pub use auth::{AuthState, TokenAuth};
pub use cli::CliArgs;
pub use server::TestServerBase;

use axum::response::IntoResponse;
use axum::Json;

/// Standard `/health` response.
pub async fn health_handler(backend: &'static str) -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "backend": backend }))
}
