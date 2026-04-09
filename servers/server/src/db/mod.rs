//! Database abstraction layer.
//!
//! Two backends, selected at compile time:
//! - `db-surreal`: SurrealDB — supports both network (`ws://`) and embedded (`surrealkv://`) via `SURREAL_URL`.
//! - `db-sqlite` (default): SQLite — lightweight, zero external dependencies.
//!
//! Both export the same `Db` type with identical async methods.
//! Handlers never touch raw queries — they call `state.db.some_operation()`.

// At least one backend must be selected at compile time.
#[cfg(not(any(feature = "db-surreal", feature = "db-sqlite")))]
compile_error!("A database backend must be enabled: db-surreal or db-sqlite.");

#[cfg(all(feature = "db-surreal", not(feature = "db-sqlite")))]
mod surreal;

#[cfg(feature = "db-sqlite")]
mod sqlite;

// When db-sqlite is enabled (including when both are on, e.g. rust-analyzer),
// prefer sqlite. Only use surreal when it is the sole backend.
#[cfg(feature = "db-sqlite")]
pub use sqlite::Db;

#[cfg(all(feature = "db-surreal", not(feature = "db-sqlite")))]
pub use surreal::Db;

use crate::config::Config;

/// Initialize the database backend and run schema migrations.
pub async fn init(config: &Config) -> anyhow::Result<Db> {
    Db::init(config).await
}
