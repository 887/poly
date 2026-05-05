//! client_settings_audit CRUD (Phase D).

use super::helpers::{bind_opt_str, drain, now_iso8601};
use super::{MemoryDb, MemoryError};

impl MemoryDb {
    pub fn record_client_settings_audit(
        &self,
        backend_id: &str,
        action: &str,
        payload_json: Option<&str>,
        status: &str,
        error_msg: Option<&str>,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO client_settings_audit
                (slug,backend_id,action,payload_json,status,error_msg,created_at)
             VALUES('system',?1,?2,?3,?4,?5,?6)"
        )?;
        stmt.bind((1, backend_id))?;
        stmt.bind((2, action))?;
        bind_opt_str(&mut stmt, 3, payload_json)?;
        stmt.bind((4, status))?;
        bind_opt_str(&mut stmt, 5, error_msg)?;
        stmt.bind((6, now.as_str()))?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn count_client_settings_audit(&self, backend_id: &str) -> Result<i64, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT COUNT(*) FROM client_settings_audit WHERE backend_id=?1"
        )?;
        stmt.bind((1, backend_id))?;
        if stmt.next()? == sqlite::State::Row {
            Ok(stmt.read::<i64, _>(0)?)
        } else {
            Ok(0)
        }
    }

    pub fn list_client_settings_audit(
        &self,
        backend_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let (sql, bind_backend) = if backend_id.is_some() {
            (
                "SELECT id,slug,backend_id,action,payload_json,status,error_msg,created_at
                 FROM client_settings_audit
                 WHERE backend_id=?1
                 ORDER BY created_at DESC LIMIT ?2",
                true,
            )
        } else {
            (
                "SELECT id,slug,backend_id,action,payload_json,status,error_msg,created_at
                 FROM client_settings_audit
                 ORDER BY created_at DESC LIMIT ?1",
                false,
            )
        };
        let mut stmt = db.prepare(sql)?;
        if bind_backend {
            stmt.bind((1, backend_id.unwrap_or("")))?;
            stmt.bind((2, limit))?;
        } else {
            stmt.bind((1, limit))?;
        }
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            let payload: Option<String> = match stmt.read::<sqlite::Value, _>(4)? {
                sqlite::Value::String(s) => Some(s),
                sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
            };
            let error: Option<String> = match stmt.read::<sqlite::Value, _>(6)? {
                sqlite::Value::String(s) => Some(s),
                sqlite::Value::Binary(_) | sqlite::Value::Float(_) | sqlite::Value::Integer(_) | sqlite::Value::Null => None,
            };
            out.push(serde_json::json!({
                "id":           stmt.read::<i64, _>(0)?,
                "slug":         stmt.read::<String, _>(1)?,
                "backend_id":   stmt.read::<String, _>(2)?,
                "action":       stmt.read::<String, _>(3)?,
                "payload_json": payload,
                "status":       stmt.read::<String, _>(5)?,
                "error_msg":    error,
                "created_at":   stmt.read::<String, _>(7)?,
            }));
        }
        Ok(out)
    }
}
