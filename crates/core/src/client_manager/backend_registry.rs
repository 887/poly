//! `BackendRegistry` — the grouping type for backend-routing fields.
//!
//! **Single reason to change:** a backend is added, removed, or updates its
//! capability declaration. This sub-store is completely orthogonal to account
//! identity ([`AccountIdentity`]) and plugin registration ([`PluginRegistry`]).
//!
//! This type is provided as a **documentation aid and future-migration target**.
//! `ClientManager` currently retains its flat field layout so existing call
//! sites (`cm.backends`, `cm.expected_account_ids`, etc.) continue to compile
//! without changes. A future caller-migration pass will move `ClientManager`
//! to embed a `BackendRegistry` value.
//!
//! Part of the SRP split of `ClientManager` — see
//! `docs/plans/plan-solid-audit-core-state.md` Phase B.1.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use poly_client::{BackendCapabilities, IsBackend};
use tokio::sync::RwLock;

/// A shared, thread-safe handle to a messenger backend.
///
/// Re-declared here for use in `BackendRegistry` type signatures.  The
/// canonical `pub type BackendHandle` lives in the parent `mod.rs`.
type BackendHandle = Arc<RwLock<Box<dyn IsBackend>>>;

/// Logical grouping for the backend-routing fields of `ClientManager`.
///
/// Contains the fields whose **only reason to change** is that a backend is
/// added, removed, or updates its runtime capability declaration:
///
/// | Field | Purpose |
/// |-------|---------|
/// | `backends` | Live `Arc<RwLock<Box<dyn IsBackend>>>` handles, keyed by account ID |
/// | `server_account_map` | Cached server-ID → account-ID routing |
/// | `expected_account_ids` | Account IDs known from storage, not yet restored |
/// | `backend_capabilities` | Runtime capability registry (seeded at startup) |
#[derive(Default)]
pub struct BackendRegistry {
    /// Active backends keyed by account ID.
    pub backends: HashMap<String, BackendHandle>,
    /// Cached mapping from server ID → account ID that owns it.
    pub server_account_map: HashMap<String, String>,
    /// Account IDs that exist in persisted storage but have not yet been restored
    /// into `backends` / `sessions`. Cleared entry-by-entry as accounts restore.
    pub expected_account_ids: HashSet<String>,
    /// Runtime capability registry: backend slug → `BackendCapabilities`.
    ///
    /// Seeded from the compile-time static table in `ClientManager::new()` and
    /// overwritten per-slug by `commit_backend_account` when a live backend connects.
    pub backend_capabilities: HashMap<String, BackendCapabilities>,
}

impl Clone for BackendRegistry {
    fn clone(&self) -> Self {
        Self {
            backends: self.backends.clone(),
            server_account_map: self.server_account_map.clone(),
            expected_account_ids: self.expected_account_ids.clone(),
            backend_capabilities: self.backend_capabilities.clone(),
        }
    }
}
