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
    AuthCredentials, BackendType, Channel, ClientBackend, ClientError, ClientEvent, ClientResult,
    DmChannel, Group, Message, MessageContent, MessageQuery, MessageSearchHit, MessageSearchQuery,
    Notification, PresenceStatus, Server, Session, User, VoiceParticipant,
};

use super::bridge;
use super::engine::{self, MessengerPlugin};
use super::host_impl::PluginHostState;

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
        })
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
        let component = self
            .components
            .get(plugin_id)
            .ok_or_else(|| format!("Plugin '{plugin_id}' not loaded"))?;

        let host_state = PluginHostState::new(plugin_id);
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
        let cached_backend_type = bridge::from_wit_backend_type(wit_backend_type);

        // Refuel before the next call
        let _ = store.set_fuel(1_000_000_000);

        let cached_backend_name = instance
            .poly_messenger_messenger_client()
            .call_get_backend_name(&mut store)
            .await
            .map_err(|e| format!("Failed to get backend_name from '{plugin_id}': {e}"))?;

        tracing::info!(
            "Plugin '{plugin_id}' reports: type={:?}, name={cached_backend_name}",
            cached_backend_type,
        );

        Ok(PluginBackend {
            plugin_id: plugin_id.to_string(),
            cached_backend_type,
            cached_backend_name,
            store: Arc::new(Mutex::new(store)),
            instance: Arc::new(instance),
        })
    }

    /// Get the IDs of all loaded plugins.
    pub fn loaded_plugins(&self) -> Vec<String> {
        self.components.keys().cloned().collect()
    }

    /// Check if a plugin is loaded.
    pub fn is_loaded(&self, plugin_id: &str) -> bool {
        self.components.contains_key(plugin_id)
    }
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
    let _ = guard.set_fuel(1_000_000_000);
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

    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        refuel(&self.store).await;
        let wit_query = bridge::to_wit_message_search_query(query);
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_search_messages(&mut *store, &wit_query)
            .await;
        match result {
            Ok(Ok(hits)) => Ok(hits
                .into_iter()
                .map(bridge::from_wit_message_search_hit)
                .collect()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
        refuel(&self.store).await;
        let mut store = self.store.lock().await;
        let result = self
            .instance
            .poly_messenger_messenger_client()
            .call_get_pinned_messages(&mut *store, channel_id)
            .await;
        match result {
            Ok(Ok(messages)) => Ok(messages.into_iter().map(bridge::from_wit_message).collect()),
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
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(bridge::from_wit_client_error(e)),
            Err(e) => Err(ClientError::Internal(format!("WASM runtime error: {e}"))),
        }
    }

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

    fn event_stream(&self) -> std::pin::Pin<Box<dyn Stream<Item = ClientEvent> + Send>> {
        // Create a stream that polls the plugin's poll_event export.
        // We use a channel-based approach: spawn a task that polls and forwards events.
        let store = self.store.clone();
        let instance = self.instance.clone();

        let (tx, rx) = tokio::sync::mpsc::channel::<ClientEvent>(64);

        tokio::spawn(async move {
            loop {
                // Refuel before each poll
                {
                    let mut guard = store.lock().await;
                    let _ = guard.set_fuel(1_000_000_000);
                }

                let event = {
                    let mut guard = store.lock().await;
                    instance
                        .poly_messenger_messenger_client()
                        .call_poll_event(&mut *guard)
                        .await
                };

                match event {
                    Ok(Some(wit_event)) => {
                        let client_event = bridge::from_wit_client_event(wit_event);
                        if tx.send(client_event).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Ok(None) => {
                        // No event pending — sleep briefly before polling again
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => {
                        tracing::error!("Plugin poll_event error: {e}");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        });

        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }

    fn backend_type(&self) -> BackendType {
        self.cached_backend_type
    }

    fn backend_name(&self) -> &str {
        &self.cached_backend_name
    }
}
