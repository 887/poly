//! chat_notes and chat_summaries CRUD.

use super::helpers::{collect_notes, drain, now_iso8601};
use super::{MemoryDb, MemoryError};

impl MemoryDb {
    pub fn store_chat_note(
        &self,
        account_id: &str,
        chat_id: &str,
        note_text: &str,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO chat_notes(account_id,chat_id,note_text,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5)"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        stmt.bind((3, note_text))?;
        stmt.bind((4, now.as_str()))?;
        stmt.bind((5, now.as_str()))?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == sqlite::State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    pub fn get_chat_notes(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,account_id,chat_id,note_text,created_at,updated_at
             FROM chat_notes
             WHERE account_id=?1 AND chat_id=?2
             ORDER BY id"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        collect_notes(&mut stmt)
    }

    pub fn forget_chat_note(&self, note_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM chat_notes WHERE id=?1")?;
        stmt.bind((1, note_id))?;
        drain(&mut stmt)
    }

    pub fn store_chat_summary(
        &self,
        account_id: &str,
        chat_id: &str,
        summary_text: &str,
        window_start_msg_id: &str,
        window_end_msg_id: &str,
    ) -> Result<(), MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO chat_summaries
                (account_id,chat_id,summary_text,window_start_msg_id,window_end_msg_id,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(account_id,chat_id) DO UPDATE SET
                summary_text        = excluded.summary_text,
                window_start_msg_id = excluded.window_start_msg_id,
                window_end_msg_id   = excluded.window_end_msg_id,
                updated_at          = excluded.updated_at"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        stmt.bind((3, summary_text))?;
        stmt.bind((4, window_start_msg_id))?;
        stmt.bind((5, window_end_msg_id))?;
        stmt.bind((6, now.as_str()))?;
        drain(&mut stmt)
    }

    pub fn get_chat_summary(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<Option<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT summary_text,window_start_msg_id,window_end_msg_id,updated_at
             FROM chat_summaries
             WHERE account_id=?1 AND chat_id=?2"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        if stmt.next()? == sqlite::State::Row {
            Ok(Some(serde_json::json!({
                "summary":      stmt.read::<String, _>(0)?,
                "window_start": stmt.read::<String, _>(1)?,
                "window_end":   stmt.read::<String, _>(2)?,
                "updated_at":   stmt.read::<String, _>(3)?,
            })))
        } else {
            Ok(None)
        }
    }
}
