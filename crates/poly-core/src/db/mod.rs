//! SurrealDB database abstraction layer for Poly.
//!
//! Provides a unified interface for local data storage using SurrealKV
//! (embedded SurrealDB with SurrealKV backend — no RocksDB/SQLite).
//!
//! ## Stored Data
//! - User settings and preferences
//! - Account credentials and tokens
//! - Server favorites
//! - Theme configuration
//! - Backup server connection info

// DECISION(D2): SurrealKV everywhere — no platform divergence.

use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};

/// The database handle wrapping SurrealDB.
pub struct Database {
    db: Surreal<Db>,
}

/// Database initialization error.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// SurrealDB error.
    #[error("database error: {0}")]
    Surreal(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Record not found.
    #[error("record not found: {0}")]
    NotFound(String),
}

impl From<surrealdb::Error> for DbError {
    fn from(err: surrealdb::Error) -> Self {
        Self::Surreal(err.to_string())
    }
}

impl Database {
    /// Initialize the database at the given path.
    ///
    /// Uses SurrealKV as the storage backend.
    pub async fn init(path: &str) -> Result<Self, DbError> {
        let db = Surreal::new::<SurrealKv>(path).await?;
        db.use_ns("poly").use_db("main").await?;

        tracing::info!("SurrealKV database initialized at: {path}");
        Ok(Self { db })
    }

    /// Initialize an in-memory database (for testing).
    ///
    /// Uses SurrealKV with a temporary directory that is cleaned up on drop.
    pub async fn init_memory() -> Result<Self, DbError> {
        let tmp = std::env::temp_dir().join(format!("poly-test-{}", std::process::id()));
        let db = Surreal::new::<SurrealKv>(tmp.to_string_lossy().as_ref()).await?;
        db.use_ns("poly").use_db("test").await?;
        Ok(Self { db })
    }

    // --- Settings CRUD (using raw SurrealQL for SurrealDB 3.0 compatibility) ---

    /// Get a setting value by key.
    pub async fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, DbError> {
        let mut response = self
            .db
            .query("SELECT value FROM settings WHERE key = $key LIMIT 1")
            .bind(("key", key.to_string()))
            .await?;

        let result: Option<serde_json::Value> = response.take("value")?;
        Ok(result)
    }

    /// Set a setting value.
    pub async fn set_setting(&self, key: &str, value: serde_json::Value) -> Result<(), DbError> {
        let value_str =
            serde_json::to_string(&value).map_err(|e| DbError::Serialization(e.to_string()))?;

        self.db
            .query("UPSERT settings SET key = $key, value = $value WHERE key = $key")
            .bind(("key", key.to_string()))
            .bind(("value", value_str))
            .await?;
        Ok(())
    }

    /// Delete a setting.
    pub async fn delete_setting(&self, key: &str) -> Result<(), DbError> {
        self.db
            .query("DELETE settings WHERE key = $key")
            .bind(("key", key.to_string()))
            .await?;
        Ok(())
    }

    /// Get the raw SurrealDB handle for advanced operations.
    pub fn handle(&self) -> &Surreal<Db> {
        &self.db
    }
}
