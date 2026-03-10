//! Client manager — manages active messenger backend connections.
//!
//! `ClientManager` holds `Arc<dyn ClientBackend>` instances keyed by account ID.
//! It provides methods to activate/deactivate the demo client and to look up
//! which backend owns a given server.
//!
//! Provided as `Signal<ClientManager>` at the `App` level.
// TODO(phase-2.5.1): Client Manager Module

use dioxus::prelude::Element;
use poly_client::{
    AccountPresence, AuthCredentials, BackendType, ClientBackend, ConnectionStatus, Server, Session,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A settings page registered by an active client backend plugin at runtime.
///
/// Backends call [`ClientManager::register_plugin_settings`] when they activate
/// and [`ClientManager::unregister_plugin_settings`] when they deactivate.
/// The settings UI reads this list reactively — the host has no compile-time
/// knowledge of any specific plugin's settings.
///
/// All fields are `'static` so the struct is `Copy` and can be stored
/// cheaply in a `Vec` without heap allocation per entry.
#[derive(Clone, Copy, Debug)]
pub struct PluginSettingsEntry {
    /// URL-safe slug — used for scroll-anchor IDs
    /// (`settings-section-plugin-{slug}`) and future deep-link routing.
    pub slug: &'static str,
    /// i18n key resolved via `t(nav_label_key)` for the sidebar nav label.
    /// Convention: `"plugin-{id}-title"` (mirrors WIT `plugin-metadata`).
    pub nav_label_key: &'static str,
    /// Emoji icon displayed next to the nav label.
    pub nav_icon: &'static str,
    /// Plain `fn() -> Element` wrapper that renders this plugin's settings page.
    /// Must be a static function (not a closure) so the entry is `Copy`.
    pub render: fn() -> Element,
}

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
    /// Live connection state per account.
    ///
    /// Set to `Connecting` when a backend activates, updated to `Connected` or
    /// `Error` by the event-stream consumer. Demo accounts start `Connected`.
    pub connection_statuses: HashMap<String, ConnectionStatus>,
    /// User-chosen presence/availability status per account.
    ///
    /// Persisted to local storage so the preference survives restarts.
    /// Defaults to `Online` for new accounts.
    pub presence_statuses: HashMap<String, AccountPresence>,
    /// Settings pages registered by active plugin backends at runtime.
    ///
    /// Populated via [`register_plugin_settings`] when a backend activates
    /// and cleared via [`unregister_plugin_settings`] when it deactivates.
    /// The settings nav sidebar and content area iterate this list to render
    /// plugin settings — nothing is hardcoded in the host.
    pub plugin_settings: Vec<PluginSettingsEntry>,
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
            connection_statuses: HashMap::new(),
            presence_statuses: HashMap::new(),
            plugin_settings: Vec::new(),
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

        // Demo backends are always "connected" — they are in-process and never fail.
        self.connection_statuses
            .insert("demo-cat".to_string(), ConnectionStatus::Connected);
        self.connection_statuses
            .insert("demo-dog".to_string(), ConnectionStatus::Connected);
        // Default presence: Online.
        self.presence_statuses
            .entry("demo-cat".to_string())
            .or_insert(AccountPresence::Online);
        self.presence_statuses
            .entry("demo-dog".to_string())
            .or_insert(AccountPresence::Online);

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
        self.connection_statuses.remove("demo-cat");
        self.connection_statuses.remove("demo-dog");
        self.presence_statuses.remove("demo-cat");
        self.presence_statuses.remove("demo-dog");
        self.demo_active = false;
        // Remove demo entries from server map
        self.server_account_map
            .retain(|_, account_id| account_id != "demo-cat" && account_id != "demo-dog");
        tracing::info!("Demo clients deactivated");
    }

    /// Return the account IDs of all currently active demo accounts.
    ///
    /// Determined by inspecting the live `sessions` map for entries whose
    /// `backend` field is [`poly_client::BackendType::Demo`]. This keeps the
    /// UI layer free from any knowledge of hard-coded demo account IDs.
    #[cfg(feature = "demo")]
    pub fn demo_account_ids(&self) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|(_, s)| s.backend == BackendType::Demo)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Commit pre-authenticated demo client bundles into the manager synchronously.
    ///
    /// This is the **second phase** of the two-phase demo activation used by
    /// [`crate::ui::demo::toggle_demo`]. The first phase authenticates all demo
    /// clients asynchronously **without** holding any Dioxus `Signal` lock; this
    /// method commits the results synchronously so no `SignalMut` is ever held
    /// across an `.await` boundary.
    ///
    /// # Signal / RefCell discipline — CRITICAL
    ///
    /// **Never** call `signal.write().activate_demo().await`. A `SignalMut` is a
    /// Dioxus `RefCell` write guard. Holding it across `.await` points causes
    /// `"RefCell already borrowed"` panics in WASM because the Dioxus runtime
    /// re-renders subscribed components during yield points, which attempt
    /// `signal.read()` borrows while the write borrow is still held.
    ///
    /// Always do async work first, then call this method inside a brief
    /// `signal.write()` block with **no** subsequent `.await`.
    #[cfg(feature = "demo")]
    pub fn commit_demo_activation(
        &mut self,
        entries: Vec<(String, Session, BackendHandle)>,
        server_map: HashMap<String, String>,
    ) {
        for (account_id, session, backend) in entries {
            self.sessions.insert(account_id.clone(), session);
            self.backends.insert(account_id.clone(), backend);
            self.connection_statuses
                .insert(account_id.clone(), ConnectionStatus::Connected);
            self.presence_statuses
                .entry(account_id)
                .or_insert(AccountPresence::Online);
        }
        self.server_account_map.extend(server_map);
        self.demo_active = true;
        tracing::info!("Demo clients committed to ClientManager");
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

    /// Register a settings page for an active plugin backend.
    ///
    /// If a page with the same slug is already registered the existing entry
    /// is replaced (idempotent — safe to call on every activation).
    pub fn register_plugin_settings(&mut self, entry: PluginSettingsEntry) {
        self.plugin_settings.retain(|e| e.slug != entry.slug);
        self.plugin_settings.push(entry);
        tracing::debug!("Plugin settings registered: {}", entry.slug);
    }

    /// Unregister a plugin's settings page.
    ///
    /// Call this when a plugin deactivates. Silently does nothing if the
    /// slug is not currently registered.
    pub fn unregister_plugin_settings(&mut self, slug: &str) {
        self.plugin_settings.retain(|e| e.slug != slug);
        tracing::debug!("Plugin settings unregistered: {slug}");
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

    /// Commit a pre-authenticated poly-server backend **synchronously**.
    ///
    /// This is the **second phase** of the two-phase poly-server activation
    /// used by the UI wizard. Phase 1 creates and authenticates the backend
    /// **without** any Dioxus `Signal` lock; this method commits the result
    /// inside a brief `.write()` block with **no** subsequent `.await`.
    ///
    /// See [`crate::ui::demo::toggle_demo`] for the pattern rationale.
    pub fn commit_poly_server(
        &mut self,
        account_id: String,
        session: Session,
        backend: BackendHandle,
        server_map: HashMap<String, String>,
    ) {
        self.sessions.insert(account_id.clone(), session);
        self.backends.insert(account_id.clone(), backend);
        self.connection_statuses
            .insert(account_id.clone(), ConnectionStatus::Connected);
        self.presence_statuses
            .entry(account_id)
            .or_insert(AccountPresence::Online);
        self.server_account_map.extend(server_map);
        tracing::info!("Poly server account committed to ClientManager");
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
