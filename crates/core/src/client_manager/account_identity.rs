//! `AccountIdentity` — the grouping type for account identity fields.
//!
//! **Single reason to change:** the authenticated identity state for an account
//! changes (new login, status update, logout, disable/re-enable). This is
//! completely orthogonal to which backends are active ([`BackendRegistry`]) and
//! which plugins are registered ([`PluginRegistry`]).
//!
//! This type is provided as a **documentation aid and future-migration target**.
//! `ClientManager` currently retains its flat field layout so existing call
//! sites (`cm.sessions`, `cm.connection_statuses`, etc.) continue to compile
//! without changes. A future caller-migration pass will move `ClientManager`
//! to embed an `AccountIdentity` value.
//!
//! Part of the SRP split of `ClientManager` — see
//! `docs/plans/plan-solid-audit-core-state.md` Phase B.1.

use std::collections::HashMap;

use poly_client::{AccountPresence, ConnectionStatus, Session};

/// Logical grouping for the account-identity fields of `ClientManager`.
///
/// Contains the fields whose **only reason to change** is that account
/// identity state changes — login, logout, connection status update, presence
/// update, or the disabled-backends list is modified:
///
/// | Field | Purpose |
/// |-------|---------|
/// | `sessions` | Authenticated sessions keyed by account ID |
/// | `connection_statuses` | Live connection state per account |
/// | `presence_statuses` | User-chosen presence/availability per account |
/// | `disabled_native_backends` | Backends disabled by the user in Settings |
#[derive(Default)]
pub struct AccountIdentity {
    /// Authenticated sessions keyed by account ID.
    ///
    /// Stored so the UI can retrieve per-account identity info (e.g. `icon_emoji`)
    /// without going through the async backend trait. Also holds offline/cached
    /// sessions for accounts restored from storage while the server is unreachable.
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
    /// Native backends currently disabled by the user in Settings → Plugins.
    pub disabled_native_backends: Vec<String>,
}

impl Clone for AccountIdentity {
    fn clone(&self) -> Self {
        Self {
            sessions: self.sessions.clone(),
            connection_statuses: self.connection_statuses.clone(),
            presence_statuses: self.presence_statuses.clone(),
            disabled_native_backends: self.disabled_native_backends.clone(),
        }
    }
}
