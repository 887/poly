//! Per-chat style CRUD (Phase E).

use super::helpers::{drain, now_iso8601, read_style_row};
use super::{MemoryDb, MemoryError};

impl MemoryDb {
    #[allow(clippy::too_many_arguments)]
    pub fn set_chat_style(
        &self,
        account_id: &str,
        chat_id: &str,
        tone: Option<&str>,
        formality: Option<&str>,
        emoji_allowed: Option<bool>,
        signature: Option<&str>,
        extra_notes: Option<&str>,
    ) -> Result<(), MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut sel = db.prepare(
            "SELECT tone,formality,emoji_allowed,signature,extra_notes
             FROM chat_style WHERE account_id=?1 AND chat_id=?2"
        )?;
        sel.bind((1, account_id))?;
        sel.bind((2, chat_id))?;

        let (cur_tone, cur_formality, cur_emoji, cur_sig, cur_notes) =
            if sel.next()? == sqlite::State::Row {
                let t  = sel.read::<Option<String>, _>(0)?;
                let f  = sel.read::<Option<String>, _>(1)?;
                let e  = sel.read::<Option<i64>, _>(2)?;
                let s  = sel.read::<Option<String>, _>(3)?;
                let n  = sel.read::<Option<String>, _>(4)?;
                (t, f, e, s, n)
            } else {
                (None, None, None, None, None)
            };
        drop(sel);

        let final_tone      = tone.map(std::string::ToString::to_string).or(cur_tone);
        let final_formality = formality.map(std::string::ToString::to_string).or(cur_formality);
        let final_emoji     = emoji_allowed
            .map(|b| if b { 1_i64 } else { 0_i64 })
            .or(cur_emoji)
            .unwrap_or(1_i64);
        let final_sig       = signature.map(std::string::ToString::to_string).or(cur_sig);
        let final_notes     = extra_notes.map(std::string::ToString::to_string).or(cur_notes);

        let mut stmt = db.prepare(
            "INSERT INTO chat_style
                (account_id,chat_id,tone,formality,emoji_allowed,signature,extra_notes,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(account_id,chat_id) DO UPDATE SET
                tone          = excluded.tone,
                formality     = excluded.formality,
                emoji_allowed = excluded.emoji_allowed,
                signature     = excluded.signature,
                extra_notes   = excluded.extra_notes,
                updated_at    = excluded.updated_at"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        match &final_tone {
            Some(v) => stmt.bind((3, v.as_str()))?,
            None    => stmt.bind((3, sqlite::Value::Null))?,
        }
        match &final_formality {
            Some(v) => stmt.bind((4, v.as_str()))?,
            None    => stmt.bind((4, sqlite::Value::Null))?,
        }
        stmt.bind((5, final_emoji))?;
        match &final_sig {
            Some(v) => stmt.bind((6, v.as_str()))?,
            None    => stmt.bind((6, sqlite::Value::Null))?,
        }
        match &final_notes {
            Some(v) => stmt.bind((7, v.as_str()))?,
            None    => stmt.bind((7, sqlite::Value::Null))?,
        }
        stmt.bind((8, now.as_str()))?;
        drain(&mut stmt)
    }

    pub fn get_chat_style(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<Option<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT tone,formality,emoji_allowed,signature,extra_notes,updated_at
             FROM chat_style WHERE account_id=?1 AND chat_id=?2"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        if stmt.next()? == sqlite::State::Row {
            Ok(Some(read_style_row(&mut stmt)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_chat_styles(
        &self,
        account_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let (sql, bind_account) = if account_id.is_some() {
            (
                "SELECT account_id,chat_id,tone,formality,emoji_allowed,signature,extra_notes,updated_at
                 FROM chat_style WHERE account_id=?1 ORDER BY account_id,chat_id",
                true,
            )
        } else {
            (
                "SELECT account_id,chat_id,tone,formality,emoji_allowed,signature,extra_notes,updated_at
                 FROM chat_style ORDER BY account_id,chat_id",
                false,
            )
        };
        let mut stmt = db.prepare(sql)?;
        if bind_account {
            stmt.bind((1, account_id.unwrap_or("")))?;
        }
        let mut out = Vec::new();
        while stmt.next()? == sqlite::State::Row {
            let aid = stmt.read::<String, _>(0)?;
            let cid = stmt.read::<String, _>(1)?;
            let row = serde_json::json!({
                "account_id":    aid,
                "chat_id":       cid,
                "tone":          stmt.read::<Option<String>, _>(2)?,
                "formality":     stmt.read::<Option<String>, _>(3)?,
                "emoji_allowed": stmt.read::<i64, _>(4)? != 0,
                "signature":     stmt.read::<Option<String>, _>(5)?,
                "extra_notes":   stmt.read::<Option<String>, _>(6)?,
                "updated_at":    stmt.read::<String, _>(7)?,
            });
            out.push(row);
        }
        Ok(out)
    }

    pub fn forget_chat_style(
        &self,
        account_id: &str,
        chat_id: &str,
    ) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM chat_style WHERE account_id=?1 AND chat_id=?2"
        )?;
        stmt.bind((1, account_id))?;
        stmt.bind((2, chat_id))?;
        drain(&mut stmt)
    }
}

// ─── ChatStyle helpers (public; consumed by UI crate) ────────────────────────

/// Static option lists for the style editor UI.
pub struct ChatStyle;

impl ChatStyle {
    #[must_use]
    pub fn tone_options() -> &'static [&'static str] {
        &["casual", "professional", "snarky", "warm", "direct"]
    }

    #[must_use]
    pub fn formality_options() -> &'static [&'static str] {
        &["tu", "vous", "neutral"]
    }
}
