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
        ).map_err(|e| MemoryError::Sqlite(e.to_string()))?;

        // ── Phase A — meta-personas schema ────────────────────────────────────
        db.execute(
            "CREATE TABLE IF NOT EXISTS personas (
                slug                     TEXT PRIMARY KEY,
                name                     TEXT NOT NULL,
                avatar_emoji             TEXT NOT NULL DEFAULT '🤖',
                system_prompt            TEXT NOT NULL,
                style_notes              TEXT,
                heartbeat_interval_secs  INTEGER,
                proactivity              TEXT NOT NULL DEFAULT 'drafts-only',
                rate_limit_per_hour      INTEGER NOT NULL DEFAULT 4,
                created_at               TEXT NOT NULL,
                updated_at               TEXT NOT NULL,
                last_run_at              TEXT,
                enabled                  INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS persona_sources (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                persona_slug    TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
                account_id      TEXT NOT NULL,
                selector_kind   TEXT NOT NULL,
                selector_value  TEXT,
                include         INTEGER NOT NULL DEFAULT 1,
                created_at      TEXT NOT NULL,
                UNIQUE (persona_slug, account_id, selector_kind, selector_value, include)
            );
            CREATE INDEX IF NOT EXISTS idx_persona_sources_slug
                ON persona_sources(persona_slug);

            CREATE TABLE IF NOT EXISTS persona_tool_whitelist (
                persona_slug  TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
                tool_name     TEXT NOT NULL,
                PRIMARY KEY (persona_slug, tool_name)
            );

            CREATE TABLE IF NOT EXISTS persona_facts (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                persona_slug  TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
                category      TEXT,
                fact_text     TEXT NOT NULL,
                pinned        INTEGER NOT NULL DEFAULT 0,
                created_at    TEXT NOT NULL,
                updated_at    TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_persona_facts_slug
                ON persona_facts(persona_slug);
            CREATE INDEX IF NOT EXISTS idx_persona_facts_pinned
                ON persona_facts(persona_slug, pinned);

            CREATE TABLE IF NOT EXISTS persona_outbound_allowlist (
                persona_slug          TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
                account_id            TEXT NOT NULL,
                chat_id               TEXT NOT NULL,
                max_messages_per_day  INTEGER NOT NULL DEFAULT 1,
                created_at            TEXT NOT NULL,
                PRIMARY KEY (persona_slug, account_id, chat_id)
            );

            CREATE TABLE IF NOT EXISTS persona_audit (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                persona_slug    TEXT NOT NULL REFERENCES personas(slug) ON DELETE CASCADE,
                occurred_at     TEXT NOT NULL,
                actor           TEXT NOT NULL,
                action          TEXT NOT NULL,
                target_account  TEXT,
                target_chat     TEXT,
                payload_json    TEXT,
                result          TEXT NOT NULL,
                error_msg       TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_persona_audit_slug_time
                ON persona_audit(persona_slug, occurred_at DESC);"
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

    // ─── personas ─────────────────────────────────────────────────────────────

    /// Insert a new persona row.  Returns the slug on success (same as input).
    pub fn create_persona(
        &self,
        slug: &str,
        name: &str,
        avatar_emoji: &str,
        system_prompt: &str,
        style_notes: Option<&str>,
        heartbeat_interval_secs: Option<i64>,
        proactivity: &str,
        rate_limit_per_hour: i64,
    ) -> Result<String, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO personas
                (slug,name,avatar_emoji,system_prompt,style_notes,
                 heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                 created_at,updated_at,enabled)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1)"
        )?;
        stmt.bind((1, slug))?;
        stmt.bind((2, name))?;
        stmt.bind((3, avatar_emoji))?;
        stmt.bind((4, system_prompt))?;
        match style_notes {
            Some(v) => stmt.bind((5, v))?,
            None    => stmt.bind((5, sqlite::Value::Null))?,
        }
        match heartbeat_interval_secs {
            Some(v) => stmt.bind((6, v))?,
            None    => stmt.bind((6, sqlite::Value::Null))?,
        }
        stmt.bind((7, proactivity))?;
        stmt.bind((8, rate_limit_per_hour))?;
        stmt.bind((9, now.as_str()))?;
        stmt.bind((10, now.as_str()))?;
        drain(&mut stmt)?;
        Ok(slug.to_string())
    }

    /// Fetch a single persona by slug.  Returns `None` if not found.
    pub fn get_persona(&self, slug: &str) -> Result<Option<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT slug,name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    created_at,updated_at,last_run_at,enabled
             FROM personas WHERE slug=?1"
        )?;
        stmt.bind((1, slug))?;
        if stmt.next()? == State::Row {
            Ok(Some(read_persona_row(&mut stmt)?))
        } else {
            Ok(None)
        }
    }

    /// Return all persona rows ordered by name.
    pub fn list_personas(&self) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT slug,name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    created_at,updated_at,last_run_at,enabled
             FROM personas ORDER BY name"
        )?;
        let mut out = Vec::new();
        while stmt.next()? == State::Row {
            out.push(read_persona_row(&mut stmt)?);
        }
        Ok(out)
    }

    /// Partial-update a persona — only supplied `Some(…)` fields are written.
    /// `slug` is the lookup key and cannot be changed.
    ///
    /// Returns `true` if the row was found and updated, `false` if slug not found.
    pub fn update_persona(
        &self,
        slug: &str,
        name: Option<&str>,
        avatar_emoji: Option<&str>,
        system_prompt: Option<&str>,
        style_notes: Option<Option<&str>>,            // Some(None) = set NULL
        heartbeat_interval_secs: Option<Option<i64>>, // Some(None) = set NULL
        proactivity: Option<&str>,
        rate_limit_per_hour: Option<i64>,
        enabled: Option<bool>,
        last_run_at: Option<Option<&str>>,            // Some(None) = set NULL
    ) -> Result<bool, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;

        // Fetch current row so we can fill unchanged fields.
        let mut sel = db.prepare(
            "SELECT name,avatar_emoji,system_prompt,style_notes,
                    heartbeat_interval_secs,proactivity,rate_limit_per_hour,
                    enabled,last_run_at
             FROM personas WHERE slug=?1"
        )?;
        sel.bind((1, slug))?;
        if sel.next()? != State::Row {
            return Ok(false);
        }
        let cur_name:    String         = sel.read::<String, _>(0)?;
        let cur_emoji:   String         = sel.read::<String, _>(1)?;
        let cur_prompt:  String         = sel.read::<String, _>(2)?;
        let cur_notes:   Option<String> = match sel.read::<sqlite::Value, _>(3)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        let cur_hb:      Option<i64>    = match sel.read::<sqlite::Value, _>(4)? {
            sqlite::Value::Integer(v) => Some(v), _ => None };
        let cur_pro:     String         = sel.read::<String, _>(5)?;
        let cur_rl:      i64            = sel.read::<i64, _>(6)?;
        let cur_enabled: i64            = sel.read::<i64, _>(7)?;
        let cur_lr:      Option<String> = match sel.read::<sqlite::Value, _>(8)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        drop(sel);

        let fin_name   = name.map(|s| s.to_string()).unwrap_or(cur_name);
        let fin_emoji  = avatar_emoji.map(|s| s.to_string()).unwrap_or(cur_emoji);
        let fin_prompt = system_prompt.map(|s| s.to_string()).unwrap_or(cur_prompt);
        let fin_notes: Option<String> = match style_notes {
            Some(Some(v)) => Some(v.to_string()),
            Some(None)    => None,
            None          => cur_notes,
        };
        let fin_hb: Option<i64> = match heartbeat_interval_secs {
            Some(Some(v)) => Some(v),
            Some(None)    => None,
            None          => cur_hb,
        };
        let fin_pro = proactivity.map(|s| s.to_string()).unwrap_or(cur_pro);
        let fin_rl  = rate_limit_per_hour.unwrap_or(cur_rl);
        let fin_en  = enabled.map(|b| if b { 1_i64 } else { 0_i64 }).unwrap_or(cur_enabled);
        let fin_lr: Option<String> = match last_run_at {
            Some(Some(v)) => Some(v.to_string()),
            Some(None)    => None,
            None          => cur_lr,
        };

        let mut stmt = db.prepare(
            "UPDATE personas SET
                name=?1, avatar_emoji=?2, system_prompt=?3, style_notes=?4,
                heartbeat_interval_secs=?5, proactivity=?6, rate_limit_per_hour=?7,
                enabled=?8, last_run_at=?9, updated_at=?10
             WHERE slug=?11"
        )?;
        stmt.bind((1, fin_name.as_str()))?;
        stmt.bind((2, fin_emoji.as_str()))?;
        stmt.bind((3, fin_prompt.as_str()))?;
        match &fin_notes {
            Some(v) => stmt.bind((4, v.as_str()))?,
            None    => stmt.bind((4, sqlite::Value::Null))?,
        }
        match fin_hb {
            Some(v) => stmt.bind((5, v))?,
            None    => stmt.bind((5, sqlite::Value::Null))?,
        }
        stmt.bind((6, fin_pro.as_str()))?;
        stmt.bind((7, fin_rl))?;
        stmt.bind((8, fin_en))?;
        match &fin_lr {
            Some(v) => stmt.bind((9, v.as_str()))?,
            None    => stmt.bind((9, sqlite::Value::Null))?,
        }
        stmt.bind((10, now.as_str()))?;
        stmt.bind((11, slug))?;
        drain(&mut stmt)?;

        let mut chk = db.prepare("SELECT changes()")?;
        if chk.next()? == State::Row {
            Ok(chk.read::<i64, _>(0)? > 0)
        } else {
            Ok(false)
        }
    }

    /// Delete a persona and cascade to all child tables.
    ///
    /// Cascade only works when `PRAGMA foreign_keys = ON` is set for the
    /// connection.  We enable it here before the delete.
    pub fn delete_persona(&self, slug: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        db.execute("PRAGMA foreign_keys = ON")?;
        let mut stmt = db.prepare("DELETE FROM personas WHERE slug=?1")?;
        stmt.bind((1, slug))?;
        drain(&mut stmt)
    }

    // ─── persona_sources ──────────────────────────────────────────────────────

    /// Add a source binding for a persona.  Silently does nothing on duplicate.
    pub fn add_persona_source(
        &self,
        persona_slug: &str,
        account_id: &str,
        selector_kind: &str,
        selector_value: Option<&str>,
        include: bool,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT OR IGNORE INTO persona_sources
                (persona_slug,account_id,selector_kind,selector_value,include,created_at)
             VALUES(?1,?2,?3,?4,?5,?6)"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, selector_kind))?;
        match selector_value {
            Some(v) => stmt.bind((4, v))?,
            None    => stmt.bind((4, sqlite::Value::Null))?,
        }
        stmt.bind((5, if include { 1_i64 } else { 0_i64 }))?;
        stmt.bind((6, now.as_str()))?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    /// List source bindings for a persona.
    pub fn list_persona_sources(
        &self,
        persona_slug: &str,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,persona_slug,account_id,selector_kind,selector_value,include,created_at
             FROM persona_sources WHERE persona_slug=?1 ORDER BY id"
        )?;
        stmt.bind((1, persona_slug))?;
        let mut out = Vec::new();
        while stmt.next()? == State::Row {
            let sv: Option<String> = match stmt.read::<sqlite::Value, _>(4)? {
                sqlite::Value::String(s) => Some(s), _ => None };
            out.push(serde_json::json!({
                "id":             stmt.read::<i64, _>(0)?,
                "persona_slug":   stmt.read::<String, _>(1)?,
                "account_id":     stmt.read::<String, _>(2)?,
                "selector_kind":  stmt.read::<String, _>(3)?,
                "selector_value": sv,
                "include":        stmt.read::<i64, _>(5)? != 0,
                "created_at":     stmt.read::<String, _>(6)?,
            }));
        }
        Ok(out)
    }

    /// Remove a source binding by its primary key.
    pub fn remove_persona_source(&self, source_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM persona_sources WHERE id=?1")?;
        stmt.bind((1, source_id))?;
        drain(&mut stmt)
    }

    // ─── persona_tool_whitelist ───────────────────────────────────────────────

    /// Add a tool to the persona's whitelist.  Silently ignores duplicates.
    pub fn add_persona_tool(&self, persona_slug: &str, tool_name: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT OR IGNORE INTO persona_tool_whitelist(persona_slug,tool_name)
             VALUES(?1,?2)"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, tool_name))?;
        drain(&mut stmt)
    }

    /// Remove a tool from the whitelist.
    pub fn remove_persona_tool(
        &self,
        persona_slug: &str,
        tool_name: &str,
    ) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM persona_tool_whitelist WHERE persona_slug=?1 AND tool_name=?2"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, tool_name))?;
        drain(&mut stmt)
    }

    /// List all tools in the whitelist for a persona.
    pub fn list_persona_tools(&self, persona_slug: &str) -> Result<Vec<String>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT tool_name FROM persona_tool_whitelist
             WHERE persona_slug=?1 ORDER BY tool_name"
        )?;
        stmt.bind((1, persona_slug))?;
        let mut out = Vec::new();
        while stmt.next()? == State::Row {
            out.push(stmt.read::<String, _>(0)?);
        }
        Ok(out)
    }

    // ─── persona_facts ────────────────────────────────────────────────────────

    /// Insert a persona fact and return its generated id.
    pub fn add_persona_fact(
        &self,
        persona_slug: &str,
        category: Option<&str>,
        fact_text: &str,
        pinned: bool,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO persona_facts
                (persona_slug,category,fact_text,pinned,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6)"
        )?;
        stmt.bind((1, persona_slug))?;
        match category {
            Some(v) => stmt.bind((2, v))?,
            None    => stmt.bind((2, sqlite::Value::Null))?,
        }
        stmt.bind((3, fact_text))?;
        stmt.bind((4, if pinned { 1_i64 } else { 0_i64 }))?;
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

    /// List facts for a persona.  `pinned_only = true` restricts to pinned rows.
    pub fn list_persona_facts(
        &self,
        persona_slug: &str,
        pinned_only: bool,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let sql = if pinned_only {
            "SELECT id,persona_slug,category,fact_text,pinned,created_at,updated_at
             FROM persona_facts WHERE persona_slug=?1 AND pinned=1 ORDER BY id"
        } else {
            "SELECT id,persona_slug,category,fact_text,pinned,created_at,updated_at
             FROM persona_facts WHERE persona_slug=?1 ORDER BY id"
        };
        let mut stmt = db.prepare(sql)?;
        stmt.bind((1, persona_slug))?;
        collect_persona_facts(&mut stmt)
    }

    /// Delete a persona fact by primary key.
    pub fn remove_persona_fact(&self, fact_id: i64) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM persona_facts WHERE id=?1")?;
        stmt.bind((1, fact_id))?;
        drain(&mut stmt)
    }

    /// Delete all facts for a persona (bulk forget).
    pub fn forget_all_persona_facts(&self, persona_slug: &str) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare("DELETE FROM persona_facts WHERE persona_slug=?1")?;
        stmt.bind((1, persona_slug))?;
        drain(&mut stmt)
    }

    // ─── persona_outbound_allowlist ───────────────────────────────────────────

    /// Upsert an entry in the outbound allowlist.
    pub fn set_persona_outbound_allow(
        &self,
        persona_slug: &str,
        account_id: &str,
        chat_id: &str,
        max_messages_per_day: i64,
    ) -> Result<(), MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO persona_outbound_allowlist
                (persona_slug,account_id,chat_id,max_messages_per_day,created_at)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(persona_slug,account_id,chat_id) DO UPDATE SET
                max_messages_per_day = excluded.max_messages_per_day"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, chat_id))?;
        stmt.bind((4, max_messages_per_day))?;
        stmt.bind((5, now.as_str()))?;
        drain(&mut stmt)
    }

    /// Remove an outbound allowlist entry.
    pub fn remove_persona_outbound_allow(
        &self,
        persona_slug: &str,
        account_id: &str,
        chat_id: &str,
    ) -> Result<(), MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM persona_outbound_allowlist
             WHERE persona_slug=?1 AND account_id=?2 AND chat_id=?3"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, account_id))?;
        stmt.bind((3, chat_id))?;
        drain(&mut stmt)
    }

    /// List all outbound allowlist entries for a persona.
    pub fn list_persona_outbound_allows(
        &self,
        persona_slug: &str,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT persona_slug,account_id,chat_id,max_messages_per_day,created_at
             FROM persona_outbound_allowlist
             WHERE persona_slug=?1 ORDER BY account_id,chat_id"
        )?;
        stmt.bind((1, persona_slug))?;
        let mut out = Vec::new();
        while stmt.next()? == State::Row {
            out.push(serde_json::json!({
                "persona_slug":         stmt.read::<String, _>(0)?,
                "account_id":           stmt.read::<String, _>(1)?,
                "chat_id":              stmt.read::<String, _>(2)?,
                "max_messages_per_day": stmt.read::<i64, _>(3)?,
                "created_at":           stmt.read::<String, _>(4)?,
            }));
        }
        Ok(out)
    }

    // ─── persona_audit ────────────────────────────────────────────────────────

    /// Append an audit entry.
    pub fn record_persona_audit(
        &self,
        persona_slug: &str,
        actor: &str,
        action: &str,
        target_account: Option<&str>,
        target_chat: Option<&str>,
        payload_json: Option<&str>,
        result: &str,
        error_msg: Option<&str>,
    ) -> Result<i64, MemoryError> {
        let now = now_iso8601();
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "INSERT INTO persona_audit
                (persona_slug,occurred_at,actor,action,
                 target_account,target_chat,payload_json,result,error_msg)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, now.as_str()))?;
        stmt.bind((3, actor))?;
        stmt.bind((4, action))?;
        bind_opt_str(&mut stmt, 5, target_account)?;
        bind_opt_str(&mut stmt, 6, target_chat)?;
        bind_opt_str(&mut stmt, 7, payload_json)?;
        stmt.bind((8, result))?;
        bind_opt_str(&mut stmt, 9, error_msg)?;
        drain(&mut stmt)?;

        let mut id_stmt = db.prepare("SELECT last_insert_rowid()")?;
        if id_stmt.next()? == State::Row {
            Ok(id_stmt.read::<i64, _>(0)?)
        } else {
            Err(MemoryError::Sqlite("last_insert_rowid returned no row".to_string()))
        }
    }

    /// Fetch the most recent `limit` audit rows for a persona (newest first).
    pub fn list_persona_audit(
        &self,
        persona_slug: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "SELECT id,persona_slug,occurred_at,actor,action,
                    target_account,target_chat,payload_json,result,error_msg
             FROM persona_audit WHERE persona_slug=?1
             ORDER BY occurred_at DESC LIMIT ?2"
        )?;
        stmt.bind((1, persona_slug))?;
        stmt.bind((2, limit))?;
        collect_persona_audit(&mut stmt)
    }

    /// Prune audit rows older than `cutoff_iso8601` across ALL personas.
    /// Intended to be called once per day from the poly-host scheduler.
    /// Returns the number of rows deleted.
    pub fn prune_persona_audit_before(&self, cutoff_iso8601: &str) -> Result<u64, MemoryError> {
        let db = self.lock()?;
        let mut stmt = db.prepare(
            "DELETE FROM persona_audit WHERE occurred_at < ?1"
        )?;
        stmt.bind((1, cutoff_iso8601))?;
        drain(&mut stmt)?;

        let mut chk = db.prepare("SELECT changes()")?;
        if chk.next()? == State::Row {
            Ok(chk.read::<i64, _>(0)? as u64)
        } else {
            Ok(0)
        }
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

/// Bind an `Option<&str>` to a positional parameter (NULL when `None`).
fn bind_opt_str(
    stmt: &mut sqlite::Statement<'_>,
    pos: usize,
    val: Option<&str>,
) -> Result<(), MemoryError> {
    match val {
        Some(v) => stmt.bind((pos, v))?,
        None    => stmt.bind((pos, sqlite::Value::Null))?,
    }
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

/// Read a single `personas` row.  Column order matches `get_persona` / `list_personas`.
/// 0=slug 1=name 2=avatar_emoji 3=system_prompt 4=style_notes
/// 5=heartbeat_interval_secs 6=proactivity 7=rate_limit_per_hour
/// 8=created_at 9=updated_at 10=last_run_at 11=enabled
fn read_persona_row(stmt: &mut sqlite::Statement<'_>) -> Result<serde_json::Value, MemoryError> {
    let style_notes: Option<String> = match stmt.read::<sqlite::Value, _>(4)? {
        sqlite::Value::String(s) => Some(s), _ => None };
    let hb: Option<i64> = match stmt.read::<sqlite::Value, _>(5)? {
        sqlite::Value::Integer(v) => Some(v), _ => None };
    let last_run: Option<String> = match stmt.read::<sqlite::Value, _>(10)? {
        sqlite::Value::String(s) => Some(s), _ => None };
    Ok(serde_json::json!({
        "slug":                    stmt.read::<String, _>(0)?,
        "name":                    stmt.read::<String, _>(1)?,
        "avatar_emoji":            stmt.read::<String, _>(2)?,
        "system_prompt":           stmt.read::<String, _>(3)?,
        "style_notes":             style_notes,
        "heartbeat_interval_secs": hb,
        "proactivity":             stmt.read::<String, _>(6)?,
        "rate_limit_per_hour":     stmt.read::<i64, _>(7)?,
        "created_at":              stmt.read::<String, _>(8)?,
        "updated_at":              stmt.read::<String, _>(9)?,
        "last_run_at":             last_run,
        "enabled":                 stmt.read::<i64, _>(11)? != 0,
    }))
}

fn collect_persona_facts(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        let cat: Option<String> = match stmt.read::<sqlite::Value, _>(2)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        out.push(serde_json::json!({
            "id":           stmt.read::<i64, _>(0)?,
            "persona_slug": stmt.read::<String, _>(1)?,
            "category":     cat,
            "fact_text":    stmt.read::<String, _>(3)?,
            "pinned":       stmt.read::<i64, _>(4)? != 0,
            "created_at":   stmt.read::<String, _>(5)?,
            "updated_at":   stmt.read::<String, _>(6)?,
        }));
    }
    Ok(out)
}

fn collect_persona_audit(
    stmt: &mut sqlite::Statement<'_>,
) -> Result<Vec<serde_json::Value>, MemoryError> {
    let mut out = Vec::new();
    while stmt.next()? == State::Row {
        let ta: Option<String> = match stmt.read::<sqlite::Value, _>(5)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        let tc: Option<String> = match stmt.read::<sqlite::Value, _>(6)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        let pj: Option<String> = match stmt.read::<sqlite::Value, _>(7)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        let em: Option<String> = match stmt.read::<sqlite::Value, _>(9)? {
            sqlite::Value::String(s) => Some(s), _ => None };
        out.push(serde_json::json!({
            "id":             stmt.read::<i64, _>(0)?,
            "persona_slug":   stmt.read::<String, _>(1)?,
            "occurred_at":    stmt.read::<String, _>(2)?,
            "actor":          stmt.read::<String, _>(3)?,
            "action":         stmt.read::<String, _>(4)?,
            "target_account": ta,
            "target_chat":    tc,
            "payload_json":   pj,
            "result":         stmt.read::<String, _>(8)?,
            "error_msg":      em,
        }));
    }
    Ok(out)
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

    // ── personas ─────────────────────────────────────────────────────────────

    #[test]
    fn create_and_get_persona() {
        let db = fresh_db();
        let slug = db.create_persona(
            "broker-bob", "Broker Bob", "💼",
            "You are my finance broker.", None, None,
            "drafts-only", 4,
        ).unwrap();
        assert_eq!(slug, "broker-bob");

        let p = db.get_persona("broker-bob").unwrap().unwrap();
        assert_eq!(p["slug"], "broker-bob");
        assert_eq!(p["name"], "Broker Bob");
        assert_eq!(p["avatar_emoji"], "💼");
        assert_eq!(p["system_prompt"], "You are my finance broker.");
        assert!(p["style_notes"].is_null());
        assert!(p["heartbeat_interval_secs"].is_null());
        assert_eq!(p["proactivity"], "drafts-only");
        assert_eq!(p["rate_limit_per_hour"], 4);
        assert_eq!(p["enabled"], true);
        assert!(p["last_run_at"].is_null());
    }

    #[test]
    fn get_persona_returns_none_when_missing() {
        let db = fresh_db();
        assert!(db.get_persona("nonexistent").unwrap().is_none());
    }

    #[test]
    fn list_personas_ordered_by_name() {
        let db = fresh_db();
        db.create_persona("zzz", "Zzz", "🤖", "prompt", None, None, "drafts-only", 4).unwrap();
        db.create_persona("aaa", "Aaa", "🤖", "prompt", None, None, "drafts-only", 4).unwrap();

        let list = db.list_personas().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["slug"], "aaa");
        assert_eq!(list[1]["slug"], "zzz");
    }

    #[test]
    fn update_persona_partial() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "old prompt", None, None, "drafts-only", 4).unwrap();

        let updated = db.update_persona(
            "bob",
            Some("Bob Updated"), // name
            None,                // avatar unchanged
            Some("new prompt"),  // system_prompt
            None,                // style_notes unchanged
            None,                // heartbeat unchanged
            None,                // proactivity unchanged
            Some(8),             // rate_limit changed
            None,                // enabled unchanged
            None,                // last_run_at unchanged
        ).unwrap();
        assert!(updated);

        let p = db.get_persona("bob").unwrap().unwrap();
        assert_eq!(p["name"], "Bob Updated");
        assert_eq!(p["system_prompt"], "new prompt");
        assert_eq!(p["rate_limit_per_hour"], 8);
        assert_eq!(p["avatar_emoji"], "🤖");   // preserved
        assert_eq!(p["proactivity"], "drafts-only"); // preserved
    }

    #[test]
    fn update_persona_nonexistent_returns_false() {
        let db = fresh_db();
        let updated = db.update_persona(
            "ghost", None, None, None, None, None, None, None, None, None,
        ).unwrap();
        assert!(!updated);
    }

    #[test]
    fn delete_persona_cascades() {
        let db = fresh_db();
        db.create_persona("frag-frank", "Frag Frank", "🎮", "hype-man", None, None, "notify", 4).unwrap();

        // Add child rows to each child table.
        db.add_persona_source("frag-frank", "discord-1", "server", Some("guild-1"), true).unwrap();
        db.add_persona_tool("frag-frank", "get_messages").unwrap();
        db.add_persona_fact("frag-frank", Some("observation"), "raid tonight", false).unwrap();
        db.set_persona_outbound_allow("frag-frank", "discord-1", "channel-1", 1).unwrap();
        db.record_persona_audit(
            "frag-frank", "user", "invoke", None, None, None, "ok", None,
        ).unwrap();

        db.delete_persona("frag-frank").unwrap();

        // Parent row gone.
        assert!(db.get_persona("frag-frank").unwrap().is_none());
        // All child tables empty.
        assert!(db.list_persona_sources("frag-frank").unwrap().is_empty());
        assert!(db.list_persona_tools("frag-frank").unwrap().is_empty());
        assert!(db.list_persona_facts("frag-frank", false).unwrap().is_empty());
        assert!(db.list_persona_outbound_allows("frag-frank").unwrap().is_empty());
        assert!(db.list_persona_audit("frag-frank", 100).unwrap().is_empty());
    }

    // ── persona_sources ───────────────────────────────────────────────────────

    #[test]
    fn add_and_list_persona_sources() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();

        db.add_persona_source("bob", "discord-1", "server", Some("guild-A"), true).unwrap();
        db.add_persona_source("bob", "discord-1", "channel", Some("ch-deny"), false).unwrap();

        let sources = db.list_persona_sources("bob").unwrap();
        assert_eq!(sources.len(), 2);
        let allow = sources.iter().find(|s| s["include"] == true).unwrap();
        assert_eq!(allow["selector_kind"], "server");
        let deny = sources.iter().find(|s| s["include"] == false).unwrap();
        assert_eq!(deny["selector_value"], "ch-deny");
    }

    #[test]
    fn add_persona_source_duplicate_is_noop() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        // Use a non-NULL selector_value so the UNIQUE constraint fires correctly
        // (SQLite treats two NULLs as distinct in UNIQUE constraints).
        db.add_persona_source("bob", "discord-1", "server", Some("guild-A"), true).unwrap();
        db.add_persona_source("bob", "discord-1", "server", Some("guild-A"), true).unwrap(); // duplicate
        assert_eq!(db.list_persona_sources("bob").unwrap().len(), 1);
    }

    #[test]
    fn remove_persona_source() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        let id = db.add_persona_source("bob", "discord-1", "server", Some("g"), true).unwrap();
        db.remove_persona_source(id).unwrap();
        assert!(db.list_persona_sources("bob").unwrap().is_empty());
    }

    // ── persona_tool_whitelist ────────────────────────────────────────────────

    #[test]
    fn add_list_remove_persona_tools() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        db.add_persona_tool("bob", "get_messages").unwrap();
        db.add_persona_tool("bob", "draft_create").unwrap();
        db.add_persona_tool("bob", "get_messages").unwrap(); // dup — ignored

        let tools = db.list_persona_tools("bob").unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"draft_create".to_string()));

        db.remove_persona_tool("bob", "draft_create").unwrap();
        let tools = db.list_persona_tools("bob").unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0], "get_messages");
    }

    // ── persona_facts ─────────────────────────────────────────────────────────

    #[test]
    fn add_and_list_persona_facts() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        let id1 = db.add_persona_fact("bob", Some("observation"), "user likes ETH", false).unwrap();
        let id2 = db.add_persona_fact("bob", Some("reminder"), "check earnings Friday", true).unwrap();
        assert!(id1 > 0 && id2 > 0);

        let all = db.list_persona_facts("bob", false).unwrap();
        assert_eq!(all.len(), 2);

        let pinned = db.list_persona_facts("bob", true).unwrap();
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned[0]["fact_text"], "check earnings Friday");
        assert_eq!(pinned[0]["pinned"], true);
    }

    #[test]
    fn remove_persona_fact() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        let id = db.add_persona_fact("bob", None, "temporary", false).unwrap();
        db.remove_persona_fact(id).unwrap();
        assert!(db.list_persona_facts("bob", false).unwrap().is_empty());
    }

    #[test]
    fn forget_all_persona_facts() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        db.add_persona_fact("bob", None, "fact 1", false).unwrap();
        db.add_persona_fact("bob", None, "fact 2", true).unwrap();
        db.forget_all_persona_facts("bob").unwrap();
        assert!(db.list_persona_facts("bob", false).unwrap().is_empty());
    }

    // ── persona_outbound_allowlist ────────────────────────────────────────────

    #[test]
    fn set_and_list_outbound_allow() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "outbound-allowlisted", 4).unwrap();
        db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 2).unwrap();
        db.set_persona_outbound_allow("bob", "discord-1", "channel-2", 1).unwrap();

        let allows = db.list_persona_outbound_allows("bob").unwrap();
        assert_eq!(allows.len(), 2);
        let a = allows.iter().find(|a| a["chat_id"] == "channel-1").unwrap();
        assert_eq!(a["max_messages_per_day"], 2);
    }

    #[test]
    fn set_outbound_allow_upsert() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "outbound-allowlisted", 4).unwrap();
        db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 1).unwrap();
        db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 5).unwrap(); // upsert
        let allows = db.list_persona_outbound_allows("bob").unwrap();
        assert_eq!(allows.len(), 1);
        assert_eq!(allows[0]["max_messages_per_day"], 5);
    }

    #[test]
    fn remove_outbound_allow() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "outbound-allowlisted", 4).unwrap();
        db.set_persona_outbound_allow("bob", "discord-1", "channel-1", 1).unwrap();
        db.remove_persona_outbound_allow("bob", "discord-1", "channel-1").unwrap();
        assert!(db.list_persona_outbound_allows("bob").unwrap().is_empty());
    }

    // ── persona_audit ─────────────────────────────────────────────────────────

    #[test]
    fn record_and_list_persona_audit() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        let id1 = db.record_persona_audit(
            "bob", "user", "invoke",
            Some("discord-1"), Some("channel-1"),
            Some("{\"msgs\":5}"), "ok", None,
        ).unwrap();
        let id2 = db.record_persona_audit(
            "bob", "heartbeat", "heartbeat_run",
            None, None, None, "ok", None,
        ).unwrap();
        assert!(id1 > 0 && id2 > 0);

        let rows = db.list_persona_audit("bob", 50).unwrap();
        assert_eq!(rows.len(), 2);
        // list returns newest first
        let invoke_row = rows.iter().find(|r| r["action"] == "invoke").unwrap();
        assert_eq!(invoke_row["actor"], "user");
        assert_eq!(invoke_row["target_account"], "discord-1");
        assert_eq!(invoke_row["result"], "ok");
        assert!(invoke_row["error_msg"].is_null());
    }

    #[test]
    fn record_audit_with_error() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        db.record_persona_audit(
            "bob", "heartbeat", "heartbeat_run",
            None, None, None, "error", Some("backend timeout"),
        ).unwrap();

        let rows = db.list_persona_audit("bob", 10).unwrap();
        assert_eq!(rows[0]["result"], "error");
        assert_eq!(rows[0]["error_msg"], "backend timeout");
    }

    #[test]
    fn prune_persona_audit_before_cutoff() {
        let db = fresh_db();
        db.create_persona("bob", "Bob", "🤖", "p", None, None, "drafts-only", 4).unwrap();
        // Insert two rows at "current" time; we can't control the timestamp,
        // so prune with a future cutoff to delete both.
        db.record_persona_audit("bob", "user", "invoke", None, None, None, "ok", None).unwrap();
        db.record_persona_audit("bob", "user", "invoke", None, None, None, "ok", None).unwrap();

        let deleted = db.prune_persona_audit_before("2099-01-01T00:00:00Z").unwrap();
        assert_eq!(deleted, 2);
        assert!(db.list_persona_audit("bob", 10).unwrap().is_empty());
    }

    #[test]
    fn migration_is_idempotent() {
        // Opening a second MemoryDb on the same ":memory:" path would give a
        // new DB.  Instead, call run_migrations again on the same connection.
        let db = fresh_db();
        // This must not fail — all CREATE TABLE IF NOT EXISTS.
        let guard = db.db.lock().unwrap();
        MemoryDb::run_migrations(&guard).unwrap();
    }
}
