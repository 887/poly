//! # poly-core
//!
//! The shared library crate for the Poly messenger.
//!
//! Contains all UI components, application state, database abstractions,
//! internationalization, theme engine, cryptography, and backup sync logic.
//!
//! **This crate MUST support Dioxus subsecond hot-reload via `dx serve --hotpatch`.**
//!
//! ## Feature Flags
//!
//! - `demo` — Include the demo/mock client (default, for UI testing)
//! - `stoat` — Include the Stoat (Revolt) client
//! - `matrix` — Include the Matrix client
//! - `discord` — Include the Discord client
//! - `teams` — Include the Microsoft Teams client

pub mod crypto;
// Legacy database module (native-only; superseded by `storage` for new code).
#[cfg(not(target_arch = "wasm32"))]
pub mod db;
pub mod i18n;
pub mod state;
pub mod storage;
pub mod sync;
pub mod theme;
pub mod ui;

// Re-export the client trait crate
pub use poly_client;

/// Global storage handle — initialised exactly once at app startup.
///
/// Access via `poly_core::STORAGE.get()`. Returns `None` until initialised.
///
/// DECISION(DX-STORAGE-3): OnceLock global mirrors the eval-bridge pattern
/// already used in desktop-devtools. Components and event handlers can call
/// storage without prop-drilling or context gymnastics.
pub static STORAGE: std::sync::OnceLock<storage::Storage> = std::sync::OnceLock::new();

/// Initialize the Poly application.
///
/// This sets up all core subsystems: database, i18n, theme, and crypto.
/// Called once at application startup from each platform's `main.rs`.
pub async fn init() -> anyhow::Result<()> {
    tracing::info!("Initializing Poly core...");

    // Initialize i18n with system locale
    i18n::init();

    // Initialize theme engine with default theme
    theme::init();

    tracing::info!("Poly core initialized successfully");
    Ok(())
}
