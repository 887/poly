//! Native (non-WASM) storage backend — SQLite.
//!
//! This is the default native backend so `poly-core` can compile without pulling
//! in SurrealDB unless it is explicitly requested.

use std::sync::{Arc, Mutex};

use sqlite::{Connection, ConnectionThreadSafe, State};

use super::StorageError;
#[derive(Clone)]
pub struct StorageInner {
    db: Arc<Mutex<ConnectionThreadSafe>>,
}

impl StorageInner {
    pub async fn init() -> Result<Self, StorageError> {
        Self::open(super::poly_data_dir()).await
    }

    #[cfg(test)]
    pub async fn init_with_path(base_dir: std::path::PathBuf) -> Result<Self, StorageError> {
        Self::open(base_dir).await
    }

    async fn open(base_dir: std::path::PathBuf) -> Result<Self, StorageError> {
        let path = base_dir.join("storage.sqlite3");
        std::fs::create_dir_all(&base_dir)
            .map_err(|e| StorageError::Backend(format!("cannot create data dir: {e}")))?;

        let mut db = Connection::open_thread_safe(&path)
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        db.set_busy_timeout(5_000)
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS poly_kv (key TEXT PRIMARY KEY NOT NULL, payload TEXT NOT NULL)",
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;

        tracing::info!("SQLite storage ready at: {}", path.to_string_lossy());
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }

    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let db = self
            .db
            .lock()
            .map_err(|_e| StorageError::Backend("sqlite mutex poisoned".to_string()))?;

        let mut statement = db
            .prepare("SELECT payload FROM poly_kv WHERE key = ?1 LIMIT 1")
            .map_err(|e| StorageError::Backend(format!("prepare get({key}): {e}")))?;
        statement
            .bind((1, key))
            .map_err(|e| StorageError::Backend(format!("bind get({key}): {e}")))?;

        match statement
            .next()
            .map_err(|e| StorageError::Backend(format!("step get({key}): {e}")))?
        {
            State::Done => Ok(None),
            State::Row => {
                let payload = statement
                    .read::<String, _>(0)
                    .map_err(|e| StorageError::Backend(format!("read get({key}): {e}")))?;
                let value = serde_json::from_str(&payload)
                    .map_err(|e| StorageError::Serde(e.to_string()))?;
                Ok(Some(value))
            }
        }
    }

    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        let serialized =
            serde_json::to_string(&value).map_err(|e| StorageError::Serde(e.to_string()))?;

        let db = self
            .db
            .lock()
            .map_err(|_e| StorageError::Backend("sqlite mutex poisoned".to_string()))?;

        let mut statement = db
            .prepare(
                "INSERT INTO poly_kv(key, payload) VALUES(?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET payload = excluded.payload",
            )
            .map_err(|e| StorageError::Backend(format!("prepare set({key}): {e}")))?;
        statement
            .bind((1, key))
            .map_err(|e| StorageError::Backend(format!("bind key set({key}): {e}")))?;
        statement
            .bind((2, serialized.as_str()))
            .map_err(|e| StorageError::Backend(format!("bind payload set({key}): {e}")))?;
        while statement
            .next()
            .map_err(|e| StorageError::Backend(format!("step set({key}): {e}")))?
            != State::Done
        {}
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let db = self
            .db
            .lock()
            .map_err(|_e| StorageError::Backend("sqlite mutex poisoned".to_string()))?;

        let mut statement = db
            .prepare("DELETE FROM poly_kv WHERE key = ?1")
            .map_err(|e| StorageError::Backend(format!("prepare delete({key}): {e}")))?;
        statement
            .bind((1, key))
            .map_err(|e| StorageError::Backend(format!("bind delete({key}): {e}")))?;
        while statement
            .next()
            .map_err(|e| StorageError::Backend(format!("step delete({key}): {e}")))?
            != State::Done
        {}
        Ok(())
    }

    pub async fn clear_all(&self) -> Result<(), StorageError> {
        let db = self
            .db
            .lock()
            .map_err(|_e| StorageError::Backend("sqlite mutex poisoned".to_string()))?;
        db.execute("DELETE FROM poly_kv")
            .map_err(|e| StorageError::Backend(format!("clear_all: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("lock")
    }

    #[tokio::test]
    async fn sqlite_storage_round_trips_values() {
        let _guard = env_guard();
        let dir = tempfile::tempdir().expect("tempdir");

        let storage = StorageInner::init_with_path(dir.path().to_path_buf())
            .await
            .expect("init");

        storage
            .set("app_settings", serde_json::json!({"theme":"neutral-dark"}))
            .await
            .expect("set");
        assert_eq!(
            storage.get("app_settings").await.expect("get"),
            Some(serde_json::json!({"theme":"neutral-dark"}))
        );

        storage
            .set("app_settings", serde_json::json!({"theme":"red"}))
            .await
            .expect("update");
        assert_eq!(
            storage.get("app_settings").await.expect("get2"),
            Some(serde_json::json!({"theme":"red"}))
        );

        storage.delete("app_settings").await.expect("delete");
        assert_eq!(storage.get("app_settings").await.expect("get3"), None);

        storage.set("a", serde_json::json!(1)).await.expect("set a");
        storage.set("b", serde_json::json!(2)).await.expect("set b");
        storage.clear_all().await.expect("clear");
        assert_eq!(storage.get("a").await.expect("geta"), None);
        assert_eq!(storage.get("b").await.expect("getb"), None);
    }

    #[tokio::test]
    async fn sqlite_storage_handles_various_types() {
        let _guard = env_guard();
        let dir = tempfile::tempdir().expect("tempdir");

        let storage = StorageInner::init_with_path(dir.path().to_path_buf())
            .await
            .expect("init");

        storage
            .set("string", serde_json::json!("hello"))
            .await
            .expect("set");
        storage
            .set("number", serde_json::json!(42.5))
            .await
            .expect("set");
        storage
            .set("boolean", serde_json::json!(true))
            .await
            .expect("set");
        storage
            .set("array", serde_json::json!([1, 2, 3]))
            .await
            .expect("set");
        storage
            .set("object", serde_json::json!({"key": "value"}))
            .await
            .expect("set");
        storage
            .set("null", serde_json::json!(null))
            .await
            .expect("set");

        assert_eq!(
            storage.get("string").await.expect("get"),
            Some(serde_json::json!("hello"))
        );
        assert_eq!(
            storage.get("number").await.expect("get"),
            Some(serde_json::json!(42.5))
        );
        assert_eq!(
            storage.get("boolean").await.expect("get"),
            Some(serde_json::json!(true))
        );
        assert_eq!(
            storage.get("array").await.expect("get"),
            Some(serde_json::json!([1, 2, 3]))
        );
        assert_eq!(
            storage.get("object").await.expect("get"),
            Some(serde_json::json!({"key": "value"}))
        );
        assert_eq!(
            storage.get("null").await.expect("get"),
            Some(serde_json::json!(null))
        );
    }

    #[tokio::test]
    async fn sqlite_storage_handles_large_values() {
        let _guard = env_guard();
        let dir = tempfile::tempdir().expect("tempdir");

        let storage = StorageInner::init_with_path(dir.path().to_path_buf())
            .await
            .expect("init");

        let large_string = "x".repeat(10000);
        storage
            .set("large", serde_json::json!(large_string))
            .await
            .expect("set");

        let retrieved = storage.get("large").await.expect("get");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().as_str().unwrap(), large_string);
    }

    #[tokio::test]
    async fn sqlite_storage_nonexistent_key_returns_none() {
        let _guard = env_guard();
        let dir = tempfile::tempdir().expect("tempdir");

        let storage = StorageInner::init_with_path(dir.path().to_path_buf())
            .await
            .expect("init");

        assert_eq!(storage.get("does_not_exist").await.expect("get"), None);
    }

    #[tokio::test]
    async fn sqlite_storage_delete_nonexistent_key_is_noop() {
        let _guard = env_guard();
        let dir = tempfile::tempdir().expect("tempdir");

        let storage = StorageInner::init_with_path(dir.path().to_path_buf())
            .await
            .expect("init");

        storage.delete("does_not_exist").await.expect("delete");
        assert_eq!(storage.get("does_not_exist").await.expect("get"), None);
    }
}
