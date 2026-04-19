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

/// WIT bindings for the WASM plugin (WASI targets only).
/// This module isolates the `wit-bindgen` macros for FFI.
#[cfg(target_os = "wasi")]
mod wit_bindings;

/// WASM plugin guest implementation (WASI targets only).
#[cfg(target_os = "wasi")]
mod guest;

/// [`ClientBackend`](poly_client::ClientBackend) implementation (native + wasm-http, non-WASI).
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub mod backend;
/// Error types (native + wasm-http, non-WASI).
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub mod error;
/// HTTP REST client (native + wasm-http, non-WASI).
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub mod http;
/// Wire-format models matching poly-server JSON payloads (native + wasm-http, non-WASI).
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub mod models;
/// Dioxus signup page component (native + wasm-http; gated on wasm-http since native implies it).
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub mod signup;
/// WebSocket client for real-time events (native only — requires tokio-tungstenite).
#[cfg(all(feature = "native", not(target_os = "wasi")))]
pub mod ws;

#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub use backend::PolyServerBackend;
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub use error::{PolyServerError, Result};
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub use http::{PolyServerConfig, PolyServerHttpClient, SessionState};
#[cfg(all(feature = "wasm-http", not(target_os = "wasi")))]
pub use models::*;
#[cfg(all(feature = "native", not(target_os = "wasi")))]
pub use ws::PolyServerWsClient;

// ─── Native plugin metadata ─────────────────────────────────────────────────
//
// Mirrors the WIT `plugin-metadata.get-translations` interface for native
// (non-WASM) builds.  `poly-core` calls this free function at startup via
// `i18n::register_native_plugin_ftl()`, mirroring how the WASM plugin host
// calls `get-translations(locale)` on WASM components at instantiation time.
// FTL files are owned by this crate, not core.

/// Return the raw FTL translation source for the Poly Server client plugin.
///
/// Mirrors the WIT `plugin-metadata.get-translations(locale) → string` export.
/// Returns an empty string for unsupported locales so the host falls back to
/// English (same contract as the WIT interface).
pub fn plugin_translations(locale: &str) -> String {
    match locale {
        "de" => include_str!("../locales/de/plugin.ftl").to_string(),
        "fr" => include_str!("../locales/fr/plugin.ftl").to_string(),
        "es" => include_str!("../locales/es/plugin.ftl").to_string(),
        "en" => include_str!("../locales/en/plugin.ftl").to_string(),
        _ => String::new(),
    }
}
