//! Backend pool — manages authenticated `ClientBackend` instances.

use crate::events::{SharedEventStore, new_event_store, spawn_fan_out};
use poly_client::{AuthCredentials, BackendType, ClientBackend, Session};
use std::collections::HashMap;
use tokio::task::JoinHandle;

/// An authenticated backend connection.
pub struct BackendEntry {
    pub backend: Box<dyn ClientBackend + Send + Sync>,
    pub session: Session,
}

/// Pool of authenticated backends, keyed by "backend_type:account_id".
pub struct BackendPool {
    backends: HashMap<String, BackendEntry>,
    /// Shared event store for Phase C event subscription / poll_events.
    pub events: SharedEventStore,
    /// Per-account fan-out task handles + shutdown senders.
    fan_out_tasks: HashMap<String, (JoinHandle<()>, tokio::sync::oneshot::Sender<()>)>,
}

impl BackendPool {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
            events: new_event_store(),
            fan_out_tasks: HashMap::new(),
        }
    }

    fn key(backend_type: BackendType, account_id: &str) -> String {
        format!("{:?}:{}", backend_type, account_id)
    }

    /// Add an authenticated backend to the pool.
    ///
    /// After inserting, a Phase-C fan-out task is spawned for this backend so
    /// its real-time events flow into the shared `EventStore`.
    pub fn insert(&mut self, session: Session, backend: Box<dyn ClientBackend + Send + Sync>) {
        let key = Self::key(session.backend.clone(), &session.user.id);

        // Start event fan-out for this backend.
        let stream = backend.event_stream();
        let (handle, shutdown) = spawn_fan_out(key.clone(), stream, self.events.clone());
        // If there was already a fan-out for this key (re-login), shut the old one down.
        if let Some((old_handle, old_shutdown)) = self.fan_out_tasks.remove(&key) {
            let _ = old_shutdown.send(());
            old_handle.abort();
        }
        self.fan_out_tasks.insert(key.clone(), (handle, shutdown));

        self.backends.insert(key, BackendEntry { backend, session });
    }

    /// Get a backend by type and account ID.
    pub fn get(&self, backend_type: BackendType, account_id: &str) -> Option<&BackendEntry> {
        let key = Self::key(backend_type, account_id);
        self.backends.get(&key)
    }

    /// Find the first backend of a given type (for single-account usage).
    pub fn find_by_type(&self, backend_type: BackendType) -> Option<&BackendEntry> {
        self.backends
            .values()
            .find(|e| e.session.backend == backend_type)
    }

    /// Find the first backend whose session account_id matches (backend-agnostic).
    pub fn find_by_account(&self, account_id: &str) -> Option<&BackendEntry> {
        self.backends
            .values()
            .find(|e| e.session.user.id == account_id)
    }

    /// Remove a backend from the pool.
    ///
    /// Also stops the Phase-C fan-out task for this account.
    pub fn remove(&mut self, backend_type: BackendType, account_id: &str) -> Option<BackendEntry> {
        let key = Self::key(backend_type, account_id);
        // Stop fan-out task.
        if let Some((handle, shutdown)) = self.fan_out_tasks.remove(&key) {
            let _ = shutdown.send(());
            handle.abort();
        }
        self.backends.remove(&key)
    }

    /// List all connected accounts.
    pub fn list_accounts(&self) -> Vec<serde_json::Value> {
        self.backends
            .values()
            .map(|e| {
                serde_json::json!({
                    "backend": format!("{:?}", e.session.backend),
                    "user_id": e.session.user.id,
                    "display_name": e.session.user.display_name,
                    "avatar_url": e.session.user.avatar_url,
                })
            })
            .collect()
    }

    /// Create and authenticate a backend.
    pub async fn login(
        &mut self,
        backend_type_str: &str,
        url: &str,
        credentials: AuthCredentials,
    ) -> anyhow::Result<Session> {
        let (mut backend, _bt) = create_backend(backend_type_str, url)?;
        let session = backend
            .authenticate(credentials)
            .await
            .map_err(|e| anyhow::anyhow!("auth failed: {e}"))?;
        self.insert(session.clone(), backend);
        Ok(session)
    }
}

/// Create an unauthenticated backend instance.
fn create_backend(
    backend_type: &str,
    url: &str,
) -> anyhow::Result<(Box<dyn ClientBackend + Send + Sync>, BackendType)> {
    match backend_type {
        "stoat" => {
            let client = poly_stoat::StoatClient::with_base_url(url)
                .map_err(|e| anyhow::anyhow!("stoat config: {e}"))?;
            Ok((Box::new(client), BackendType::from("stoat")))
        }
        "matrix" => {
            let client = poly_matrix::MatrixClient::with_homeserver(url)
                .map_err(|e| anyhow::anyhow!("matrix config: {e}"))?;
            Ok((Box::new(client), BackendType::from("matrix")))
        }
        "lemmy" => {
            let client = poly_lemmy::LemmyClient::new(url);
            Ok((Box::new(client), BackendType::from("lemmy")))
        }
        "hackernews" | "hn" => {
            // HN API paths live under /v0/ — append it if not already present.
            let hn_url = if url.ends_with("/v0") || url.contains("/v0/") {
                url.to_string()
            } else {
                format!("{}/v0", url.trim_end_matches('/'))
            };
            let client = poly_hackernews::HackerNewsClient::with_base_url(hn_url);
            Ok((Box::new(client), BackendType::from("hackernews")))
        }
        "discord" => {
            let client = poly_discord::DiscordClient::with_base_url(url.to_string());
            Ok((Box::new(client), BackendType::from("discord")))
        }
        "teams" => {
            let client = poly_teams::TeamsClient::with_base_url(url.to_string());
            Ok((Box::new(client), BackendType::from("teams")))
        }
        "poly" => {
            // For MCP, we need the caller to provide private key bytes.
            // For now, generate an ephemeral key.
            let key: [u8; 32] = rand::random();
            let client = poly_server_client::PolyServerBackend::new(url, key);
            Ok((Box::new(client), BackendType::from("poly")))
        }
        _ => anyhow::bail!("unknown backend type: {backend_type}"),
    }
}
