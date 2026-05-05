//! Draft queue CRUD (Phase B).

use super::helpers::{collect_drafts, drain, now_iso8601};
use super::{MemoryDb, MemoryError};

impl MemoryDb {
    pub fn draft_insert(
        &self,
        account_id: &str,
        chat_id: &str,
        body: &str,
        suggested_by: &str,
        auto_send_at: Option<&str>,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO drafts(account_id,chat_id,body,suggested_by,created_at,auto_send_at,status)
             VALUES(?1,?2,?3,?4,?5,?6,'pending')"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        stmt.bind((3, body))?;
        stmt.bind((4, suggested_by))?;
        stmt.bind((5, now.as_str()))?;
        match auto_send_at {
            Some(ts) => stmt.bind((6, ts))?,
            None     => stmt.bind((6, sqlite::Value::Null))?,
        }
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn draft_list(
        &self,
        account_id: Option<&str>,
        chat_id:    Option<&str>,
        status:     Option<&str>,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut conditions: Vec<&str> = Vec::new();
        if account_id.is_some() { conditions.push("account_id=?1"); }
        if chat_id.is_some()    { conditions.push("chat_id=?2");    }
        if status.is_some()     { conditions.push("status=?3");     }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT id,account_id,chat_id,body,suggested_by,created_at,auto_send_at,status
             FROM drafts {where_clause} ORDER BY id"
        );
        let mut stmt = db.prepare(&sql)?;
        if account_id.is_some() { stmt.bind((1, account_id.unwrap_or("")))?; }
        if chat_id.is_some()    { stmt.bind((2, chat_id.unwrap_or("")))?;    }
        if status.is_some()     { stmt.bind((3, status.unwrap_or("")))?;     }
        collect_drafts(&mut stmt)
    }

    pub fn draft_get(&self, draft_id: i64) -> Result<Option<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,account_id,chat_id,body,suggested_by,created_at,auto_send_at,status
             FROM drafts WHERE id=?1"
        )?;
        stmt.bind((1, draft_id))?;
        let mut rows = collect_drafts(&mut stmt)?;
        Ok(rows.pop())
    }

    pub fn draft_edit(&self, draft_id: i64, new_body: &str) -> Result<bool, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "UPDATE drafts SET body=?1 WHERE id=?2 AND status='pending'"
        )?;
        stmt.bind((1, new_body))?;
        stmt.bind((2, draft_id))?;
        drain(&mut stmt)?;

        let mut chk = db.prepare("SELECT changes()")?;
        if chk.next()? == sqlite::State::Row {
            Ok(chk.read::<i64, _>(0)? > 0)
        } else {
            Ok(false)
        }
    }

    pub fn draft_set_status(&self, draft_id: i64, new_status: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("UPDATE drafts SET status=?1 WHERE id=?2")?;
        stmt.bind((1, new_status))?;
        stmt.bind((2, draft_id))?;
        drain(&mut stmt)
    }

    pub fn draft_clear_autosend(&self, draft_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "UPDATE drafts SET auto_send_at=NULL WHERE id=?1 AND status='pending'"
        )?;
        stmt.bind((1, draft_id))?;
        drain(&mut stmt)
    }

    pub fn draft_pending_autosend(&self) -> Result<Vec<serde_json::Value>, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,account_id,chat_id,body,suggested_by,created_at,auto_send_at,status
             FROM drafts
             WHERE status='pending' AND auto_send_at IS NOT NULL AND auto_send_at <= ?1
             ORDER BY id"
        )?;
        stmt.bind((1, now.as_str()))?;
        collect_drafts(&mut stmt)
    }
}
