#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]
//! Shared infrastructure for Poly test server suite.
//!
//! Provides dynamic port binding, CLI args, health/reset/seed route builders,
//! simple opaque token auth helpers, and event broadcast infrastructure used
//! by all mock test servers.
//!
//! ## Lifecycle Endpoints
//!
//! Every test server exposes three lifecycle endpoints:
//! - **`POST /seed`** — populate demo data (idempotent, skips if already present)
//! - **`POST /reset`** — wipe all data to empty state
//! - **`POST /reseed`** — wipe + re-seed in one call (most common between test runs)
//!
//! ## Event Broadcast
//!
//! All backends need real-time event delivery (messages, typing, presence).
//! The shared [`EventBus`] wraps a `tokio::sync::broadcast` channel so that:
//! - REST handlers publish events when state changes (e.g. message sent)
//! - WebSocket handlers / long-poll `/sync` endpoints subscribe and receive them
//!
//! Each backend defines its own event enum but uses the same broadcast plumbing.

pub mod avatars;
mod auth;
mod broadcast;
mod cli;
pub mod inspect;
mod server;

pub use auth::{AuthState, TokenAuth, wipe_persisted};
pub use avatars::serve_animal;
pub use broadcast::EventBus;
pub use cli::CliArgs;
pub use inspect::{
    HEADER_INSPECT_CAP, HeaderEntry, HeaderInspectBuffer, handle_inspect_last_headers,
    header_inspect_middleware,
};
pub use server::TestServerBase;

use axum::response::IntoResponse;
use axum::Json;

/// Standard `/health` response.
pub async fn health_handler(backend: &'static str) -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "backend": backend }))
}
