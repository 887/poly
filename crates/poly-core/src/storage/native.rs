//! Native (non-WASM) storage backend — SurrealDB with SurrealKV.
//!
//! Uses the same SurrealKV engine that the main Poly app uses, ensuring
//! the data written by desktop-devtools is readable by the production app.
//!
//! Storage location (follows XDG / platform conventions):
//! - Linux:   `$XDG_DATA_HOME/poly/storage.db`  (default: `~/.local/share/poly/storage.db`)
//! - macOS:   `~/Library/Application Support/poly/storage.db`
//! - Windows: `%APPDATA%\poly\storage.db`
//!
//! # Implementation notes
//!
//! Uses raw SurrealQL `.query()` calls rather than the typed SDK (`db.select`, `db.upsert`)
//! because user-defined types require `#[derive(SurrealValue)]` from an internal surrealdb
//! proc-macro crate not exposed to downstream consumers. The `.query()` path avoids that
//! restriction entirely.
//!
//! `serde_json::Value` implements the internal `SurrealValue` trait, so we use it
//! in `.take()` calls to extract results from query `Response` objects.
//!
//! Storage schema:
//!   Table `poly_kv`, each record = `poly_kv:<key>`, field `payload` stores the
//!   raw JSON string of the value (double-serialized, mirrors the WASM localStorage approach).

use std::sync::Arc;

use surrealdb::{
    Surreal,
    engine::local::{Db, SurrealKv},
};

use super::StorageError;

// ── StorageInner ──────────────────────────────────────────────────────────────

/// SurrealDB-backed storage inner.
///
/// Uses raw SurrealQL `.query()` calls — avoids the `SurrealValue` derive
/// requirement that the typed SDK imposes on all record types.
#[derive(Clone)]
pub struct StorageInner {
    db: Arc<Surreal<Db>>,
}

impl StorageInner {
    /// Open (or create) the SurrealKV store in the platform data directory.
    pub async fn init() -> Result<Self, StorageError> {
        let path = poly_data_dir().join("storage.db");
        std::fs::create_dir_all(path.parent().unwrap())
            .map_err(|e| StorageError::Backend(format!("cannot create data dir: {e}")))?;

        let path_str = path.to_string_lossy().to_string();
        tracing::info!("Opening SurrealKV storage at: {path_str}");

        let db: Surreal<Db> = Surreal::new::<SurrealKv>(&*path_str)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        db.use_ns("poly")
            .use_db("main")
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        tracing::info!("SurrealKV storage ready ✓");
        Ok(Self { db: Arc::new(db) })
    }

    /// Get raw JSON value by key.
    ///
    /// Uses `SELECT payload FROM poly_kv:<key>` — the record-ID syntax guarantees
    /// point lookup without a table scan.
    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        // `key` is always a controlled literal (e.g. "app_settings") — never user input.
        let query = format!("SELECT payload FROM poly_kv:{key}");
        let mut resp = self
            .db
            .query(&query)
            .await
            .map_err(|e| StorageError::Backend(format!("query({key}): {e}")))?;

        // `.take("payload")` extracts the "payload" field from the first result record.
        // Returns None if the record doesn't exist.
        let raw: Option<String> = resp
            .take::<Option<String>>("payload")
            .map_err(|e| StorageError::Backend(format!("take payload ({key}): {e}")))?;

        tracing::debug!("storage::get({key}) → {raw:?}");

        match raw {
            None => Ok(None),
            Some(s) => {
                let val: serde_json::Value =
                    serde_json::from_str(&s).map_err(|e| StorageError::Serde(e.to_string()))?;
                Ok(Some(val))
            }
        }
    }

    /// Upsert raw JSON value by key.
    ///
    /// Uses `UPSERT poly_kv:<key> SET payload = $payload` — record-ID syntax creates
    /// or replaces exactly one record, the canonical SurrealDB 3.0 upsert pattern.
    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        let serialized =
            serde_json::to_string(&value).map_err(|e| StorageError::Serde(e.to_string()))?;

        tracing::debug!("storage::set({key}) serialized_len={}", serialized.len());

        // NOTE: We renamed the field to `payload` (from `value`) to avoid any potential
        // SurrealDB keyword collision with the `VALUE` keyword used in expressions.
        let query = format!("UPSERT poly_kv:{key} SET payload = $payload");
        // Bind as a JSON object — serde_json::Value implements SurrealValue, and
        // Value::Object is treated as a variables map by IntoVariables.
        let mut resp = self
            .db
            .query(&query)
            .bind(serde_json::json!({ "payload": serialized }))
            .await
            .map_err(|e| StorageError::Backend(format!("upsert({key}): {e}")))?;

        // Consume result index 0 to surface any SurrealQL-level errors.
        // The UPSERT returns the upserted record, so we check it exists and discard it.
        let result: Option<serde_json::Value> = resp
            .take::<Option<serde_json::Value>>(0usize)
            .map_err(|e| StorageError::Backend(format!("upsert result ({key}): {e}")))?;

        if result.is_none() {
            tracing::warn!("storage::set({key}): UPSERT returned no record — possible issue");
        } else {
            tracing::info!("storage::set({key}) committed ✓");
        }

        Ok(())
    }

    /// Delete a key from storage. No-op if not present.
    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let query = format!("DELETE poly_kv:{key}");
        let mut resp = self
            .db
            .query(&query)
            .await
            .map_err(|e| StorageError::Backend(format!("delete({key}): {e}")))?;
        // Consume result to surface errors.
        let _ = resp.take::<Option<serde_json::Value>>(0usize);
        Ok(())
    }
}

// ── Data directory resolution ─────────────────────────────────────────────────

/// Return the Poly data directory path for the current platform.
///
/// Matches the path that `reset_app` in the MCP server removes:
/// `~/.local/share/poly` on Linux.
fn poly_data_dir() -> std::path::PathBuf {
    #[cfg(target_os = "linux")]
    {
        let base: std::path::PathBuf = std::env::var("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                std::path::PathBuf::from(home).join(".local").join("share")
            });
        base.join("poly")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::Path::new(&home)
            .join("Library")
            .join("Application Support")
            .join("poly")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        std::path::Path::new(&appdata).join("poly")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        std::path::PathBuf::from(".poly")
    }
}
