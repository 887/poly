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
//! ## Authentication
//!
//! Authentication uses Ed25519 challenge-response (no passwords):
//!
//! 1. **Signup**: Send hex-encoded Ed25519 public key + username → receive JWT
//! 2. **Signin**: Request challenge nonce → sign with Ed25519 key → submit → receive JWT
//!
//! The JWT is automatically included in all subsequent API requests.

pub mod backend;
pub mod error;
pub mod http;
pub mod models;
pub mod ws;

pub use backend::PolyServerBackend;
pub use error::{PolyServerError, Result};
pub use http::{PolyServerConfig, PolyServerHttpClient, SessionState};
pub use models::*;
pub use ws::PolyServerWsClient;
