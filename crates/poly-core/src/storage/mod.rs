//! Platform-transparent storage abstraction for Poly.
//!
//! Provides a unified key/value storage interface with typed convenience
//! methods for all persisted data (settings, identity, account tokens, etc.).
//!
//! ## Backend selection
//!
//! | Target | Backend | Notes |
//! |---|---|---|
//! | Native (Linux/macOS/Windows) | SurrealDB (SurrealKV) | Same as main app |
//! | WebAssembly | `localStorage` via `gloo-storage` | Persistent across page reloads |
//!
//! ## Usage
//!
//! ```rust,ignore
//! // At app startup (in App component):
//! let storage = Storage::init().await?;
//!
//! // Read typed settings:
//! let settings = storage.get_app_settings().await?;
//!
//! // Write typed settings:
//! storage.set_app_settings(&AppSettings {
//!     setup_complete: true,
//!     account_id: "abc123".into(),
//!     locale: "en".into(),
//! }).await?;
//! ```
//!
//! DECISION(DX-STORAGE-1): Unified trait pattern — same call sites work whether
//! compiled to native or WASM. No feature flags required at call sites.

use serde::{Deserialize, Serialize};

// ── Platform backends ─────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
use native::StorageInner;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
use web::StorageInner;

// ── Error type ────────────────────────────────────────────────────────────────

/// Storage operation error.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Backend-specific error (SurrealDB or Web API error).
    #[error("storage backend error: {0}")]
    Backend(String),

    /// (De)serialization error.
    #[error("serialization error: {0}")]
    Serde(String),
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e.to_string())
    }
}

// ── Typed data models ─────────────────────────────────────────────────────────

/// Top-level application settings.
///
/// Persisted under the key `"app_settings"`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    /// Whether the setup wizard has been completed.
    pub setup_complete: bool,
    /// BIP39 account ID (hex-encoded public key).
    pub account_id: String,
    /// Selected locale (e.g. `"en"`, `"de"`).
    pub locale: String,
    /// Selected theme preset (e.g. `"neutral-dark"`, `"purple"`, `"red"`).
    pub theme: String,
}

/// A stored messenger account credential/token.
///
/// Persisted under the key `"account:{backend}:{id}"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountToken {
    /// Backend identifier (`"stoat"`, `"matrix"`, `"discord"`, …).
    pub backend: String,
    /// Account ID within that backend.
    pub account_id: String,
    /// Auth token / session token.
    pub token: String,
    /// Display name.
    pub display_name: String,
}

// ── Storage handle ────────────────────────────────────────────────────────────

/// Platform-transparent storage handle.
///
/// Cheap to clone — backed by `Arc` internally on native,
/// by a zero-size-type on WASM (localStorage is a browser global).
#[derive(Clone)]
pub struct Storage(StorageInner);

impl Storage {
    /// Initialize the storage backend.
    ///
    /// * **Native**: opens (or creates) the SurrealKV database in the platform
    ///   data directory (`~/.local/share/poly` on Linux etc.).
    /// * **WASM**: no-op — `localStorage` is always available.
    pub async fn init() -> Result<Self, StorageError> {
        Ok(Self(StorageInner::init().await?))
    }

    // ── Raw KV access ─────────────────────────────────────────────────────────

    /// Get a value by key. Returns `None` if the key is not present.
    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        self.0.get(key).await
    }

    /// Set a value by key (upsert semantics).
    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        self.0.set(key, value).await
    }

    /// Delete a key. No-op if the key does not exist.
    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.0.delete(key).await
    }

    // ── Typed access — AppSettings ────────────────────────────────────────────

    /// Read application settings, returning [`AppSettings::default`] if not yet set.
    pub async fn get_app_settings(&self) -> Result<AppSettings, StorageError> {
        Ok(self
            .get("app_settings")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    /// Persist application settings.
    pub async fn set_app_settings(&self, settings: &AppSettings) -> Result<(), StorageError> {
        self.set("app_settings", serde_json::to_value(settings)?).await
    }

    // ── Typed access — AccountToken ───────────────────────────────────────────

    /// List all stored account tokens.
    pub async fn get_account_tokens(&self) -> Result<Vec<AccountToken>, StorageError> {
        Ok(self
            .get("account_tokens")
            .await?
            .and_then(|v| serde_json::from_value::<Vec<AccountToken>>(v).ok())
            .unwrap_or_default())
    }

    /// Persist (upsert) an account token. Identified by `backend + account_id`.
    pub async fn upsert_account_token(&self, token: &AccountToken) -> Result<(), StorageError> {
        let mut tokens = self.get_account_tokens().await?;
        tokens.retain(|t| !(t.backend == token.backend && t.account_id == token.account_id));
        tokens.push(token.clone());
        self.set("account_tokens", serde_json::to_value(&tokens)?).await
    }

    /// Remove an account token.
    pub async fn remove_account_token(&self, backend: &str, account_id: &str) -> Result<(), StorageError> {
        let mut tokens = self.get_account_tokens().await?;
        tokens.retain(|t| !(t.backend == backend && t.account_id == account_id));
        self.set("account_tokens", serde_json::to_value(&tokens)?).await
    }
}
