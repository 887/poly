//! Phase A memory store — `contact_facts`, `chat_notes`, `chat_summaries`.
//!
//! All three tables live in Poly's main `storage.sqlite3` (or an in-memory DB
//! for tests). This module owns the schema migration and every CRUD operation
//! the MCP tools need.

use std::sync::{Arc, Mutex};

use sqlite::{Connection, ConnectionThreadSafe, State};

// ─── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("sqlite error: {0}")]
    Sqlite(String),
}

impl From<sqlite::Error> for MemoryError {
    fn from(e: sqlite::Error) -> Self {
        Self::Sqlite(e.to_string())
    }
}

// ─── Handle ───────────────────────────────────────────────────────────────────

/// Thread-safe handle to the memory tables.
///
/// Cheap to clone — backed by `Arc<Mutex<…>>`.
#[derive(Clone)]
pub struct MemoryDb {
    db: Arc<Mutex<ConnectionThreadSafe>>,
}

impl MemoryDb {
    /// Open the memory tables in the same `storage.sqlite3` that the rest of
    /// Poly uses.
    ///
    /// Pass `":memory:"` for tests.
    pub fn open(path: &str) -> Result<Self, MemoryError> {
        let mut db = if path == ":memory:" {
            Connection::open_thread_safe(":memory:")
        } else {
            Connection::open_thread_safe(path)
        }
        .map_err(|e| MemoryError::Sqlite(e.to_string()))?;

        db.set_busy_timeout(5_000)
            .map_err(|e| MemoryError::Sqlite(e.to_string()))?;

        Self::run_migrations(&db)?;
        Ok(Self { db: Arc::new(Mutex::new(db)) })
    }

    fn run_migrations(db: &ConnectionThreadSafe) -> Result<(), MemoryError> {
        db.execute(
            "CREATE TABLE IF NOT EXISTS contact_facts (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id  TEXT    NOT NULL,
                contact_id  TEXT    NOT NULL,
                category    TEXT    NOT NULL DEFAULT '',
                fact_text   TEXT    NOT NULL,
                created_at  TEXT    NOT NULL,
                updated_at  TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_contact_facts_contact
                ON contact_facts(account_id, contact_id);

            CREATE TABLE IF NOT EXISTS chat_notes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id  TEXT    NOT NULL,
                chat_id     TEXT    NOT NULL,
                note_text   TEXT    NOT NULL,
                created_at  TEXT    NOT NULL,
                updated_at  TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_notes_chat
                ON chat_notes(account_id, chat_id);

            CREATE TABLE IF NOT EXISTS chat_summaries (
                account_id          TEXT NOT NULL,
                chat_id             TEXT NOT NULL,
                summary_text        TEXT NOT NULL,
                window_start_msg_id TEXT NOT NULL DEFAULT '',
                window_end_msg_id   TEXT NOT NULL DEFAULT '',
                updated_at          TEXT NOT NULL,
                PRIMARY KEY(account_id, chat_id)
            );

            CREATE TABLE IF NOT EXISTS drafts (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                account_id   TEXT NOT NULL,
                chat_id      TEXT NOT NULL,
                body         TEXT NOT NULL,
                suggested_by TEXT NOT NULL,
                created_at   TEXT NOT NULL,
                auto_send_at TEXT,
                status       TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_drafts_chat
                ON drafts(account_id, chat_id, status);

            CREATE TABLE IF NOT EXISTS chat_style (
                account_id    TEXT NOT NULL,
                chat_id       TEXT NOT NULL,
                tone          TEXT,
                formality     TEXT,
                emoji_allowed INTEGER NOT NULL DEFAULT 1,
                signature     TEXT,
                extra_notes   TEXT,
                updated_at    TEXT NOT NULL,
                PRIMARY KEY(account_id, chat_id)
            );"
        ).map_err(|e| MemoryError::Sqlite(e.to_string()))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ConnectionThreadSafe>, MemoryError> {
        self.db
            .lock()
            .map_err(|_| MemoryError::Sqlite("mutex poisoned".to_string()))
    }

    // ─── contact_facts ────────────────────────────────────────────────────────

    /// Insert a new fact and return its generated `id`.
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
        if id_stmt.next()? == State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    /// Return all facts for a contact, optionally filtered by category.
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

    /// Delete a fact by primary key.
    pub fn forget_fact(&self, fact_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM contact_facts WHERE id=?1")?;
        stmt.bind((1, fact_id))?;
        drain(&mut stmt)
    }

    /// Full-text LIKE search over `fact_text`, optionally scoped to one account.
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

    // ─── chat_notes ───────────────────────────────────────────────────────────

    /// Insert a new note and return its `id`.
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
        if id_stmt.next()? == State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    /// Return all notes for a chat.
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

    /// Delete a note by primary key.
    pub fn forget_chat_note(&self, note_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM chat_notes WHERE id=?1")?;
        stmt.bind((1, note_id))?;
        drain(&mut stmt)
    }

    // ─── chat_summaries ───────────────────────────────────────────────────────

    /// Upsert the rolling summary for a chat.
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

    /// Fetch the summary for a chat, or `None` if not yet stored.
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
        if stmt.next()? == State::Row {
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

    // ─── drafts ───────────────────────────────────────────────────────────────

    /// Insert a new draft and return its generated `id`.
    ///
    /// `auto_send_at` is an ISO-8601 UTC timestamp or `None`.
    /// `status` is typically `"pending"`.
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
        if id_stmt.next()? == State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    /// List drafts, optionally filtered by `account_id`, `chat_id`, and/or `status`.
    pub fn draft_list(
        &self,
        account_id: Option<&str>,
        chat_id:    Option<&str>,
        status:     Option<&str>,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        // Build query dynamically based on which filters are present.
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

    /// Look up a single draft by primary key.
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

    /// Update a draft's body. Only allowed while `status = 'pending'`.
    /// Returns `true` if the row was found and updated, `false` if not found or wrong status.
    pub fn draft_edit(&self, draft_id: i64, new_body: &str) -> Result<bool, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "UPDATE drafts SET body=?1 WHERE id=?2 AND status='pending'"
        )?;
        stmt.bind((1, new_body))?;
        stmt.bind((2, draft_id))?;
        drain(&mut stmt)?;

        let mut chk = db.prepare("SELECT changes()")?;
        if chk.next()? == State::Row {
            Ok(chk.read::<i64, _>(0)? > 0)
        } else {
            Ok(false)
        }
    }

    /// Transition a draft's status to `new_status`.
    pub fn draft_set_status(&self, draft_id: i64, new_status: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("UPDATE drafts SET status=?1 WHERE id=?2")?;
        stmt.bind((1, new_status))?;
        stmt.bind((2, draft_id))?;
        drain(&mut stmt)
    }

    /// Clear `auto_send_at` for a draft (cancel auto-send).
    pub fn draft_clear_autosend(&self, draft_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "UPDATE drafts SET auto_send_at=NULL WHERE id=?1 AND status='pending'"
        )?;
        stmt.bind((1, draft_id))?;
        drain(&mut stmt)
    }

    /// Return all pending drafts whose `auto_send_at <= now`.
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

    // ─── chat_style ───────────────────────────────────────────────────────────

    /// Upsert the per-chat style record.
    ///
    /// Fields that are `None` are left at their current DB value (not
    /// overwritten with NULL) unless there is no existing row, in which case
    /// `None` columns are stored as SQL NULL.
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
        // Fetch any existing row so we can preserve columns the caller didn't
        // supply (partial-update semantics).
        let mut sel = db.prepare(
            "SELECT tone,formality,emoji_allowed,signature,extra_notes
             FROM chat_style WHERE account_id=?1 AND chat_id=?2"
        )?;
        sel.bind((1, account_id))?;
        sel.bind((2, chat_id))?;

        let (cur_tone, cur_formality, cur_emoji, cur_sig, cur_notes) =
            if sel.next()? == State::Row {
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

        let final_tone      = tone.map(|s| s.to_string()).or(cur_tone);
        let final_formality = formality.map(|s| s.to_string()).or(cur_formality);
        let final_emoji     = emoji_allowed
            .map(|b| if b { 1_i64 } else { 0_i64 })
            .or(cur_emoji)
            .unwrap_or(1_i64);
        let final_sig       = signature.map(|s| s.to_string()).or(cur_sig);
        let final_notes     = extra_notes.map(|s| s.to_string()).or(cur_notes);

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

    /// Fetch the style for a chat, or `None` if not configured.
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
        if stmt.next()? == State::Row {
            Ok(Some(read_style_row(&mut stmt)?))
        } else {
            Ok(None)
        }
    }

    /// Return all style records, optionally filtered by account.
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
        while stmt.next()? == State::Row {
            let aid = stmt.read::<String, _>(0)?;
            let cid = stmt.read::<String, _>(1)?;
            // Columns 2-7 are the same order as in read_style_row but we
            // need to shift the index — build manually.
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

    /// Delete the style record for a chat.  No-op if not present.
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
    /// Predefined tone labels (free-form values are also accepted).
    pub fn tone_options() -> &'static [&'static str] {
        &["casual", "professional", "snarky", "warm", "direct"]
    }

    /// Predefined formality labels.
    pub fn formality_options() -> &'static [&'static str] {
        &["tu", "vous", "neutral"]
    }
}

// ─── Private helpers ──────────────────────────────────────────────────────────

fn now_iso8601() -> String {
    // std-only, no chrono dep: use UNIX_EPOCH seconds formatted manually.
    // RFC 3339 / ISO 8601 UTC: "YYYY-MM-DDTHH:MM:SSZ"
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Julian day arithmetic — simple integer math.
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400; // days since 1970-01-01
    // Gregorian calendar conversion.
    let (y, mo, d) = days_to_ymd(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://www.researchgate.net/publication/316558298
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    (y, mo, d)
}

/// Step a statement to completion (for INSERT/UPDATE/DELETE).
fn drain(stmt: &mut sqlite::Statement<'_>) -> Result<(), MemoryError> {
    while stmt.next()? != State::Done {}
    Ok(())
}

fn collect_facts(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        out.push(serde_json::json!({
            "id":          stmt.read::<i64, _>(0)?,
            "account_id":  stmt.read::<String, _>(1)?,
            "contact_id":  stmt.read::<String, _>(2)?,
            "category":    stmt.read::<String, _>(3)?,
            "fact_text":   stmt.read::<String, _>(4)?,
            "created_at":  stmt.read::<String, _>(5)?,
            "updated_at":  stmt.read::<String, _>(6)?,
        }));
    }
    Ok(out)
}

fn collect_notes(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        out.push(serde_json::json!({
            "id":          stmt.read::<i64, _>(0)?,
            "account_id":  stmt.read::<String, _>(1)?,
            "chat_id":     stmt.read::<String, _>(2)?,
            "note_text":   stmt.read::<String, _>(3)?,
            "created_at":  stmt.read::<String, _>(4)?,
            "updated_at":  stmt.read::<String, _>(5)?,
        }));
    }
    Ok(out)
}

fn collect_drafts(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        // auto_send_at may be NULL — read as Option<String>.
        let auto_send_at: Option<String> = match stmt.read::<sqlite::Value, _>(6)? {
            sqlite::Value::String(s) => Some(s),
            _ => None,
        };
        out.push(serde_json::json!({
            "id":           stmt.read::<i64, _>(0)?,
            "account_id":   stmt.read::<String, _>(1)?,
            "chat_id":      stmt.read::<String, _>(2)?,
            "body":         stmt.read::<String, _>(3)?,
            "suggested_by": stmt.read::<String, _>(4)?,
            "created_at":   stmt.read::<String, _>(5)?,
            "auto_send_at": auto_send_at,
            "status":       stmt.read::<String, _>(7)?,
        }));
    }
    Ok(out)
}

/// Read a single `chat_style` row from a prepared statement already
/// positioned at a row.  Column order:
/// 0=tone 1=formality 2=emoji_allowed 3=signature 4=extra_notes 5=updated_at
fn read_style_row(stmt: &mut sqlite::Statement<'_>) -> Result<serde_json::Value, MemoryError> {
    Ok(serde_json::json!({
        "tone":          stmt.read::<Option<String>, _>(0)?,
        "formality":     stmt.read::<Option<String>, _>(1)?,
        "emoji_allowed": stmt.read::<i64, _>(2)? != 0,
        "signature":     stmt.read::<Option<String>, _>(3)?,
        "extra_notes":   stmt.read::<Option<String>, _>(4)?,
        "updated_at":    stmt.read::<String, _>(5)?,
    }))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn fresh_db() -> MemoryDb {
        MemoryDb::open(":memory:").expect("open in-memory db")
    }

    // ── contact_facts ─────────────────────────────────────────────────────────

    #[test]
    fn remember_and_recall_fact() {
        let db = fresh_db();
        let id = db.remember_fact("acc1", "contact1", "preference", "likes coffee").unwrap();
        assert!(id > 0);

        let facts = db.recall_facts("acc1", "contact1", None).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0]["fact_text"], "likes coffee");
        assert_eq!(facts[0]["category"], "preference");
        assert_eq!(facts[0]["id"], id);
    }

    #[test]
    fn recall_facts_with_category_filter() {
        let db = fresh_db();
        db.remember_fact("acc1", "c1", "preference", "likes coffee").unwrap();
        db.remember_fact("acc1", "c1", "schedule", "free Friday").unwrap();
        db.remember_fact("acc1", "c1", "preference", "hates Mondays").unwrap();

        let prefs = db.recall_facts("acc1", "c1", Some("preference")).unwrap();
        assert_eq!(prefs.len(), 2);

        let sched = db.recall_facts("acc1", "c1", Some("schedule")).unwrap();
        assert_eq!(sched.len(), 1);
        assert_eq!(sched[0]["fact_text"], "free Friday");
    }

    #[test]
    fn recall_facts_account_scoped() {
        let db = fresh_db();
        db.remember_fact("acc1", "c1", "", "fact A").unwrap();
        db.remember_fact("acc2", "c1", "", "fact B").unwrap();

        let a = db.recall_facts("acc1", "c1", None).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["fact_text"], "fact A");

        let b = db.recall_facts("acc2", "c1", None).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0]["fact_text"], "fact B");
    }

    #[test]
    fn forget_fact() {
        let db = fresh_db();
        let id = db.remember_fact("acc1", "c1", "", "to forget").unwrap();
        db.forget_fact(id).unwrap();

        let facts = db.recall_facts("acc1", "c1", None).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn forget_nonexistent_fact_is_noop() {
        let db = fresh_db();
        db.forget_fact(9999).unwrap(); // must not error
    }

    #[test]
    fn search_facts_like() {
        let db = fresh_db();
        db.remember_fact("acc1", "c1", "", "loves hiking in the mountains").unwrap();
        db.remember_fact("acc1", "c2", "", "prefers staying indoors").unwrap();
        db.remember_fact("acc2", "c1", "", "hiking enthusiast").unwrap();

        let results = db.search_facts("hiking", None).unwrap();
        assert_eq!(results.len(), 2);

        let scoped = db.search_facts("hiking", Some("acc1")).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0]["account_id"], "acc1");
    }

    #[test]
    fn search_facts_no_match() {
        let db = fresh_db();
        db.remember_fact("acc1", "c1", "", "likes tea").unwrap();
        let results = db.search_facts("coffee", None).unwrap();
        assert!(results.is_empty());
    }

    // ── chat_notes ────────────────────────────────────────────────────────────

    #[test]
    fn store_and_get_chat_note() {
        let db = fresh_db();
        let id = db.store_chat_note("acc1", "chat1", "remember: bring umbrella").unwrap();
        assert!(id > 0);

        let notes = db.get_chat_notes("acc1", "chat1").unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0]["note_text"], "remember: bring umbrella");
        assert_eq!(notes[0]["id"], id);
    }

    #[test]
    fn multiple_notes_ordered_by_id() {
        let db = fresh_db();
        let id1 = db.store_chat_note("acc1", "chat1", "note one").unwrap();
        let id2 = db.store_chat_note("acc1", "chat1", "note two").unwrap();
        let notes = db.get_chat_notes("acc1", "chat1").unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes[0]["id"].as_i64().unwrap() < notes[1]["id"].as_i64().unwrap());
        let _ = (id1, id2);
    }

    #[test]
    fn forget_chat_note() {
        let db = fresh_db();
        let id = db.store_chat_note("acc1", "chat1", "to forget").unwrap();
        db.forget_chat_note(id).unwrap();

        let notes = db.get_chat_notes("acc1", "chat1").unwrap();
        assert!(notes.is_empty());
    }

    #[test]
    fn get_chat_notes_empty_for_unknown_chat() {
        let db = fresh_db();
        let notes = db.get_chat_notes("acc1", "unknown-chat").unwrap();
        assert!(notes.is_empty());
    }

    // ── chat_summaries ────────────────────────────────────────────────────────

    #[test]
    fn store_and_get_chat_summary() {
        let db = fresh_db();
        db.store_chat_summary("acc1", "chat1", "Alice and Bob discussed the project", "msg1", "msg20").unwrap();

        let s = db.get_chat_summary("acc1", "chat1").unwrap();
        assert!(s.is_some());
        let s = s.unwrap();
        assert_eq!(s["summary"], "Alice and Bob discussed the project");
        assert_eq!(s["window_start"], "msg1");
        assert_eq!(s["window_end"], "msg20");
    }

    #[test]
    fn chat_summary_upsert() {
        let db = fresh_db();
        db.store_chat_summary("acc1", "chat1", "old summary", "msg1", "msg10").unwrap();
        db.store_chat_summary("acc1", "chat1", "new summary", "msg11", "msg20").unwrap();

        let s = db.get_chat_summary("acc1", "chat1").unwrap().unwrap();
        assert_eq!(s["summary"], "new summary");
        assert_eq!(s["window_start"], "msg11");
    }

    #[test]
    fn get_chat_summary_returns_none_when_missing() {
        let db = fresh_db();
        let s = db.get_chat_summary("acc1", "no-chat").unwrap();
        assert!(s.is_none());
    }

    #[test]
    fn summaries_are_per_account_and_chat() {
        let db = fresh_db();
        db.store_chat_summary("acc1", "chat1", "summary A", "", "").unwrap();
        db.store_chat_summary("acc2", "chat1", "summary B", "", "").unwrap();
        db.store_chat_summary("acc1", "chat2", "summary C", "", "").unwrap();

        assert_eq!(db.get_chat_summary("acc1", "chat1").unwrap().unwrap()["summary"], "summary A");
        assert_eq!(db.get_chat_summary("acc2", "chat1").unwrap().unwrap()["summary"], "summary B");
        assert_eq!(db.get_chat_summary("acc1", "chat2").unwrap().unwrap()["summary"], "summary C");
    }

    // ── drafts ────────────────────────────────────────────────────────────────

    #[test]
    fn draft_insert_and_list() {
        let db = fresh_db();
        let id = db.draft_insert("acc1", "chat1", "Hello!", "test-agent", None).unwrap();
        assert!(id > 0);

        let drafts = db.draft_list(Some("acc1"), Some("chat1"), Some("pending")).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0]["body"], "Hello!");
        assert_eq!(drafts[0]["status"], "pending");
        assert_eq!(drafts[0]["suggested_by"], "test-agent");
        assert!(drafts[0]["auto_send_at"].is_null());
    }

    #[test]
    fn draft_insert_with_autosend() {
        let db = fresh_db();
        let id = db.draft_insert("acc1", "chat1", "Scheduled!", "test-agent", Some("2030-01-01T00:00:00Z")).unwrap();
        assert!(id > 0);

        let drafts = db.draft_list(Some("acc1"), Some("chat1"), None).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0]["auto_send_at"], "2030-01-01T00:00:00Z");
    }

    #[test]
    fn draft_edit_pending() {
        let db = fresh_db();
        let id = db.draft_insert("acc1", "chat1", "Original", "bot", None).unwrap();
        let changed = db.draft_edit(id, "Updated body").unwrap();
        assert!(changed);

        let d = db.draft_get(id).unwrap().unwrap();
        assert_eq!(d["body"], "Updated body");
    }

    #[test]
    fn draft_edit_non_pending_fails() {
        let db = fresh_db();
        let id = db.draft_insert("acc1", "chat1", "body", "bot", None).unwrap();
        db.draft_set_status(id, "sent").unwrap();

        let changed = db.draft_edit(id, "attempt").unwrap();
        assert!(!changed, "edit of sent draft should return false");
    }

    #[test]
    fn draft_discard() {
        let db = fresh_db();
        let id = db.draft_insert("acc1", "chat1", "body", "bot", None).unwrap();
        db.draft_set_status(id, "discarded").unwrap();

        let d = db.draft_get(id).unwrap().unwrap();
        assert_eq!(d["status"], "discarded");

        let pending = db.draft_list(Some("acc1"), Some("chat1"), Some("pending")).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn draft_clear_autosend() {
        let db = fresh_db();
        let id = db.draft_insert("acc1", "chat1", "body", "bot", Some("2030-01-01T00:00:00Z")).unwrap();
        db.draft_clear_autosend(id).unwrap();

        let d = db.draft_get(id).unwrap().unwrap();
        assert!(d["auto_send_at"].is_null());
    }

    #[test]
    fn draft_list_no_filters() {
        let db = fresh_db();
        db.draft_insert("acc1", "chat1", "a", "bot", None).unwrap();
        db.draft_insert("acc2", "chat2", "b", "bot", None).unwrap();

        let all = db.draft_list(None, None, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn draft_pending_autosend_returns_overdue() {
        let db = fresh_db();
        // Past timestamp — should be returned.
        db.draft_insert("acc1", "chat1", "overdue", "bot", Some("2020-01-01T00:00:00Z")).unwrap();
        // Future timestamp — should NOT be returned.
        db.draft_insert("acc1", "chat1", "future", "bot", Some("2090-01-01T00:00:00Z")).unwrap();
        // No auto_send — should NOT be returned.
        db.draft_insert("acc1", "chat1", "manual", "bot", None).unwrap();

        let due = db.draft_pending_autosend().unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0]["body"], "overdue");
    }

    // ── chat_style ────────────────────────────────────────────────────────────

    #[test]
    fn set_and_get_chat_style() {
        let db = fresh_db();
        db.set_chat_style("acc1", "chat1", Some("casual"), Some("tu"), Some(true), Some("Alex"), Some("prefers short replies")).unwrap();
        let style = db.get_chat_style("acc1", "chat1").unwrap();
        assert!(style.is_some());
        let s = style.unwrap();
        assert_eq!(s["tone"], "casual");
        assert_eq!(s["formality"], "tu");
        assert_eq!(s["emoji_allowed"], true);
        assert_eq!(s["signature"], "Alex");
        assert_eq!(s["extra_notes"], "prefers short replies");
    }

    #[test]
    fn get_chat_style_returns_none_when_missing() {
        let db = fresh_db();
        let style = db.get_chat_style("acc1", "no-chat").unwrap();
        assert!(style.is_none());
    }

    #[test]
    fn set_chat_style_partial_update_preserves_unset_fields() {
        let db = fresh_db();
        db.set_chat_style("acc1", "chat1", Some("warm"), Some("vous"), Some(false), Some("Bob"), None).unwrap();
        // Update only tone — other fields must stay.
        db.set_chat_style("acc1", "chat1", Some("direct"), None, None, None, None).unwrap();
        let s = db.get_chat_style("acc1", "chat1").unwrap().unwrap();
        assert_eq!(s["tone"], "direct");
        assert_eq!(s["formality"], "vous");
        assert_eq!(s["emoji_allowed"], false);
        assert_eq!(s["signature"], "Bob");
    }

    #[test]
    fn list_chat_styles_filtered_by_account() {
        let db = fresh_db();
        db.set_chat_style("acc1", "chat1", Some("casual"), None, Some(true), None, None).unwrap();
        db.set_chat_style("acc1", "chat2", Some("warm"), None, Some(true), None, None).unwrap();
        db.set_chat_style("acc2", "chat1", Some("direct"), None, Some(true), None, None).unwrap();

        let list1 = db.list_chat_styles(Some("acc1")).unwrap();
        assert_eq!(list1.len(), 2);
        for item in &list1 {
            assert_eq!(item["account_id"], "acc1");
        }

        let all = db.list_chat_styles(None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn forget_chat_style() {
        let db = fresh_db();
        db.set_chat_style("acc1", "chat1", Some("snarky"), None, Some(true), None, None).unwrap();
        db.forget_chat_style("acc1", "chat1").unwrap();
        assert!(db.get_chat_style("acc1", "chat1").unwrap().is_none());
    }

    #[test]
    fn forget_chat_style_nonexistent_is_noop() {
        let db = fresh_db();
        db.forget_chat_style("acc1", "ghost-chat").unwrap(); // must not error
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    #[test]
    fn now_iso8601_looks_plausible() {
        let s = now_iso8601();
        // "2026-04-19T12:34:56Z" — length 20, has 'T' and 'Z'
        assert_eq!(s.len(), 20, "unexpected length: {s}");
        assert!(s.contains('T'));
        assert!(s.ends_with('Z'));
        assert!(s.starts_with("20")); // year 2xxx
    }
}
