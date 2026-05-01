//! Simple opaque token auth for mock test servers.
//!
//! Tokens are random UUIDs mapped to user IDs in a DashMap. Optionally
//! persisted to a JSON file so tokens survive server restarts — important
//! when a test UI has an account stored and we restart the backing server
//! twenty times in a row while iterating.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Shared auth state: maps opaque tokens → user IDs.
#[derive(Clone, Default)]
pub struct AuthState {
    /// token → user_id
    tokens: DashMap<String, String>,
    /// Optional on-disk location. When set, mutations write through.
    persist_path: Arc<Mutex<Option<PathBuf>>>,
}

#[derive(Serialize, Deserialize, Default)]
struct OnDisk {
    tokens: HashMap<String, String>,
}

impl AuthState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load tokens from `path`. If the file is missing or malformed, start
    /// empty. Subsequent mutations are written back to `path`.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let tokens = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<OnDisk>(&bytes).map_or_else(
                |e| {
                    tracing::warn!("auth file {} is malformed ({e}); starting empty", path.display());
                    DashMap::new()
                },
                |d| d.tokens.into_iter().collect::<DashMap<_, _>>(),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => DashMap::new(),
            Err(e) => {
                tracing::warn!("could not read auth file {}: {e}", path.display());
                DashMap::new()
            }
        };
        tracing::info!(
            "auth: loaded {} persisted token(s) from {}",
            tokens.len(),
            path.display()
        );
        Self {
            tokens,
            persist_path: Arc::new(Mutex::new(Some(path))),
        }
    }

    /// Best-effort write of the current token map to the persist path.
    fn save(&self) {
        let Some(path) = self
            .persist_path
            .lock()
            .ok()
            .and_then(|g| g.as_ref().cloned())
        else {
            return;
        };
        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!("could not create auth dir {}: {e}", parent.display());
            return;
        }
        let snapshot: HashMap<String, String> = self
            .tokens
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();
        let bytes = match serde_json::to_vec_pretty(&OnDisk { tokens: snapshot }) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("serialize auth file failed: {e}");
                return;
            }
        };
        if let Err(e) = std::fs::write(&path, bytes) {
            tracing::warn!("write auth file {} failed: {e}", path.display());
        }
    }

    /// Create a new token for the given user ID. Returns the token string.
    #[must_use]
    pub fn create_token(&self, user_id: &str) -> String {
        let token = Uuid::new_v4().to_string();
        self.tokens.insert(token.clone(), user_id.to_string());
        self.save();
        token
    }

    /// Validate a token and return the associated user ID.
    #[must_use]
    pub fn validate(&self, token: &str) -> Option<String> {
        self.tokens.get(token).map(|entry| entry.value().clone())
    }

    /// Revoke a token (logout).
    pub fn revoke(&self, token: &str) {
        self.tokens.remove(token);
        self.save();
    }

    /// Clear all tokens in memory and on disk.
    pub fn clear(&self) {
        self.tokens.clear();
        self.save();
    }
}

/// Delete the persisted auth file at `path`, if any. Used by `--reset` at
/// server startup before the AuthState is loaded.
pub fn wipe_persisted(path: impl AsRef<Path>) {
    let path = path.as_ref();
    match std::fs::remove_file(path) {
        Ok(()) => tracing::info!("auth: wiped {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!("could not wipe {}: {e}", path.display()),
    }
}

/// Trait for extracting auth from request headers.
/// Implement per-backend since header formats vary
/// (Matrix: `Bearer`, Stoat: `x-session-token`, Discord: `Bot`, etc.)
pub trait TokenAuth {
    /// Extract user ID from request headers, if authenticated.
    fn extract_user_id(&self, auth_header: Option<&str>) -> Option<String>;
}

impl TokenAuth for AuthState {
    fn extract_user_id(&self, auth_header: Option<&str>) -> Option<String> {
        let header = auth_header?;
        let token = header.strip_prefix("Bearer ").unwrap_or(header);
        self.validate(token)
    }
}
