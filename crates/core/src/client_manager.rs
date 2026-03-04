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
/// Each backend is keyed by its account ID (e.g., `"demo-cat"` for the cat demo client).
/// Multiple accounts from the same backend type can be active simultaneously
/// (e.g., two Discord accounts, three Matrix accounts).
pub struct ClientManager {
    /// Active backends keyed by account ID.
    backends: HashMap<String, BackendHandle>,
    /// Whether the demo client is currently active.
    pub demo_active: bool,
    /// Cached mapping from server ID → account ID that owns it.
    server_account_map: HashMap<String, String>,
    /// Authenticated sessions keyed by account ID.
    ///
    /// Stored so the UI can retrieve per-account identity info (e.g. `icon_emoji`)
    /// without going through the async backend trait.
    pub sessions: HashMap<String, Session>,
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
            sessions: HashMap::new(),
        }
    }

    /// Activate the demo client.
    ///
    /// Creates two `DemoClient` instances — one for the "cat" demo account (`demo`)
    /// and one for the "dog" demo account (`demo2`). Both are authenticated and added
    /// to the backends map, giving the UI a realistic multi-account scenario.
    /// Returns the session for the first (cat) account so callers can record the
    /// local user.
    #[cfg(feature = "demo")]
    pub async fn activate_demo(&mut self) -> Result<Session, String> {
        if self.demo_active {
            // Return a fresh demo session even if already active
            return Ok(poly_demo::data::demo_session());
        }

        // Activate demo (cat) account
        let mut client = poly_demo::DemoClient::new();
        let session = client
            .authenticate(AuthCredentials::Token("demo-token".to_string()))
            .await
            .map_err(|e| format!("Demo auth failed: {e}"))?;
        self.sessions
            .insert("demo-cat".to_string(), session.clone());
        self.backends.insert(
            "demo-cat".to_string(),
            Arc::new(RwLock::new(Box::new(client))),
        );

        // Activate demo2 (dog) account
        let mut client2 = poly_demo::DemoClient2::new();
        let session2 = client2
            .authenticate(AuthCredentials::Token("demo2-token".to_string()))
            .await
            .map_err(|e| format!("Demo2 auth failed: {e}"))?;
        self.sessions.insert("demo-dog".to_string(), session2);
        self.backends.insert(
            "demo-dog".to_string(),
            Arc::new(RwLock::new(Box::new(client2))),
        );

        self.demo_active = true;

        // Rebuild server-account map
        self.rebuild_server_map().await;

        tracing::info!("Demo clients activated (demo + demo2)");
        Ok(session)
    }

    /// Deactivate the demo client.
    ///
    /// Removes both demo backends from the map and clears demo-related cache.
    #[cfg(feature = "demo")]
    pub fn deactivate_demo(&mut self) {
        self.backends.remove("demo-cat");
        self.backends.remove("demo-dog");
        self.sessions.remove("demo-cat");
        self.sessions.remove("demo-dog");
        self.demo_active = false;
        // Remove demo entries from server map
        self.server_account_map
            .retain(|_, account_id| account_id != "demo-cat" && account_id != "demo-dog");
        tracing::info!("Demo clients deactivated");
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

    /// Add a poly-server account.
    ///
    /// Creates a [`PolyServerBackend`], authenticates (signup or signin), and
    /// registers it in the backends map. Returns the session on success.
    ///
    /// # Arguments
    /// * `server_url` — Base URL of the poly-server instance
    /// * `private_key_bytes` — Raw 32-byte Ed25519 signing key
    /// * `username` — Username (for signup only)
    /// * `display_name` — Display name (for signup only)
    /// * `is_signup` — `true` for signup, `false` for signin
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn add_poly_server(
        &mut self,
        server_url: &str,
        private_key_bytes: [u8; 32],
        username: Option<&str>,
        display_name: Option<&str>,
        is_signup: bool,
    ) -> Result<Session, String> {
        use poly_server_client::PolyServerBackend;
        let mut backend = PolyServerBackend::new(server_url, private_key_bytes);

        let credentials = AuthCredentials::PolyServer {
            server_url: server_url.to_string(),
            private_key_bytes: private_key_bytes.to_vec(),
            username: username.map(|s| s.to_string()),
            display_name: display_name.map(|s| s.to_string()),
            is_signup,
        };

        let session = backend
            .authenticate(credentials)
            .await
            .map_err(|e| format!("Poly server auth failed: {e}"))?;

        let account_id = session.id.clone();
        self.sessions.insert(account_id.clone(), session.clone());
        self.backends
            .insert(account_id, Arc::new(RwLock::new(Box::new(backend))));

        // Rebuild server map to include the new account's servers.
        self.rebuild_server_map().await;

        tracing::info!(
            "Poly server account added: {} ({})",
            session.user.display_name,
            server_url
        );
        Ok(session)
    }

    /// Remove a poly-server account by account ID.
    pub async fn remove_poly_server(&mut self, account_id: &str) -> Result<(), String> {
        if let Some(backend) = self.backends.remove(account_id) {
            let mut guard = backend.write().await;
            if let Err(e) = guard.logout().await {
                tracing::warn!("Logout failed for poly-server {account_id}: {e}");
            }
        }
        self.sessions.remove(account_id);
        self.server_account_map.retain(|_, aid| aid != account_id);
        tracing::info!("Poly server account removed: {account_id}");
        Ok(())
    }
}
