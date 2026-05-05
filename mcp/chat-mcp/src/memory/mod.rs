//! Memory store for the Poly chat-mcp server.
//!
//! All tables live in Poly's main `storage.sqlite3` (or an in-memory DB for
//! tests). This module owns the schema migration and re-exports every type
//! callers need.

use std::sync::{Arc, Mutex};

use sqlite::{Connection, ConnectionThreadSafe};

pub mod chat_style;
pub mod client_settings;
pub mod drafts;
pub mod facts;
pub(super) mod helpers;
pub mod notes_summaries;
pub mod persona;

#[cfg(test)]
mod tests;

pub use chat_style::ChatStyle;
pub use persona::{QueryPersonaAuditArgs, UpdatePersonaArgs};

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
    pub(crate) db: Arc<Mutex<ConnectionThreadSafe>>,
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

    pub(crate) fn run_migrations(db: &ConnectionThreadSafe) -> Result<(), MemoryError> {
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
        ).map_err(|e| MemoryError::Sqlite(e.to_string()))?;

        // ── Phase D — client_settings_audit schema ─────────────────────────────
        // Separate from persona_audit: not persona-scoped, uses slug="system"
        // as a synthetic marker. The cross-persona lint (Q.1) does not scan
        // this table — it has its own helper `record_client_settings_audit`.
        db.execute(
            "CREATE TABLE IF NOT EXISTS client_settings_audit (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                slug         TEXT NOT NULL DEFAULT 'system',
                backend_id   TEXT NOT NULL,
                action       TEXT NOT NULL,
                payload_json TEXT,
                status       TEXT NOT NULL,
                error_msg    TEXT,
                created_at   TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_client_settings_audit_backend_time
                ON client_settings_audit(backend_id, created_at DESC);"
        ).map_err(|e| MemoryError::Sqlite(e.to_string()))?;

        // ── Phase G — quiet_hours_disabled column (additive migration) ─────────
        // ALTER TABLE ignores the error if the column already exists so repeated
        // open() calls are safe.
        drop(db.execute(
            "ALTER TABLE personas ADD COLUMN quiet_hours_disabled INTEGER NOT NULL DEFAULT 0"
        ));

        Ok(())
    }

    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, ConnectionThreadSafe>, MemoryError> {
        // poly-lint: PoisonError carries no useful payload past "mutex poisoned".
        #[allow(clippy::map_err_ignore)]
        self.db
            .lock()
            .map_err(|_| MemoryError::Sqlite("mutex poisoned".to_string()))
    }
}
