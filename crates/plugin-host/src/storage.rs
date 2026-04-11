//! Plugin storage backend trait and default in-memory implementation.
//!
//! [`PluginStorageBackend`] is the abstraction that allows plugin KV storage
//! to be swapped at runtime — the default [`InMemoryPluginStorage`] is used
//! for tests and development; a SQLite-backed implementation can be injected
//! for production via [`PluginRegistry::with_default_storage`].
//!
//! Keys are namespaced by `plugin_id` and scope (global vs. per-account) so
//! a single shared backend instance can safely serve multiple plugins.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

// ─── Trait ─────────────────────────────────────────────────────────────────

/// Storage abstraction for plugin-scoped key-value data.
///
/// Implementations must be `Send + Sync + 'static` so they can be shared
/// across async task boundaries and stored in `Arc<dyn PluginStorageBackend>`.
///
/// All methods receive `plugin_id` for namespacing, even though each
/// [`crate::host_impl::PluginHostState`] is bound to a single plugin.
/// This lets a single shared backend (e.g. SQLite) safely serve multiple
/// plugin instances without key collisions.
#[async_trait::async_trait]
pub trait PluginStorageBackend: Send + Sync + 'static {
    /// Read a value from plugin-global storage.
    async fn get(&self, plugin_id: &str, key: &str) -> Result<Option<Vec<u8>>, String>;
    /// Write a value to plugin-global storage.
    async fn set(&self, plugin_id: &str, key: &str, value: Vec<u8>) -> Result<(), String>;
    /// Delete a key from plugin-global storage.
    async fn delete(&self, plugin_id: &str, key: &str) -> Result<(), String>;

    /// Read a value from per-account plugin-scoped storage.
    async fn account_get(
        &self,
        plugin_id: &str,
        account: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>, String>;
    /// Write a value to per-account plugin-scoped storage.
    async fn account_set(
        &self,
        plugin_id: &str,
        account: &str,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), String>;
    /// Delete a key from per-account plugin-scoped storage.
    async fn account_delete(
        &self,
        plugin_id: &str,
        account: &str,
        key: &str,
    ) -> Result<(), String>;
}

// ─── In-memory default implementation ──────────────────────────────────────

/// Thread-safe in-memory implementation of [`PluginStorageBackend`].
///
/// Keys are stored internally as `"{plugin}:global:{key}"` for plugin-global
/// entries and `"{plugin}:account:{account}:{key}"` for per-account entries,
/// so the two namespaces are disjoint and share the same backing map.
///
/// This is the default storage used when no backend is explicitly injected.
/// It satisfies all tests and development scenarios; data is lost on process
/// exit.
#[derive(Default)]
pub struct InMemoryPluginStorage {
    inner: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryPluginStorage {
    /// Create a new empty in-memory storage.
    pub fn new() -> Self {
        Self::default()
    }

    fn global_key(plugin_id: &str, key: &str) -> String {
        format!("{plugin_id}:global:{key}")
    }

    fn account_key(plugin_id: &str, account: &str, key: &str) -> String {
        format!("{plugin_id}:account:{account}:{key}")
    }
}

#[async_trait::async_trait]
impl PluginStorageBackend for InMemoryPluginStorage {
    async fn get(&self, plugin_id: &str, key: &str) -> Result<Option<Vec<u8>>, String> {
        let map = self.inner.lock().await;
        Ok(map.get(&Self::global_key(plugin_id, key)).cloned())
    }

    async fn set(&self, plugin_id: &str, key: &str, value: Vec<u8>) -> Result<(), String> {
        let mut map = self.inner.lock().await;
        map.insert(Self::global_key(plugin_id, key), value);
        Ok(())
    }

    async fn delete(&self, plugin_id: &str, key: &str) -> Result<(), String> {
        let mut map = self.inner.lock().await;
        map.remove(&Self::global_key(plugin_id, key));
        Ok(())
    }

    async fn account_get(
        &self,
        plugin_id: &str,
        account: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let map = self.inner.lock().await;
        Ok(map
            .get(&Self::account_key(plugin_id, account, key))
            .cloned())
    }

    async fn account_set(
        &self,
        plugin_id: &str,
        account: &str,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), String> {
        let mut map = self.inner.lock().await;
        map.insert(Self::account_key(plugin_id, account, key), value);
        Ok(())
    }

    async fn account_delete(
        &self,
        plugin_id: &str,
        account: &str,
        key: &str,
    ) -> Result<(), String> {
        let mut map = self.inner.lock().await;
        map.remove(&Self::account_key(plugin_id, account, key));
        Ok(())
    }
}
