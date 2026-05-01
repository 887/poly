//! Draft queue read access for the Poly UI.
//!
//! The `drafts` table is written by `poly-chat-mcp` (Phase B) and lives in the
//! same `storage.sqlite3` as the rest of Poly's data. This module provides
//! **read-only** helper functions so the UI (`DraftBanner`, `DraftsSidebar`)
//! can display pending drafts without round-tripping through the MCP over HTTP.
//!
//! ## Platform gating
//!
//! Direct SQLite access is only available on native targets. On WASM the
//! functions return empty lists (no draft display on web-only builds without a
//! host bridge). Use the `cfg(not(target_arch = "wasm32"))` guard at call sites
//! or rely on the conditional helpers exported below.

use serde::{Deserialize, Serialize};

/// A pending draft as stored in the `drafts` table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Draft {
    pub id:           i64,
    pub account_id:   String,
    pub chat_id:      String,
    pub body:         String,
    pub suggested_by: String,
    pub created_at:   String,
    pub auto_send_at: Option<String>,
    pub status:       String,
}

// ─── Native (SQLite) implementation ──────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
pub use native_impl::*;

#[cfg(not(target_arch = "wasm32"))]
mod native_impl {
    use super::Draft;
    use sqlite::{Connection, State};
    use std::sync::{Arc, Mutex};

    /// Lightweight handle to the shared `storage.sqlite3`, scoped to draft reads.
    ///
    /// Cheap to clone. Deliberately separate from `Storage` (the KV handle) so
    /// draft queries don't touch the KV schema.
    #[derive(Clone)]
    pub struct DraftStore {
        db: Arc<Mutex<sqlite::ConnectionThreadSafe>>,
    }

    impl DraftStore {
        /// Open the `drafts` table in the shared Poly database.
        ///
        /// Returns `Ok(None)` if the database does not exist yet (MCP not run yet).
        #[must_use]
        pub fn try_open() -> Option<Self> {
            let path = crate::storage::poly_data_dir().join("storage.sqlite3");
            if !path.exists() {
                return None;
            }
            let mut db = Connection::open_thread_safe(&path).ok()?;
            db.set_busy_timeout(1_000).ok()?;
            Some(Self { db: Arc::new(Mutex::new(db)) })
        }

        /// Return all pending drafts for a specific `account_id` + `chat_id`.
        #[must_use]
        pub fn pending_for_chat(&self, account_id: &str, chat_id: &str) -> Vec<Draft> {
            let db = match self.db.lock() {
                Ok(d) => d,
                Err(_) => return Vec::new(),
            };
            let mut stmt = match db.prepare(
                "SELECT id,account_id,chat_id,body,suggested_by,created_at,auto_send_at,status
                 FROM drafts
                 WHERE account_id=?1 AND chat_id=?2 AND status='pending'
                 ORDER BY id"
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            if stmt.bind((1, account_id)).is_err() { return Vec::new(); }
            if stmt.bind((2, chat_id)).is_err()    { return Vec::new(); }

            collect_drafts_from_stmt(&mut stmt)
        }

        /// Return all pending drafts for an `account_id` across all chats.
        #[must_use]
        pub fn pending_for_account(&self, account_id: &str) -> Vec<Draft> {
            let db = match self.db.lock() {
                Ok(d) => d,
                Err(_) => return Vec::new(),
            };
            let mut stmt = match db.prepare(
                "SELECT id,account_id,chat_id,body,suggested_by,created_at,auto_send_at,status
                 FROM drafts
                 WHERE account_id=?1 AND status='pending'
                 ORDER BY id"
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            if stmt.bind((1, account_id)).is_err() { return Vec::new(); }

            collect_drafts_from_stmt(&mut stmt)
        }
    }

    fn collect_drafts_from_stmt(stmt: &mut sqlite::Statement<'_>) -> Vec<Draft> {
        let mut out = Vec::new();
        while let Ok(State::Row) = stmt.next() {
            let auto_send_at: Option<String> = match stmt.read::<sqlite::Value, _>(6) {
                Ok(sqlite::Value::String(s)) => Some(s),
                _ => None,
            };
            let draft = Draft {
                id:           stmt.read::<i64, _>(0).unwrap_or(0),
                account_id:   stmt.read::<String, _>(1).unwrap_or_default(),
                chat_id:      stmt.read::<String, _>(2).unwrap_or_default(),
                body:         stmt.read::<String, _>(3).unwrap_or_default(),
                suggested_by: stmt.read::<String, _>(4).unwrap_or_default(),
                created_at:   stmt.read::<String, _>(5).unwrap_or_default(),
                auto_send_at,
                status:       stmt.read::<String, _>(7).unwrap_or_default(),
            };
            out.push(draft);
        }
        out
    }
}

// ─── WASM stub ───────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub use wasm_stub::*;

#[cfg(target_arch = "wasm32")]
mod wasm_stub {
    use super::Draft;

    /// No-op draft store for WASM builds.
    ///
    /// Returns empty lists on every query — drafts are managed by the MCP
    /// (native process) and the UI simply shows nothing when built for
    /// browser targets without a host bridge.
    #[derive(Clone)]
    pub struct DraftStore;

    impl DraftStore {
        #[must_use] 
        pub fn try_open() -> Option<Self> {
            Some(Self)
        }

        #[must_use] 
        pub fn pending_for_chat(&self, _account_id: &str, _chat_id: &str) -> Vec<Draft> {
            Vec::new()
        }

        #[must_use] 
        pub fn pending_for_account(&self, _account_id: &str) -> Vec<Draft> {
            Vec::new()
        }
    }
}
