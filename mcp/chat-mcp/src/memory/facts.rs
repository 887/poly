//! contact_facts CRUD (Phase A).

use super::helpers::{collect_facts, drain, now_iso8601};
use super::{MemoryDb, MemoryError};

impl MemoryDb {
    pub fn remember_fact(
        &self,
        account_id: &str,
        contact_id: &str,
        category: &str,
        fact_text: &str,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO contact_facts(account_id,contact_id,category,fact_text,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, contact_id))?;
        stmt.bind((3, category))?;
        stmt.bind((4, fact_text))?;
        stmt.bind((5, now.as_str()))?;
        stmt.bind((6, now.as_str()))?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn recall_facts(
        &self,
        account_id: &str,
        contact_id: &str,
        category: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let (sql, cat_bind) = if category.is_some() {
            (
                "SELECT id,account_id,contact_id,category,fact_text,created_at,updated_at
                 FROM contact_facts
                 WHERE account_id=?1 AND contact_id=?2 AND category=?3
                 ORDER BY id",
                true,
            )
        } else {
            (
                "SELECT id,account_id,contact_id,category,fact_text,created_at,updated_at
                 FROM contact_facts
                 WHERE account_id=?1 AND contact_id=?2
                 ORDER BY id",
                false,
            )
        };
        let mut stmt = db.prepare(sql)?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, contact_id))?;
        if cat_bind {
            stmt.bind((3, category.unwrap_or("")))?;
        }
        collect_facts(&mut stmt)
    }

    pub fn forget_fact(&self, fact_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM contact_facts WHERE id=?1")?;
        stmt.bind((1, fact_id))?;
        drain(&mut stmt)
    }

    pub fn search_facts(
        &self,
        query: &str,
        account_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let pattern = format!("%{query}%");
        let (sql, account_bind) = if account_id.is_some() {
            (
                "SELECT id,account_id,contact_id,category,fact_text,created_at,updated_at
                 FROM contact_facts
                 WHERE fact_text LIKE ?1 AND account_id=?2
                 ORDER BY id",
                true,
            )
        } else {
            (
                "SELECT id,account_id,contact_id,category,fact_text,created_at,updated_at
                 FROM contact_facts
                 WHERE fact_text LIKE ?1
                 ORDER BY id",
                false,
            )
        };
        let mut stmt = db.prepare(sql)?;
        stmt.bind((1, pattern.as_str()))?;
        if account_bind {
            stmt.bind((2, account_id.unwrap_or("")))?;
        }
        collect_facts(&mut stmt)
    }
}
