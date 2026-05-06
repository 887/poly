//! Plugin registry: loading, management, and lifecycle.
//!
//! The [`PluginRegistry`] is the main entry point for the WASM plugin system.
//! It loads plugin `.wasm` components, instantiates them with the host API,
//! and provides a [`PluginBackend`] wrapper that implements `ClientBackend`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let registry = PluginRegistry::new()?;
//! registry.load_from_file("demo", &Path::new("plugins/poly_demo.wasm"))?;
//! let backend = registry.instantiate("demo").await?;
//! let session = backend.authenticate(credentials).await?;
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use async_trait::async_trait;
use futures::stream::Stream;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};

use poly_client::{
    ActionOutcome, AuthCredentials, BackendType, Channel, ClientBackend, ClientError, ClientEvent,
    ClientResult, ComposerButton, Cursor, DmChannel, ForumBackend, ForumPost,
    ForumSortOrder, Group, MenuItem, MenuTargetKind, Message, MessageContent, MessageQuery,
    Notification, PendingHandle, PresenceStatus, Server,
    Session, SettingsScope, SettingsSection, SidebarDeclaration, ThreadInfo,
    ThreadsBackend, User, ViewDescriptor, ViewDetail, ViewRowsPage, VoiceParticipant,
};

use super::bridge;
use super::engine::{self, MessengerPlugin};
use super::host_impl::PluginHostState;
use super::storage::{InMemoryPluginStorage, PluginStorageBackend};

/// Locales the host supports — must match poly-core's `SUPPORTED_LOCALES`.
/// Stored here to avoid a circular dependency on poly-core.
const SUPPORTED_LOCALES: &[&str] = &["en", "de", "fr", "es"];

/// Registry that manages all loaded WASM plugins.
///
/// Holds the shared [`Engine`] and provides methods to load plugin
/// components from bytes or files. Each loaded plugin gets its own
/// [`Store`] with isolated state.
pub struct PluginRegistry {
    /// Shared wasmtime engine (expensive to create, cheap to clone).
    engine: Engine,
    /// Pre-configured linker with host imports registered.
    linker: Linker<PluginHostState>,
    /// Loaded plugin components keyed by plugin ID.
    components: HashMap<String, Component>,
    /// Default storage backend injected into every new plugin instance.
    default_storage: Arc<dyn PluginStorageBackend>,
}

impl PluginRegistry {
    /// Create a new plugin registry with a configured engine and linker.
    pub fn new() -> Result<Self, String> {
        let engine =
            engine::create_engine().map_err(|e| format!("Failed to create WASM engine: {e}"))?;

        let mut linker = Linker::new(&engine);

        // Register WASI imports (required for wasm32-wasip2 targets)
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)
            .map_err(|e| format!("Failed to add WASI to linker: {e}"))?;

        // Register our host-api imports
        MessengerPlugin::add_to_linker::<_, HasSelf<_>>(
            &mut linker,
            |state: &mut PluginHostState| state,
        )
        .map_err(|e| format!("Failed to add host API to linker: {e}"))?;

        Ok(Self {
            engine,
            linker,
            components: HashMap::new(),
            default_storage: Arc::new(InMemoryPluginStorage::default()),
        })
    }

    /// Override the default storage backend for all plugins instantiated by this registry.
    ///
    /// Call this before any [`Self::instantiate`] calls. Returns `self` for chaining.
    #[must_use]
    pub fn with_default_storage(mut self, storage: Arc<dyn PluginStorageBackend>) -> Self {
        self.default_storage = storage;
        self
    }

    /// Load a plugin component from raw WASM bytes.
    ///
    /// The `plugin_id` is used for logging, storage namespacing, and
    /// component deduplication.
    pub fn load_from_bytes(&mut self, plugin_id: &str, bytes: &[u8]) -> Result<(), String> {
        let component = engine::load_component(&self.engine, bytes)
            .map_err(|e| format!("Failed to load component '{plugin_id}': {e}"))?;
        self.components.insert(plugin_id.to_string(), component);
        tracing::info!("Loaded WASM plugin: {plugin_id}");
        Ok(())
    }

    /// Load a plugin component from a `.wasm` file on disk.
    pub fn load_from_file(&mut self, plugin_id: &str, path: &Path) -> Result<(), String> {
        let component = engine::load_component_from_file(&self.engine, path).map_err(|e| {
            format!(
                "Failed to load component '{plugin_id}' from {}: {e}",
                path.display()
            )
        })?;
        self.components.insert(plugin_id.to_string(), component);
        tracing::info!(
            "Loaded WASM plugin from file: {plugin_id} ({})",
            path.display()
        );
        Ok(())
    }

    /// Instantiate a loaded plugin and return a [`PluginBackend`] wrapper.
    ///
    /// The wrapper implements `ClientBackend` so it can be used in the
    /// existing `ClientManager` infrastructure.
    pub async fn instantiate(&self, plugin_id: &str) -> Result<PluginBackend, String> {
        self.instantiate_with_host_state(
            plugin_id,
            PluginHostState::new_with_storage(plugin_id, self.default_storage.clone()),
        )
        .await
    }

    /// Instantiate a loaded plugin using a caller-provided host state.
    ///
    /// This is primarily used by plugin-host tests so they can inject mocked
    /// host I/O while still exercising the real WASM guest code path.
    pub async fn instantiate_with_host_state(
        &self,
        plugin_id: &str,
        host_state: PluginHostState,
    ) -> Result<PluginBackend, String> {
        let component = self
            .components
            .get(plugin_id)
            .ok_or_else(|| format!("Plugin '{plugin_id}' not loaded"))?;

        let mut store = Store::new(&self.engine, host_state);

        // Give the plugin some fuel to work with
        store
            .set_fuel(1_000_000_000)
            .map_err(|e| format!("Failed to set fuel: {e}"))?;

        let instance = MessengerPlugin::instantiate_async(&mut store, component, &self.linker)
            .await
            .map_err(|e| format!("Failed to instantiate plugin '{plugin_id}': {e}"))?;

        // Query the plugin for its self-reported backend type and name.
        // This avoids hardcoded string matching on plugin IDs.
        let wit_backend_type = instance
            .poly_messenger_messenger_client()
            .call_get_backend_type(&mut store)
            .await
            .map_err(|e| format!("Failed to get backend_type from '{plugin_id}': {e}"))?;
        // D17 — WIT backend-type is now plain `string` (slug).
        let cached_backend_type = BackendType::from_slug(&wit_backend_type);

        // Refuel before the next call
        drop(store.set_fuel(1_000_000_000));

        let cached_backend_name = instance
            .poly_messenger_messenger_client()
            .call_get_backend_name(&mut store)
            .await
            .map_err(|e| format!("Failed to get backend_name from '{plugin_id}': {e}"))?;

        // Load plugin translations and settings schema via the plugin-metadata interface.
        // FTL strings are stored in PluginBackend.plugin_ftl; the host (poly-core) reads
        // them after instantiation and calls i18n::register_plugin_ftl().
        drop(store.set_fuel(1_000_000_000));
        let meta = instance.poly_messenger_plugin_metadata();
        let mut plugin_ftl: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for locale in SUPPORTED_LOCALES {
            drop(store.set_fuel(1_000_000_000));
            match meta.call_get_translations(&mut store, locale).await {
                Ok(ftl_src) if !ftl_src.trim().is_empty() => {
                    plugin_ftl.insert(locale.to_string(), ftl_src);
                    tracing::debug!("Loaded '{plugin_id}' FTL for locale '{locale}'");
                }
                Ok(_) => {
                    tracing::debug!("Plugin '{plugin_id}' has no FTL for locale '{locale}'");
                }
                Err(e) => {
                    tracing::warn!("Plugin '{plugin_id}' get-translations({locale}) failed: {e}");
                }
            }
        }

        // Load settings schema.
        //
        // D18 — `plugin-metadata.get-settings-schema` has been removed.
        // The equivalent lives in `client-settings.get-settings-sections`,
        // which will be surfaced through `ClientBackend` in WP 1.C. For
        // now, start with an empty schema so the host compiles. Plugin
        // settings UI will be re-wired as part of that work package.
        //
        // TODO(WP 1.C): call client-settings::get-settings-sections and
        // pick the `scope == account-global` section to build this list.
        drop(store.set_fuel(1_000_000_000));
        let schema: Vec<SettingDescriptor> = Vec::new();

        drop(store.set_fuel(1_000_000_000));
        let display_name_key = match meta.call_get_display_name_key(&mut store).await {
            Ok(k) => k,
            Err(_) => format!("plugin-{plugin_id}-title"),
        };

        drop(store.set_fuel(1_000_000_000));
        let icon = meta.call_get_icon(&mut store).await.unwrap_or_default();

        tracing::info!(
            "Plugin '{plugin_id}' reports: type={:?}, name={cached_backend_name}, \
             icon={icon}, settings={} fields",
            cached_backend_type,
            schema.len(),
        );

        Ok(PluginBackend {
            plugin_id: plugin_id.to_string(),
            cached_backend_type,
            cached_backend_name,
            display_name_key,
            icon,
            plugin_ftl,
            schema,
            store: Arc::new(Mutex::new(store)),
            instance: Arc::new(instance),
        })
    }

    /// Get the IDs of all loaded plugins.
    #[must_use]
    pub fn loaded_plugins(&self) -> Vec<String> {
        self.components.keys().cloned().collect()
    }

    /// Check if a plugin is loaded.
    #[must_use]
    pub fn is_loaded(&self, plugin_id: &str) -> bool {
        self.components.contains_key(plugin_id)
    }
}

/// Kind of UI control for a plugin setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingKind {
    /// On/off toggle / checkbox.
    Toggle,
    /// Single-line text input.
    TextInput,
    /// Dropdown with a fixed list of options.
    Select,
    /// Numeric slider (min/max/step encoded in `extra`).
    Slider,
    /// Read-only informational label row.
    InfoLabel,
}

/// Descriptor for a single plugin-provided setting field.
///
/// Returned from the plugin's `get-settings-schema` WIT export.
/// The host uses this to render the appropriate UI control.
#[derive(Debug, Clone)]
pub struct SettingDescriptor {
    /// Unique key for this setting (used as storage key).
    pub key: String,
    /// UI control kind.
    pub kind: SettingKind,
    /// Default value as a string (parsed by the host for each kind).
    pub default_value: String,
    /// Kind-specific extra data (e.g. comma-separated options for Select).
    pub extra: String,
}

/// A WASM plugin wrapped as a [`ClientBackend`].
///
/// This bridges the WIT Component Model interface back to the poly-client
/// trait, allowing plugins to be used interchangeably with native backends.
///
/// Each method call:
/// 1. Converts poly-client args → WIT types (via bridge)
/// 2. Calls the guest export through wasmtime
/// 3. Converts WIT result → poly-client types (via bridge)
pub struct PluginBackend {
    /// Plugin identifier.
    plugin_id: String,
    /// Cached backend type from the plugin's `get_backend_type()` export.
    cached_backend_type: BackendType,
    /// Cached backend name from the plugin's `get_backend_name()` export.
    cached_backend_name: String,
    /// FTL key for the plugin's display name (from `get-display-name-key`).
    pub display_name_key: String,
    /// Icon emoji or short string (from `get-icon`).
    pub icon: String,
    /// Plugin-owned FTL strings, keyed by locale code (e.g. "en", "de").
    ///
    /// The host (poly-core) reads this after instantiation and merges the FTL
    /// into the live i18n bundles via `i18n::register_plugin_ftl()`.
    pub plugin_ftl: std::collections::HashMap<String, String>,
    /// Settings schema (from `get-settings-schema`).
    pub schema: Vec<SettingDescriptor>,
    /// Wasmtime store holding the plugin's host state.
    /// Uses Mutex (not RwLock) because wasmtime::Store is Send but not Sync.
    store: Arc<Mutex<Store<PluginHostState>>>,
    /// The instantiated plugin component.
    instance: Arc<MessengerPlugin>,
}

// Manual Debug since Store/MessengerPlugin don't implement Debug.
impl std::fmt::Debug for PluginBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginBackend")
            .field("plugin_id", &self.plugin_id)
            .finish()
    }
}

/// Helper to refuel the store before each guest call.
async fn refuel(store: &Arc<Mutex<Store<PluginHostState>>>) {
    let mut guard = store.lock().await;
    // Ignore fuel errors — fuel is best-effort
    drop(guard.set_fuel(1_000_000_000));
}

/// Convert a WIT result with no conversion needed on the value.
fn convert_result_unit(
    wit_result: Result<
        Result<(), super::engine::poly::messenger::types::ClientError>,
        wasmtime::Error,
    >,
) -> ClientResult<()> {
    match wit_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
        Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
    }
}

#[async_trait]
impl ClientBackend for PluginBackend {
    async fn authenticate(&mut self, credentials: AuthCredentials) -> ClientResult<Session> {
        refuel(&self.store).await;
        let wit_creds = bridge::to_wit_auth_credentials(credentials);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_authenticate(&mut *store, &wit_creds)
            .await;
        match result {
            Ok(Ok(session)) => Ok(bridge::from_wit_session(session)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn logout(&mut self) -> ClientResult<()> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_logout(&mut *store)
            .await;
        convert_result_unit(result)
    }

    fn is_authenticated(&self) -> bool {
        // We need a sync check but the plugin is async — use a blocking approach.
        // In practice, the host should cache this state. For now, default to false.
        // TODO(phase-2.14.3): Implement proper sync auth check with cached state
        false
    }

    async fn get_servers(&self) -> ClientResult<Vec<Server>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_servers(&mut *store)
            .await;
        match result {
            Ok(Ok(servers)) => Ok(servers.into_iter().map(bridge::from_wit_server).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_server(&self, id: &str) -> ClientResult<Server> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_server(&mut *store, id)
            .await;
        match result {
            Ok(Ok(server)) => Ok(bridge::from_wit_server(server)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_channels(&self, server_id: &str) -> ClientResult<Vec<Channel>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_channels(&mut *store, server_id)
            .await;
        match result {
            Ok(Ok(channels)) => Ok(channels.into_iter().map(bridge::from_wit_channel).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_channel(&self, id: &str) -> ClientResult<Channel> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_channel(&mut *store, id)
            .await;
        match result {
            Ok(Ok(channel)) => Ok(bridge::from_wit_channel(channel)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        refuel(&self.store).await;
        let wit_content = bridge::to_wit_message_content(content);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_send_message(&mut *store, channel_id, &wit_content)
            .await;
        match result {
            Ok(Ok(msg)) => Ok(bridge::from_wit_message(msg)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_messages(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        refuel(&self.store).await;
        let wit_query = bridge::to_wit_message_query(query);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_messages(&mut *store, channel_id, &wit_query)
            .await;
        match result {
            Ok(Ok(msgs)) => Ok(msgs.into_iter().map(bridge::from_wit_message).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    // ── Messaging extras (H.4.a — moved to MessagingBackend) ────────────────

    fn as_messaging(&self) -> Option<&dyn poly_client::MessagingBackend> {
        Some(self)
    }

    // ── Server admin (H.4.b — moved to ServerAdminBackend) ──────────────────
    // WIT does not expose server management → NotSupported stubs

    fn as_server_admin(&self) -> Option<&dyn poly_client::ServerAdminBackend> {
        None
    }

    // ── Social graph (H.3.b — moved to SocialGraphBackend) ──────────────────

    fn as_social_graph(&self) -> Option<&dyn poly_client::SocialGraphBackend> {
        Some(self)
    }

    async fn get_channel_members(&self, channel_id: &str) -> ClientResult<Vec<User>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_channel_members(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(users)) => Ok(users.into_iter().map(bridge::from_wit_user).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    // ── DMs and groups (H.3.c — moved to DmsAndGroupsBackend) ──────────────

    fn as_dms_and_groups(&self) -> Option<&dyn poly_client::DmsAndGroupsBackend> {
        Some(self)
    }

    async fn get_notifications(&self) -> ClientResult<Vec<Notification>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_notifications(&mut *store)
            .await;
        match result {
            Ok(Ok(notifs)) => Ok(notifs
                .into_iter()
                .map(bridge::from_wit_notification)
                .collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_voice_participants(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<VoiceParticipant>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_voice_participants(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(participants)) => Ok(participants
                .into_iter()
                .map(bridge::from_wit_voice_participant)
                .collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    fn event_stream(&self) -> std::pin::Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // Push-based event delivery:
        //
        // 1. Set up event channel — guest calls emit-event → host forwards here
        // 2. Take the WS inbound receiver — WS read tasks send data here
        // 3. Spawn a loop that forwards WS data to guest via handle-ws-data,
        //    which triggers the guest to call emit-event with parsed events
        //
        // No polling. Events flow push-to-push.
        let store = self.store.clone();
        let instance = self.instance.clone();

        let (event_tx, event_rx) = tokio::sync::mpsc::channel::<ClientEvent>(64);

        // Install the event sender in the host state so emit-event can use it.
        // Also take ownership of the WS inbound receiver.
        // Use try_lock — the store is never contended at event_stream() call time,
        // and blocking_lock() would panic when called from within a tokio runtime.
        let ws_inbound_rx = if let Ok(mut guard) = store.try_lock() {
            guard.data_mut().event_tx = Some(event_tx);
            guard.data_mut().ws_inbound_rx.take()
        } else {
            tracing::warn!("event_stream: store was contended, event delivery may be delayed");
            None
        };

        // Spawn the WS data forwarding loop
        if let Some(mut ws_rx) = ws_inbound_rx {
            tokio::spawn(async move {
                while let Some(ws_data) = ws_rx.recv().await {
                    // Refuel before calling into guest
                    {
                        let mut guard = store.lock().await;
                        drop(guard.set_fuel(1_000_000_000));
                    }

                    // Forward WS data to guest — guest parses it and calls emit-event
                    let result = {
                        let mut guard = store.lock().await;
                        instance
                            .poly_messenger_messenger_client()
                            .call_handle_ws_data(
                                &mut *guard,
                                ws_data.handle,
                                &ws_data.data,
                            )
                            .await
                    };

                    if let Err(e) = result {
                        tracing::error!("Plugin handle_ws_data error: {e}");
                    }
                }
            });
        }

        Box::pin(tokio_stream::wrappers::ReceiverStream::new(event_rx))
    }

    fn backend_type(&self) -> BackendType {
        self.cached_backend_type.clone()
    }

    fn backend_name(&self) -> &str {
        &self.cached_backend_name
    }

    // ── Client-provided UI surface (WP 1.C) ────────────────────────

    async fn get_context_menu_items(
        &self,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        refuel(&self.store).await;
        let wit_target = bridge::to_wit_menu_target_kind(target);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_menus()
            .call_get_context_menu_items(&mut *store, wit_target, target_id)
            .await;
        match result {
            Ok(Ok(items)) => Ok(items.into_iter().map(bridge::from_wit_menu_item).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn invoke_context_action(
        &self,
        action_id: &str,
        target: MenuTargetKind,
        target_id: &str,
    ) -> ClientResult<ActionOutcome> {
        refuel(&self.store).await;
        let wit_target = bridge::to_wit_menu_target_kind(target);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_menus()
            .call_invoke_context_action(&mut *store, action_id, wit_target, target_id)
            .await;
        match result {
            Ok(Ok(outcome)) => Ok(bridge::from_wit_action_outcome(outcome)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn poll_action(&self, handle: PendingHandle) -> ClientResult<ActionOutcome> {
        refuel(&self.store).await;
        let wit_handle = bridge::to_wit_pending_handle(handle);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_menus()
            .call_poll_action(&mut *store, &wit_handle)
            .await;
        match result {
            Ok(Ok(outcome)) => Ok(bridge::from_wit_action_outcome(outcome)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_settings_sections(&self) -> ClientResult<Vec<SettingsSection>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_settings()
            .call_get_settings_sections(&mut *store)
            .await;
        match result {
            Ok(Ok(sections)) => Ok(sections
                .into_iter()
                .map(bridge::from_wit_settings_section)
                .collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
    ) -> ClientResult<String> {
        refuel(&self.store).await;
        let wit_scope = bridge::to_wit_settings_scope(scope);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_settings()
            .call_get_setting_value(&mut *store, wit_scope, scope_id, key)
            .await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn set_setting_value(
        &self,
        scope: SettingsScope,
        scope_id: &str,
        key: &str,
        value: &str,
    ) -> ClientResult<()> {
        refuel(&self.store).await;
        let wit_scope = bridge::to_wit_settings_scope(scope);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_settings()
            .call_set_setting_value(&mut *store, wit_scope, scope_id, key, value)
            .await;
        convert_result_unit(result)
    }

    async fn get_sidebar_declaration(&self) -> ClientResult<SidebarDeclaration> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_sidebar()
            .call_get_sidebar_declaration(&mut *store)
            .await;
        match result {
            Ok(Ok(decl)) => Ok(bridge::from_wit_sidebar_declaration(decl)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn invoke_sidebar_action(&self, action_id: &str) -> ClientResult<ActionOutcome> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_sidebar()
            .call_invoke_sidebar_action(&mut *store, action_id)
            .await;
        match result {
            Ok(Ok(outcome)) => Ok(bridge::from_wit_sidebar_action_outcome(outcome)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_channel_view(&self, channel_id: &str) -> ClientResult<ViewDescriptor> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_views()
            .call_get_channel_view(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(d)) => Ok(bridge::from_wit_view_descriptor(d)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_view_rows(
        &self,
        channel_id: &str,
        cursor: Option<Cursor>,
        sort_id: Option<&str>,
        filter_id: Option<&str>,
        tab_id: Option<&str>,
    ) -> ClientResult<ViewRowsPage> {
        refuel(&self.store).await;
        let wit_cursor = cursor.map(bridge::to_wit_view_cursor);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_views()
            .call_get_view_rows(
                &mut *store,
                channel_id,
                wit_cursor.as_ref(),
                sort_id,
                filter_id,
                tab_id,
            )
            .await;
        match result {
            Ok(Ok(page)) => Ok(bridge::from_wit_view_rows_page(page)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_view_detail(
        &self,
        channel_id: &str,
        row_id: &str,
    ) -> ClientResult<ViewDetail> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_views()
            .call_get_view_detail(&mut *store, channel_id, row_id)
            .await;
        match result {
            Ok(Ok(d)) => Ok(bridge::from_wit_view_detail(d)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_composer_buttons(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<ComposerButton>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_composer()
            .call_get_composer_buttons(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(buttons)) => Ok(buttons
                .into_iter()
                .map(bridge::from_wit_composer_button)
                .collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_message_actions(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<Vec<MenuItem>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_composer()
            .call_get_message_actions(&mut *store, channel_id, message_id)
            .await;
        match result {
            Ok(Ok(items)) => Ok(items
                .into_iter()
                .map(bridge::from_wit_composer_menu_item)
                .collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn invoke_composer_action(
        &self,
        action_id: &str,
        channel_id: &str,
    ) -> ClientResult<ActionOutcome> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_composer()
            .call_invoke_composer_action(&mut *store, action_id, channel_id)
            .await;
        match result {
            Ok(Ok(outcome)) => Ok(bridge::from_wit_composer_action_outcome(outcome)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn invoke_message_action(
        &self,
        action_id: &str,
        channel_id: &str,
        message_id: &str,
    ) -> ClientResult<ActionOutcome> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_client_composer()
            .call_invoke_message_action(&mut *store, action_id, channel_id, message_id)
            .await;
        match result {
            Ok(Ok(outcome)) => Ok(bridge::from_wit_composer_action_outcome(outcome)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    // --- Forum channels (H.2.b — moved to ForumBackend) ---

    fn as_forum(&self) -> Option<&dyn ForumBackend> {
        Some(self)
    }

    // --- Thread channels (H.2.c — moved to ThreadsBackend) ---

    fn as_threads(&self) -> Option<&dyn ThreadsBackend> {
        Some(self)
    }

}

// ── H.2.b — ForumBackend ─────────────────────────────────────────────────────

#[async_trait]
impl ForumBackend for PluginBackend {
    async fn get_forum_posts(
        &self,
        forum_channel_id: &str,
        sort: ForumSortOrder,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>> {
        refuel(&self.store).await;
        let wit_sort = bridge::to_wit_forum_sort_order(sort);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_forum_posts(&mut *store, forum_channel_id, wit_sort, limit)
            .await;
        match result {
            Ok(Ok(posts)) => Ok(posts.into_iter().map(bridge::from_wit_forum_post).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn create_forum_post(
        &self,
        forum_channel_id: &str,
        title: &str,
        body: &str,
        tags: Vec<String>,
    ) -> ClientResult<ForumPost> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_create_forum_post(&mut *store, forum_channel_id, title, body, &tags)
            .await;
        match result {
            Ok(Ok(post)) => Ok(bridge::from_wit_forum_post(post)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_recent_comments(
        &self,
        _channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        // WIT bridge does not expose get-recent-comments; only Lemmy implements this
        // natively.  WASM plugins that want this capability must do it via
        // get-view-rows or a custom sidebar action.
        Err(ClientError::NotSupported("get_recent_comments".to_string()))
    }
}

// ── H.2.c — ThreadsBackend ───────────────────────────────────────────────────

#[async_trait]
impl ThreadsBackend for PluginBackend {
    async fn get_active_threads(&self, server_id: &str) -> ClientResult<Vec<ThreadInfo>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_active_threads(&mut *store, server_id)
            .await;
        match result {
            Ok(Ok(threads)) => Ok(threads.into_iter().map(bridge::from_wit_thread_info).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_archived_threads(
        &self,
        parent_channel_id: &str,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ThreadInfo>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_archived_threads(&mut *store, parent_channel_id, limit)
            .await;
        match result {
            Ok(Ok(threads)) => Ok(threads.into_iter().map(bridge::from_wit_thread_info).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }
}

// ── H.3.b — SocialGraphBackend ────────────────────────────────────────────────
//
// The WIT `messenger-client` interface exposes `get-user`, `get-friends`,
// `get-presence`, and `set-presence`. The remaining social-graph methods
// (block/ignore/friend management) are not in the WIT interface yet — they
// return `NotSupported` until the WIT surface is extended.

#[async_trait]
impl poly_client::SocialGraphBackend for PluginBackend {
    async fn get_user(&self, id: &str) -> ClientResult<User> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_user(&mut *store, id)
            .await;
        match result {
            Ok(Ok(user)) => Ok(bridge::from_wit_user(user)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_friends(&self) -> ClientResult<Vec<User>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_friends(&mut *store)
            .await;
        match result {
            Ok(Ok(users)) => Ok(users.into_iter().map(bridge::from_wit_user).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn add_friend(&self, _user_id: &str) -> ClientResult<()> {
        // WIT does not expose add-friend yet.
        Err(ClientError::NotSupported("plugin: add_friend not in WIT interface".to_string()))
    }

    async fn remove_friend(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: remove_friend not in WIT interface".to_string()))
    }

    async fn respond_to_friend_request(&self, _user_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: respond_to_friend_request not in WIT interface".to_string()))
    }

    async fn set_friend_nickname(
        &self,
        _user_id: &str,
        _nickname: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: set_friend_nickname not in WIT interface".to_string()))
    }

    async fn set_user_note(&self, _user_id: &str, _note: Option<&str>) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: set_user_note not in WIT interface".to_string()))
    }

    async fn block_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: block_user not in WIT interface".to_string()))
    }

    async fn unblock_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: unblock_user not in WIT interface".to_string()))
    }

    async fn ignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: ignore_user not in WIT interface".to_string()))
    }

    async fn unignore_user(&self, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("plugin: unignore_user not in WIT interface".to_string()))
    }

    async fn get_presence(&self, user_id: &str) -> ClientResult<PresenceStatus> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_presence(&mut *store, user_id)
            .await;
        match result {
            Ok(Ok(status)) => Ok(bridge::from_wit_presence(status)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn set_presence(&self, status: PresenceStatus) -> ClientResult<()> {
        refuel(&self.store).await;
        let wit_status = bridge::to_wit_presence(status);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_set_presence(&mut *store, wit_status)
            .await;
        convert_result_unit(result)
    }
}

// DmsAndGroupsBackend: WIT exposes get_dm_channels, get_groups, open_direct_message_channel,
// open_saved_messages_channel, add_group_member, remove_group_member.
// Lifecycle methods (close, mute, leave, edit, add_users) not yet in WIT → NotSupported.

#[async_trait]
impl poly_client::MessagingBackend for PluginBackend {
    async fn send_typing(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "plugin: send_typing not in WIT interface".to_string(),
        ))
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: poly_client::MessageContent,
    ) -> ClientResult<poly_client::Message> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let wit_content = bridge::to_wit_message_content(content);
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_send_reply_message(&mut *store, channel_id, reply_to_message_id, &wit_content)
            .await;
        match result {
            Ok(Ok(msg)) => Ok(bridge::from_wit_message(msg)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn search_messages(
        &self,
        query: poly_client::MessageSearchQuery,
    ) -> ClientResult<Vec<poly_client::MessageSearchHit>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let wit_query = bridge::to_wit_message_search_query(query);
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_search_messages(&mut *store, &wit_query)
            .await;
        match result {
            Ok(Ok(hits)) => Ok(hits.into_iter().map(bridge::from_wit_message_search_hit).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<poly_client::Message>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_pinned_messages(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(msgs)) => Ok(msgs.into_iter().map(bridge::from_wit_message).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn set_message_pinned(
        &self,
        channel_id: &str,
        message_id: &str,
        pinned: bool,
    ) -> ClientResult<()> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_set_message_pinned(&mut *store, channel_id, message_id, pinned)
            .await;
        convert_result_unit(result)
    }

    async fn get_channel_commands(
        &self,
        _channel_id: &str,
    ) -> ClientResult<Vec<poly_client::ChatCommand>> {
        Err(ClientError::NotSupported(
            "plugin: get_channel_commands not in WIT interface".to_string(),
        ))
    }

    async fn get_available_emojis(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<poly_client::CustomEmoji>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_available_emojis(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(emojis)) => Ok(emojis.into_iter().map(bridge::from_wit_custom_emoji).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_available_stickers(
        &self,
        channel_id: &str,
    ) -> ClientResult<Vec<poly_client::StickerItem>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_available_stickers(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(stickers)) => Ok(stickers.into_iter().map(bridge::from_wit_sticker_item).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }
}

#[async_trait]
impl poly_client::DmsAndGroupsBackend for PluginBackend {
    async fn get_groups(&self) -> ClientResult<Vec<Group>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_groups(&mut *store)
            .await;
        match result {
            Ok(Ok(groups)) => Ok(groups.into_iter().map(bridge::from_wit_group).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_dm_channels(&self) -> ClientResult<Vec<DmChannel>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_dm_channels(&mut *store)
            .await;
        match result {
            Ok(Ok(dms)) => Ok(dms.into_iter().map(bridge::from_wit_dm_channel).collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<DmChannel> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_open_direct_message_channel(&mut *store, user_id)
            .await;
        match result {
            Ok(Ok(dm)) => Ok(bridge::from_wit_dm_channel(dm)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn open_saved_messages_channel(&self) -> ClientResult<DmChannel> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_open_saved_messages_channel(&mut *store)
            .await;
        match result {
            Ok(Ok(dm)) => Ok(bridge::from_wit_dm_channel(dm)),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn add_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_add_group_member(&mut *store, group_id, user_id)
            .await;
        convert_result_unit(result)
    }

    async fn remove_group_member(&self, group_id: &str, user_id: &str) -> ClientResult<()> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_remove_group_member(&mut *store, group_id, user_id)
            .await;
        convert_result_unit(result)
    }

    async fn add_users_to_group_dm(
        &self,
        _channel_id: &str,
        _user_ids: &[String],
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "add_users_to_group_dm: not exposed in WIT interface".to_string(),
        ))
    }

    async fn close_dm_channel(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "close_dm_channel: not exposed in WIT interface".to_string(),
        ))
    }

    async fn mute_conversation(
        &self,
        _channel_id: &str,
        _until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "mute_conversation: not exposed in WIT interface".to_string(),
        ))
    }

    async fn unmute_conversation(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "unmute_conversation: not exposed in WIT interface".to_string(),
        ))
    }

    async fn leave_group_dm(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "leave_group_dm: not exposed in WIT interface".to_string(),
        ))
    }

    async fn edit_group_dm(
        &self,
        _channel_id: &str,
        _name: Option<&str>,
        _avatar_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "edit_group_dm: not exposed in WIT interface".to_string(),
        ))
    }
}
