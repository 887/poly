//! Native (non-WASM) storage backend — SurrealDB with SurrealKV.
//!
//! This backend is opt-in behind the `storage-surreal` feature. The default
//! native backend is SQLite to keep `cargo check` lighter.

use std::sync::Arc;

use surrealdb::{
    Surreal,
    engine::local::{Db, SurrealKv},
};

use super::StorageError;

#[derive(Clone)]
pub struct StorageInner {
    db: Arc<Surreal<Db>>,
}

impl StorageInner {
    pub async fn init() -> Result<Self, StorageError> {
        let path = super::poly_data_dir().join("storage.surrealkv");
        std::fs::create_dir_all(super::poly_data_dir())
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

        tracing::info!("SurrealKV storage ready");
        Ok(Self { db: Arc::new(db) })
    }

    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let query = format!("SELECT payload FROM poly_kv:{key}");
        let mut resp = self
            .db
            .query(&query)
            .await
            .map_err(|e| StorageError::Backend(format!("query({key}): {e}")))?;

        let raw: Option<String> = resp
            .take::<Option<String>>("payload")
            .map_err(|e| StorageError::Backend(format!("take payload ({key}): {e}")))?;

        match raw {
            None => Ok(None),
            Some(s) => {
                let val =
                    serde_json::from_str(&s).map_err(|e| StorageError::Serde(e.to_string()))?;
                Ok(Some(val))
            }
        }
    }

    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        let serialized =
            serde_json::to_string(&value).map_err(|e| StorageError::Serde(e.to_string()))?;

        let query = format!("UPSERT poly_kv:{key} SET payload = $payload RETURN NONE");
        let mut resp = self
            .db
            .query(&query)
            .bind(serde_json::json!({ "payload": serialized }))
            .await
            .map_err(|e| StorageError::Backend(format!("upsert({key}): {e}")))?;

        let _ = resp
            .take::<Option<serde_json::Value>>(0usize)
            .map_err(|e| StorageError::Backend(format!("upsert result ({key}): {e}")))?;

        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let query = format!("DELETE poly_kv:{key}");
        let mut resp = self
            .db
            .query(&query)
            .await
            .map_err(|e| StorageError::Backend(format!("delete({key}): {e}")))?;
        let _ = resp.take::<Option<serde_json::Value>>(0usize);
        Ok(())
    }

    pub async fn clear_all(&self) -> Result<(), StorageError> {
        let mut resp = self
            .db
            .query("DELETE poly_kv")
            .await
            .map_err(|e| StorageError::Backend(format!("clear_all: {e}")))?;
        let _ = resp.take::<Option<serde_json::Value>>(0usize);
        Ok(())
    }
}
