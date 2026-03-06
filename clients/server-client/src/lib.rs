//! # poly-server-client
//!
//! [`ClientBackend`](poly_client::ClientBackend) implementation for poly-server.
//!
//! This crate provides the complete client for connecting to poly-server
//! instances:
//!
//! - **[`PolyServerHttpClient`]** — Typed HTTP client for all REST API endpoints
//! - **[`PolyServerWsClient`]** — WebSocket client for real-time events
//! - **[`PolyServerBackend`]** — [`ClientBackend`](poly_client::ClientBackend) implementation
//! - **[`models`]** — Wire-format types matching the server's JSON payloads
//!
//! ## Build Modes
//!
//! - **Native** (`--features native`): Full HTTP/WS/Ed25519 functionality.
//! - **WASM plugin** (target `wasm32-wasip2`): Stub WIT export (real impl TBD).
//!
//! DECISION(D21): WASM Plugin Backends.
//!
//! ## Authentication
//!
//! Authentication uses Ed25519 challenge-response (no passwords):
//!
//! 1. **Signup**: Send hex-encoded Ed25519 public key + username → receive JWT
//! 2. **Signin**: Request challenge nonce → sign with Ed25519 key → submit → receive JWT
//!
//! The JWT is automatically included in all subsequent API requests.

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

#[cfg(feature = "native")]
pub mod backend;
#[cfg(feature = "native")]
pub mod error;
#[cfg(feature = "native")]
pub mod http;
#[cfg(feature = "native")]
pub mod models;
#[cfg(feature = "native")]
pub mod ws;

#[cfg(feature = "native")]
pub use backend::PolyServerBackend;
#[cfg(feature = "native")]
pub use error::{PolyServerError, Result};
#[cfg(feature = "native")]
pub use http::{PolyServerConfig, PolyServerHttpClient, SessionState};
#[cfg(feature = "native")]
pub use models::*;
#[cfg(feature = "native")]
pub use ws::PolyServerWsClient;
