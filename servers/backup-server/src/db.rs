//! SurrealKV database initialization and schema.
//!
//! Pattern mirrors `poly-server/src/db.rs`: open embedded SurrealKV,
//! select namespace/database, then apply an idempotent `DEFINE … OVERWRITE` schema.
//!
//! Permissions are enforced in Rust handlers, not SurrealQL — consistent with the
//! DECISION(DX-STORAGE-1) established in poly-core.

use surrealdb::{
    Surreal,
    engine::local::{Db as SurrealDb, SurrealKv},
};
use tracing::info;

use crate::config::Config;

/// Type alias for our embedded SurrealDB handle.
pub type Db = Surreal<SurrealDb>;

/// Initialize SurrealKV, select namespace/database, and run schema migrations.
// Sequential init + migrations — splitting would obscure the startup flow.
#[allow(clippy::cognitive_complexity)]
pub async fn init(config: &Config) -> anyhow::Result<Db> {
    let db_path = config.data_dir.join("backup.db");
    info!("Opening SurrealKV at {}", db_path.display());

    let db = Surreal::new::<SurrealKv>(db_path).await?;
    db.use_ns("poly").use_db("backup").await?;
    db.query(SCHEMA).await?.check()?;

    info!("Backup server database schema applied");
    Ok(db)
}

/// Idempotent SurrealQL schema for the backup server.
///
/// Uses `DEFINE … OVERWRITE` so re-applying on every restart is safe.
const SCHEMA: &str = r"
-- Registered accounts (identified by Ed25519 public key)
DEFINE TABLE OVERWRITE account SCHEMAFULL;
DEFINE FIELD OVERWRITE public_key    ON account TYPE string;
DEFINE FIELD OVERWRITE registered_at ON account TYPE string;
DEFINE FIELD OVERWRITE last_seen_at  ON account TYPE string;
DEFINE INDEX OVERWRITE account_pk    ON account COLUMNS public_key UNIQUE;

-- Session tokens (stored as SHA-256 hashes — never raw)
DEFINE TABLE OVERWRITE token SCHEMAFULL;
DEFINE FIELD OVERWRITE token_hash   ON token TYPE string;
DEFINE FIELD OVERWRITE public_key   ON token TYPE string;
DEFINE FIELD OVERWRITE device_name  ON token TYPE string;
DEFINE FIELD OVERWRITE created_at   ON token TYPE string;
DEFINE FIELD OVERWRITE last_seen_at ON token TYPE string;
DEFINE FIELD OVERWRITE expires_at   ON token TYPE string;
DEFINE INDEX OVERWRITE token_hash_idx ON token COLUMNS token_hash UNIQUE;

-- Encrypted settings blobs — append-only per account
DEFINE TABLE OVERWRITE sync_blob SCHEMAFULL;
DEFINE FIELD OVERWRITE public_key     ON sync_blob TYPE string;
DEFINE FIELD OVERWRITE sequence       ON sync_blob TYPE int;
DEFINE FIELD OVERWRITE encrypted_blob ON sync_blob TYPE string;
DEFINE FIELD OVERWRITE pushed_at      ON sync_blob TYPE string;
DEFINE INDEX OVERWRITE blob_pk_seq    ON sync_blob COLUMNS public_key, sequence UNIQUE;

-- Short-lived PoW challenges for API auth
DEFINE TABLE OVERWRITE challenge SCHEMAFULL;
DEFINE FIELD OVERWRITE nonce       ON challenge TYPE string;
DEFINE FIELD OVERWRITE public_key  ON challenge TYPE string;
DEFINE FIELD OVERWRITE difficulty  ON challenge TYPE int;
DEFINE FIELD OVERWRITE created_at  ON challenge TYPE string;
DEFINE FIELD OVERWRITE expires_at  ON challenge TYPE string;
DEFINE INDEX OVERWRITE challenge_nonce ON challenge COLUMNS nonce UNIQUE;

-- Per-IP rate-limit counters for API auth endpoints
DEFINE TABLE OVERWRITE rate_limit SCHEMAFULL;
DEFINE FIELD OVERWRITE ip           ON rate_limit TYPE string;
DEFINE FIELD OVERWRITE failures     ON rate_limit TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE window_start ON rate_limit TYPE string;
DEFINE INDEX OVERWRITE rate_limit_ip ON rate_limit COLUMNS ip UNIQUE;
";

// ── DB record types (deserialization targets) ──────────────────────────────────

/// A registered account record from the `account` table.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AccountRecord {
    /// SurrealDB record ID — `account:<ulid>`.
    pub id: Option<serde_json::Value>,
    pub public_key: String,
    pub registered_at: String,
    pub last_seen_at: String,
}

/// A session token record from the `token` table.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TokenRecord {
    /// SurrealDB record ID.
    pub id: Option<serde_json::Value>,
    pub token_hash: String,
    pub public_key: String,
    pub device_name: String,
    pub created_at: String,
    pub last_seen_at: String,
    pub expires_at: String,
}

/// A sync blob entry from the `sync_blob` table.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SyncBlobRecord {
    pub id: Option<serde_json::Value>,
    pub public_key: String,
    pub sequence: i64,
    pub encrypted_blob: String,
    pub pushed_at: String,
}

/// A PoW challenge record.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ChallengeRecord {
    pub nonce: String,
    pub public_key: String,
    pub difficulty: i64,
    pub created_at: String,
    pub expires_at: String,
}
