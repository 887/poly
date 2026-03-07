//! WASM storage backend — browser `localStorage` via `gloo-storage`.
//!
//! `localStorage` persists across page reloads and browser sessions until
//! explicitly cleared. It is synchronous under the hood; we expose an `async`
//! interface to match the native backend API.
//!
//! Capacity: 5–10 MB (more than enough for settings, tokens, and identity).
//!
//! DECISION(DX-STORAGE-2): `localStorage` chosen over IndexedDB for the initial
//! WASM implementation because:
//!   - Zero setup — no schema migrations, no object-store definitions.
//!   - Synchronous underlying API wraps trivially into async signatures.
//!   - Our storage payload (settings, account tokens) is well under 1 MB.
//!
//! Full IndexedDB support (for caching messages, etc.) is planned for Phase 3.

use super::StorageError;

/// Zero-size-type — `localStorage` is a browser global, no handle needed.
#[derive(Clone)]
pub struct StorageInner;

// `StorageInner` is a unit struct with no fields, so Rust automatically
// provides `Send + Sync` — no manual impls needed.

impl StorageInner {
    /// No-op initialisation — `localStorage` is always available.
    pub async fn init() -> Result<Self, StorageError> {
        Ok(Self)
    }

    /// Get a raw JSON value from `localStorage`.
    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        use gloo_storage::errors::StorageError as GlooErr;
        use gloo_storage::{LocalStorage, Storage as _};

        match LocalStorage::get::<serde_json::Value>(key) {
            Ok(val) => Ok(Some(val)),
            Err(GlooErr::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(StorageError::Backend(e.to_string())),
        }
    }

    /// Set a raw JSON value in `localStorage` (upsert semantics).
    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        use gloo_storage::{LocalStorage, Storage as _};

        LocalStorage::set(key, &value).map_err(|e| StorageError::Backend(e.to_string()))
    }

    /// Remove a key from `localStorage`. No-op if not present.
    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        use gloo_storage::{LocalStorage, Storage as _};
        LocalStorage::delete(key);
        Ok(())
    }

    /// Clear all localStorage data.
    pub async fn clear_all(&self) -> Result<(), StorageError> {
        use gloo_storage::{LocalStorage, Storage as _};
        LocalStorage::clear();
        Ok(())
    }
}
