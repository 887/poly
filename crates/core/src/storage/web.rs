//! WASM storage backend — browser IndexedDB.

use super::StorageError;

use indexed_db_futures::{
    KeyPath,
    database::Database,
    prelude::{Build, BuildPrimitive, BuildSerde, QuerySource},
    transaction::TransactionMode,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct StorageInner;

const DB_NAME: &str = "poly-storage";
const STORE_NAME: &str = "kv";

#[derive(Debug, Serialize, Deserialize)]
struct StoredRecord {
    key: String,
    payload: serde_json::Value,
}

fn backend_error<E: std::fmt::Display>(error: E) -> StorageError {
    StorageError::Backend(error.to_string())
}

async fn open_db() -> Result<Database, StorageError> {
    Database::open(DB_NAME)
        .with_version(1u32)
        .with_on_upgrade_needed(|event, db| {
            if event.old_version() < 1.0 {
                db.create_object_store(STORE_NAME)
                    .with_key_path(KeyPath::from("key"))
                    .build()?;
            }
            Ok(())
        })
        .await
        .map_err(backend_error)
}

impl StorageInner {
    pub async fn init() -> Result<Self, StorageError> {
        let db = open_db().await?;
        db.close();
        Ok(Self)
    }

    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let db = open_db().await?;
        let record = {
            let tx = db.transaction(STORE_NAME).build().map_err(backend_error)?;
            let store = tx.object_store(STORE_NAME).map_err(backend_error)?;
            let record: Option<StoredRecord> = store
                .get(key)
                .serde()
                .map_err(backend_error)?
                .await
                .map_err(backend_error)?;
            record
        };
        db.close();
        Ok(record.map(|record| record.payload))
    }

    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        let db = open_db().await?;
        let tx = db
            .transaction(STORE_NAME)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(backend_error)?;
        let store = tx.object_store(STORE_NAME).map_err(backend_error)?;
        store
            .put(StoredRecord {
                key: key.to_string(),
                payload: value,
            })
            .without_key_type()
            .serde()
            .map_err(backend_error)?
            .await
            .map_err(backend_error)?;
        tx.commit().await.map_err(backend_error)?;
        db.close();
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let db = open_db().await?;
        let tx = db
            .transaction(STORE_NAME)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(backend_error)?;
        let store = tx.object_store(STORE_NAME).map_err(backend_error)?;
        store
            .delete(key)
            .primitive()
            .map_err(backend_error)?
            .await
            .map_err(backend_error)?;
        tx.commit().await.map_err(backend_error)?;
        db.close();
        Ok(())
    }

    pub async fn clear_all(&self) -> Result<(), StorageError> {
        let db = open_db().await?;
        let tx = db
            .transaction(STORE_NAME)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .map_err(backend_error)?;
        let store = tx.object_store(STORE_NAME).map_err(backend_error)?;
        store.clear().map_err(backend_error)?.await.map_err(backend_error)?;
        tx.commit().await.map_err(backend_error)?;
        db.close();
        Ok(())
    }
}
