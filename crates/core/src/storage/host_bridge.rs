//! Host-bridge storage backend.
//!
//! Proxies every KV operation through [`poly_host_bridge::Client`] to a
//! native shell that owns the real storage file. Lets poly-web (and any
//! future pure-WASM shell without direct disk access) share one SQLite
//! database with the rest of the platform family.
//!
//! ## When this is selected
//!
//! `crates/core/src/storage/mod.rs` picks this implementation when the
//! `storage-host-bridge` feature is on. The feature is mutually
//! exclusive with the native SQLite/SurrealKV backends at compile time
//! — binaries pick one per platform.
//!
//! ## Required runtime piece
//!
//! A shell that mounts the `/host/kv/*` routes (phase 2.21 layout) must
//! be running at [`poly_host_bridge::BRIDGE_BASE_URL`]. That's one of:
//!
//! - `apps/desktop-web` — already has a webview, so it just adds the
//!   routes alongside its existing MCP eval bridge.
//! - `apps/desktop-electron-web` — same story via electron's Node
//!   HTTP server.
//! - `apps/poly-host` — the standalone daemon for `apps/web`, run as
//!   `cargo run -p poly-host` alongside `dx serve --platform web`.

use super::StorageError;
use poly_host_bridge::{BridgeError, Client as BridgeClient};

/// KV storage that routes every operation through the host bridge.
///
/// Cheap to clone — the inner `BridgeClient` wraps a `reqwest::Client`.
#[derive(Clone)]
pub struct StorageInner {
    client: BridgeClient,
}

fn to_storage_error(err: BridgeError) -> StorageError {
    StorageError::Backend(err.to_string())
}

impl StorageInner {
    /// Build a new client and verify the bridge is reachable.
    ///
    /// Returns `Ok` even if the ping fails — the first real `get`/`set`
    /// call will surface the error with a more useful context. We log a
    /// warning so dev users notice that the daemon isn't running.
    pub async fn init() -> Result<Self, StorageError> {
        let client = BridgeClient::new();
        if let Err(e) = client.status().await {
            tracing::warn!(
                "host-bridge storage: bridge not reachable yet ({e}). \
                 Is `cargo run -p poly-host` running?"
            );
        } else {
            tracing::info!("host-bridge storage: bridge reachable");
        }
        Ok(Self { client })
    }

    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>, StorageError> {
        self.client.kv_get(key).await.map_err(to_storage_error)
    }

    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<(), StorageError> {
        self.client.kv_set(key, value).await.map_err(to_storage_error)
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client.kv_delete(key).await.map_err(to_storage_error)
    }

    pub async fn clear_all(&self) -> Result<(), StorageError> {
        self.client.kv_clear().await.map_err(to_storage_error)
    }
}
