//! Client manager — manages active messenger backend connections.
//!
//! `ClientManager` holds `Arc<dyn ClientBackend>` instances keyed by account ID.
//! It provides methods to activate/deactivate the demo client and to look up
//! which backend owns a given server.
//!
//! Provided as `Signal<ClientManager>` at the `App` level.
// TODO(phase-2.5.1): Client Manager Module

use poly_client::{AuthCredentials, BackendType, ClientBackend, Server, Session};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A shared, thread-safe handle to a messenger backend.
pub type BackendHandle = Arc<RwLock<Box<dyn ClientBackend>>>;

/// Manages active messenger backend connections.
///
/// Each backend is keyed by its account ID (e.g., `"demo"` for the demo client).
/// Multiple accounts from the same backend type can be active simultaneously
/// (e.g., two Discord accounts, three Matrix accounts).
pub struct ClientManager {
    /// Active backends keyed by account ID.
    backends: HashMap<String, BackendHandle>,
    /// Whether the demo client is currently active.
    pub demo_active: bool,
    /// Cached mapping from server ID → account ID that owns it.
    server_account_map: HashMap<String, String>,
}

impl std::fmt::Debug for ClientManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientManager")
            .field("demo_active", &self.demo_active)
            .field("backend_count", &self.backends.len())
            .field("account_ids", &self.backends.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Default for ClientManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientManager {
    /// Create a new empty client manager.
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            demo_active: false,
            server_account_map: HashMap::new(),
        }
    }

    /// Activate the demo client.
    ///
    /// Creates a `DemoClient`, authenticates it, and adds it to the backends
    /// map with key `"demo"`. Sets `demo_active = true`.
    /// Returns the authenticated session so callers can record the local user.
    #[cfg(feature = "demo")]
    pub async fn activate_demo(&mut self) -> Result<Session, String> {
        if self.demo_active {
            // Return a fresh demo session even if already active
            return Ok(poly_demo::data::demo_session());
        }

        let mut client = poly_demo::DemoClient::new();
        let session = client
            .authenticate(AuthCredentials::Token("demo-token".to_string()))
            .await
            .map_err(|e| format!("Demo auth failed: {e}"))?;

        let account_id = "demo".to_string();
        self.backends
            .insert(account_id, Arc::new(RwLock::new(Box::new(client))));
        self.demo_active = true;

        // Rebuild server-account map
        self.rebuild_server_map().await;

        tracing::info!("Demo client activated");
        Ok(session)
    }

    /// Deactivate the demo client.
    ///
    /// Removes the demo backend from the map and clears demo-related cache.
    #[cfg(feature = "demo")]
    pub fn deactivate_demo(&mut self) {
        self.backends.remove("demo");
        self.demo_active = false;
        // Remove demo entries from server map
        self.server_account_map
            .retain(|_, account_id| account_id != "demo");
        tracing::info!("Demo client deactivated");
    }

    /// Get the backend for a specific account ID.
    pub fn get_backend(&self, account_id: &str) -> Option<BackendHandle> {
        self.backends.get(account_id).cloned()
    }

    /// Find which account owns a given server, return (account_id, backend_arc).
    pub fn get_backend_for_server(&self, server_id: &str) -> Option<(String, BackendHandle)> {
        let account_id = self.server_account_map.get(server_id)?;
        let backend = self.backends.get(account_id)?;
        Some((account_id.clone(), backend.clone()))
    }

    /// Get all servers from all active backends.
    pub async fn all_servers(&self) -> Vec<Server> {
        let mut servers = Vec::new();
        for backend in self.backends.values() {
            let guard = backend.read().await;
            if let Ok(mut s) = guard.get_servers().await {
                servers.append(&mut s);
            }
        }
        servers
    }

    /// Rebuild the server → account_id mapping cache.
    async fn rebuild_server_map(&mut self) {
        self.server_account_map.clear();
        for (account_id, backend) in &self.backends {
            let guard = backend.read().await;
            if let Ok(servers) = guard.get_servers().await {
                for server in servers {
                    self.server_account_map
                        .insert(server.id.clone(), account_id.clone());
                }
            }
        }
    }

    /// Get the list of active account IDs.
    pub fn active_account_ids(&self) -> Vec<String> {
        self.backends.keys().cloned().collect()
    }

    /// Get the backend type for a given account ID.
    pub async fn backend_type_for_account(&self, account_id: &str) -> Option<BackendType> {
        let backend = self.backends.get(account_id)?;
        let guard = backend.read().await;
        Some(guard.backend_type())
    }

    /// Disconnect all active accounts and clear runtime backend state.
    ///
    /// Calls `logout()` on each backend to close any live connections
    /// (including websocket/event streams where implemented), then clears
    /// all cached backend mappings.
    pub async fn disconnect_all(&mut self) {
        let mut backends = std::mem::take(&mut self.backends);
        for (account_id, backend) in backends.drain() {
            let mut guard = backend.write().await;
            if let Err(err) = guard.logout().await {
                tracing::warn!("Logout failed for account {account_id}: {err}");
            }
        }
        self.clear_all_backends();
    }

    /// Clear all runtime backend/cache state without calling backend APIs.
    pub fn clear_all_backends(&mut self) {
        self.backends.clear();
        self.server_account_map.clear();
        self.demo_active = false;
    }
}
