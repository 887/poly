//! Backend pool — manages authenticated `ClientBackend` instances.

use poly_client::{AuthCredentials, BackendType, ClientBackend, Session};
use std::collections::HashMap;

/// An authenticated backend connection.
pub struct BackendEntry {
    pub backend: Box<dyn ClientBackend + Send + Sync>,
    pub session: Session,
}

/// Pool of authenticated backends, keyed by "backend_type:account_id".
pub struct BackendPool {
    backends: HashMap<String, BackendEntry>,
}

impl BackendPool {
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    fn key(backend_type: BackendType, account_id: &str) -> String {
        format!("{:?}:{}", backend_type, account_id)
    }

    /// Add an authenticated backend to the pool.
    pub fn insert(&mut self, session: Session, backend: Box<dyn ClientBackend + Send + Sync>) {
        let key = Self::key(session.backend.clone(), &session.user.id);
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

    /// Remove a backend from the pool.
    pub fn remove(&mut self, backend_type: BackendType, account_id: &str) -> Option<BackendEntry> {
        let key = Self::key(backend_type, account_id);
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
            let client = poly_hackernews::HackerNewsClient::with_base_url(url);
            Ok((Box::new(client), BackendType::from("hackernews")))
        }
        "discord" => anyhow::bail!("Discord client not yet implemented"),
        "teams" => anyhow::bail!("Teams client not yet implemented"),
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
