//! Client manager — manages active messenger backend connections.
//!
//! `ClientManager` holds `Arc<dyn ClientBackend>` instances keyed by account ID.
//! It provides methods to activate/deactivate the demo client and to look up
//! which backend owns a given server.
//!
//! Provided as `Signal<ClientManager>` at the `App` level.
// TODO(phase-2.5.1): Client Manager Module

use dioxus::prelude::{Callback, Element};
use poly_client::{
    AccountPresence, AuthCredentials, BackendType, ClientBackend, ConnectionStatus, Server,
    Session, SignupCompleted, SignupContext, TestAccountEntry,
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

/// Describes a client backend that the user can sign up for.
///
/// Registered at startup by each compiled-in native plugin via
/// [`ClientManager::register_signup_entry`].  WASM plugins register
/// themselves at load time.  The signup picker reads this list at
/// runtime — the host has **no compile-time knowledge** of any specific
/// backend.  This mirrors the `PluginSettingsEntry` pattern exactly.
///
/// FTL keys (`name_key`, `desc_key`) are resolved in the **plugin's own**
/// Fluent bundle (e.g. `plugin-poly-signup-name`), NOT in core locale files.
#[derive(Clone, Copy, Debug)]
pub struct SignupEntry {
    /// URL slug used in `/signup/:client` routing (e.g. `"poly"`, `"stoat"`).
    pub slug: &'static str,
    /// Emoji displayed in the backend picker card.
    pub icon: &'static str,
    /// FTL key for the backend display name (resolved from plugin's bundle).
    pub name_key: &'static str,
    /// FTL key for the one-line backend description (resolved from plugin's bundle).
    pub desc_key: &'static str,
    /// Function that renders the full-page signup UI for this backend.
    ///
    /// The host calls this with:
    /// - `on_complete` — a callback the plugin calls when auth succeeds.
    ///   The host commits the session to `ClientManager` + `ChatData` and navigates.
    /// - `ctx` — [`SignupContext`] with the private key (for Ed25519-based auth)
    ///   and the host's i18n lookup function.
    ///
    /// Must be a static function (not a closure) so the entry is `Copy`.
    pub render: fn(Callback<SignupCompleted>, SignupContext) -> Element,
}

/// A shared, thread-safe handle to a messenger backend.
pub type BackendHandle = Arc<RwLock<Box<dyn ClientBackend>>>;

// Hang #4 prevention helper — see `crate::client_manager_timeout` module
// docs and `docs/plans/plan-backend-read-timeout.md`. Re-exported here so
// consumers can `use poly_core::client_manager::BackendHandleExt;`.
pub use crate::client_manager_timeout::{BackendHandleExt, BackendReadTimeout};

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
    /// Native backends currently disabled by the user in Settings → Plugins.
    pub disabled_native_backends: Vec<String>,
    /// Signup entries registered by compiled-in or WASM plugins at startup.
    ///
    /// The signup picker (`/signup` route) reads this list at runtime to
    /// show the available backends.  The host has no compile-time knowledge
    /// of any specific backend — each plugin registers itself via
    /// [`register_signup_entry`].
    pub signup_entries: Vec<SignupEntry>,
    /// Test accounts registered by native plugins for the quick-add dev panel.
    pub test_account_entries: Vec<TestAccountEntry>,
}

impl Clone for ClientManager {
    fn clone(&self) -> Self {
        Self {
            backends: self.backends.clone(),
            demo_active: self.demo_active,
            server_account_map: self.server_account_map.clone(),
            sessions: self.sessions.clone(),
            connection_statuses: self.connection_statuses.clone(),
            presence_statuses: self.presence_statuses.clone(),
            plugin_settings: self.plugin_settings.clone(),
            disabled_native_backends: self.disabled_native_backends.clone(),
            signup_entries: self.signup_entries.clone(),
            test_account_entries: self.test_account_entries.clone(),
        }
    }
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
            disabled_native_backends: Vec::new(),
            signup_entries: Vec::new(),
            test_account_entries: Vec::new(),
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
    /// Removes all demo backends from the map and clears demo-related cache.
    #[cfg(feature = "demo")]
    pub fn deactivate_demo(&mut self) {
        for id in &["demo-cat", "demo-dog", "demo-platypus"] {
            self.backends.remove(*id);
            self.sessions.remove(*id);
            self.connection_statuses.remove(*id);
            self.presence_statuses.remove(*id);
        }
        self.demo_active = false;
        // Remove demo entries from server map
        self.server_account_map.retain(|_, account_id| {
            account_id != "demo-cat"
                && account_id != "demo-dog"
                && account_id != "demo-platypus"
        });
        tracing::info!("Demo clients deactivated");
    }

    /// Return the account IDs of all currently active demo accounts.
    ///
    /// Determined by inspecting the live `sessions` map for entries whose
    /// `backend` field is [`poly_client::BackendType::from("demo")`]. This keeps the
    /// UI layer free from any knowledge of hard-coded demo account IDs.
    #[cfg(feature = "demo")]
    pub fn demo_account_ids(&self) -> Vec<String> {
        self.sessions
            .iter()
            .filter(|(_, s)| {
                let slug = s.backend.as_str();
                slug == "demo" || slug == "demo_forum"
            })
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

    /// Get the list of accounts that should appear in the sidebar.
    ///
    /// Includes both **live** accounts (with a connected backend) and **offline**
    /// accounts that only have a cached session — the latter covers:
    ///   1. Accounts restored from storage while the server is unreachable.
    ///   2. Accounts whose stored token was rejected with 401 (Unauthenticated).
    ///
    /// Both cases need to stay visible so the user can click through to
    /// reauthenticate. Live operations (send message, sync) still iterate
    /// `backends` directly and skip offline entries naturally.
    pub fn active_account_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.backends.keys().cloned().collect();
        for id in self.sessions.keys() {
            if !ids.iter().any(|x| x == id) {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// Register an offline/cached session for an account that has no live backend yet.
    ///
    /// Called during storage init to restore accounts from persisted tokens so Bar 1
    /// can show account icons even when offline. Sets `ConnectionStatus::Disconnected`.
    pub fn register_offline_session(&mut self, account_id: String, session: Session) {
        self.sessions.insert(account_id.clone(), session);
        self.connection_statuses
            .entry(account_id.clone())
            .or_insert(ConnectionStatus::Disconnected);
        self.presence_statuses
            .entry(account_id)
            .or_insert(AccountPresence::Online);
    }

    /// Mark an account as needing reauthentication (stored token rejected with 401).
    ///
    /// Sets the connection status to `Unauthenticated` — the account icon shows a
    /// 🔑 badge regardless of whether the backend uses forum layout. The caller
    /// should also push a `NotificationKind::ReauthRequired` notification into
    /// `ChatData` so the user sees an actionable toast.
    pub fn mark_unauthenticated(&mut self, account_id: &str, reason: impl Into<String>) {
        self.connection_statuses
            .insert(account_id.to_string(), ConnectionStatus::Unauthenticated(reason.into()));
    }

    /// Register a server → account mapping so `get_backend_for_server` can route API
    /// calls for a newly-created server without a full server-map rebuild.
    pub fn register_server(&mut self, server_id: String, account_id: String) {
        self.server_account_map.insert(server_id, account_id);
    }

    /// Remove all backends belonging to a given [`BackendType`], returning the removed
    /// account IDs and their handles so the caller can run async cleanup (logout, token
    /// removal) **after** releasing the `Signal` write lock.
    ///
    /// Also clears all server-map entries and status entries owned by those accounts.
    pub fn take_accounts_by_backend(
        &mut self,
        backend_type: BackendType,
    ) -> (Vec<String>, Vec<BackendHandle>) {
        // Collect the IDs that belong to this backend type
        // by inspecting the sessions map (BackendType is stored there).
        let removed_ids: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.backend == backend_type)
            .map(|(id, _)| id.clone())
            .collect();

        let mut handles = Vec::new();
        for id in &removed_ids {
            if let Some(handle) = self.backends.remove(id) {
                handles.push(handle);
            }
            self.sessions.remove(id.as_str());
            self.connection_statuses.remove(id.as_str());
            self.presence_statuses.remove(id.as_str());
        }
        // Remove stale server → account entries.
        self.server_account_map
            .retain(|_, aid| !removed_ids.contains(aid));

        tracing::info!(
            "take_accounts_by_backend({:?}): removed {} account(s)",
            backend_type,
            removed_ids.len()
        );
        (removed_ids, handles)
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

    /// Register a backend's signup entry in the /signup picker.
    ///
    /// Called at app startup by each compiled-in native plugin (and at load
    /// time for WASM plugins). Idempotent — re-registering the same slug
    /// replaces the existing entry.
    pub fn register_signup_entry(&mut self, entry: SignupEntry) {
        self.signup_entries.retain(|e| e.slug != entry.slug);
        self.signup_entries.push(entry);
        tracing::debug!("Signup entry registered: {}", entry.slug);
    }

    /// Register a test account entry from a native plugin.
    ///
    /// Idempotent — re-registering the same `(base_url, username)` pair
    /// replaces the existing entry instead of duplicating it. The App's
    /// `use_effect` re-runs more than once during boot, so without dedupe
    /// this list grew to 28 entries on every restart (2× the unique 14).
    pub fn register_test_account(&mut self, entry: TestAccountEntry) {
        self.test_account_entries
            .retain(|e| e.base_url != entry.base_url || e.username != entry.username);
        self.test_account_entries.push(entry);
    }

    /// Replace the in-memory disabled native backend list.
    pub fn set_disabled_native_backends(&mut self, disabled: Vec<String>) {
        self.disabled_native_backends = disabled;
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
        self.commit_backend_account(account_id, session, backend, server_map);
    }

    /// Commit a pre-authenticated backend account synchronously.
    pub fn commit_backend_account(
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
        tracing::info!("Backend account committed to ClientManager");
    }

    /// Detach a single account from the runtime maps and return its backend
    /// handle so the caller can `logout()` outside the write-lock.
    ///
    /// All runtime state keyed on the account id is cleared (backends,
    /// sessions, connection/presence status, server→account mapping). The
    /// caller is responsible for removing the persisted storage row.
    pub fn take_account(&mut self, account_id: &str) -> Option<BackendHandle> {
        let handle = self.backends.remove(account_id);
        self.sessions.remove(account_id);
        self.connection_statuses.remove(account_id);
        self.presence_statuses.remove(account_id);
        self.server_account_map.retain(|_, aid| aid != account_id);
        handle
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
