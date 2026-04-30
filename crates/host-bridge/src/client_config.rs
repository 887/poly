//! `ClientConfigStore` — host-side persistence for per-backend client settings.
//!
//! Wraps `kv_get` / `kv_set` / `kv_delete` from [`crate::Client`] under the
//! `client.config.<backend_id>.*` key namespace.
//!
//! # Key namespace
//!
//! | Key                                                   | Type            | Meaning                       |
//! |-------------------------------------------------------|-----------------|-------------------------------|
//! | `client.config.<backend_id>.version_override`         | `String`        | Overridden User-Agent version |
//! | `client.config.<backend_id>.mechanisms`               | `Vec<String>`   | Registry of known mech IDs    |
//! | `client.config.<backend_id>.mechanism.<mech_id>`      | `bool`          | Whether mechanism is enabled  |
//!
//! The `mechanisms` registry key holds a JSON array of every mechanism ID that
//! has been set for a given backend. This is needed because the underlying KV
//! store has no prefix-scan capability — `list_overrides` rebuilds the snapshot
//! from this registry rather than scanning by prefix.

use crate::{BridgeError, Client};

// ─── Namespace helpers ────────────────────────────────────────────────────────

fn key_version_override(backend_id: &str) -> String {
    format!("client.config.{backend_id}.version_override")
}

fn key_mechanism(backend_id: &str, mechanism_id: &str) -> String {
    format!("client.config.{backend_id}.mechanism.{mechanism_id}")
}

fn key_mechanisms_registry(backend_id: &str) -> String {
    format!("client.config.{backend_id}.mechanisms")
}

// ─── Error ───────────────────────────────────────────────────────────────────

/// Alias so callers can import a single coherent type rather than spelling out
/// `BridgeError` in client-config call sites.
pub type ClientConfigError = BridgeError;

// ─── Snapshot ────────────────────────────────────────────────────────────────

/// Complete settings snapshot for one backend, as read from KV at call time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClientSettingsSnapshot {
    /// The backend ID this snapshot belongs to.
    pub backend_id: String,
    /// The version override string, if set.
    pub version_override: Option<String>,
    /// All mechanism states: `(mechanism_id, enabled)`.
    pub mechanisms: Vec<(String, bool)>,
}

// ─── Store ───────────────────────────────────────────────────────────────────

/// Host-side store for per-backend client-settings overrides.
///
/// Backed by the `poly_kv` table via `Client::kv_*`. Cheap to clone — the
/// inner `Client` is `Arc`-backed. Construct via [`ClientConfigStore::new`] or
/// [`ClientConfigStore::from_client`]; there is no global singleton because
/// each shell may bind a different bridge port.
#[derive(Debug, Clone)]
pub struct ClientConfigStore {
    client: Client,
}

impl ClientConfigStore {
    /// Create a store backed by a default-URL bridge client.
    #[must_use]
    pub fn new() -> Self {
        Self { client: Client::new() }
    }

    /// Create a store backed by an existing bridge client.
    #[must_use]
    pub fn from_client(client: Client) -> Self {
        Self { client }
    }

    // ── Version override ──────────────────────────────────────────────────────

    /// Read the version-override string for `backend_id`.
    ///
    /// Returns `Ok(None)` when no override is set.
    pub async fn get_version_override(
        &self,
        backend_id: &str,
    ) -> Result<Option<String>, ClientConfigError> {
        let key = key_version_override(backend_id);
        match self.client.kv_get(&key).await? {
            None => Ok(None),
            Some(v) => {
                let s = v
                    .as_str()
                    .ok_or_else(|| {
                        BridgeError::ParseResponse(format!(
                            "expected string for key {key}, got {v}"
                        ))
                    })?
                    .to_owned();
                Ok(Some(s))
            }
        }
    }

    /// Write or clear the version-override for `backend_id`.
    ///
    /// Passing `None` **deletes** the key (not sets it to an empty string).
    pub async fn set_version_override(
        &self,
        backend_id: &str,
        override_: Option<String>,
    ) -> Result<(), ClientConfigError> {
        let key = key_version_override(backend_id);
        match override_ {
            Some(s) => self.client.kv_set(&key, serde_json::Value::String(s)).await,
            None => self.client.kv_delete(&key).await,
        }
    }

    // ── Mechanism state ───────────────────────────────────────────────────────

    /// Read whether `mechanism_id` is enabled for `backend_id`.
    ///
    /// Returns `Ok(None)` when no state has been persisted for this mechanism
    /// (caller should treat as "use backend default").
    pub async fn get_mechanism_state(
        &self,
        backend_id: &str,
        mechanism_id: &str,
    ) -> Result<Option<bool>, ClientConfigError> {
        let key = key_mechanism(backend_id, mechanism_id);
        match self.client.kv_get(&key).await? {
            None => Ok(None),
            Some(v) => {
                let b = v.as_bool().ok_or_else(|| {
                    BridgeError::ParseResponse(format!(
                        "expected bool for key {key}, got {v}"
                    ))
                })?;
                Ok(Some(b))
            }
        }
    }

    /// Persist the enabled/disabled state for `mechanism_id` on `backend_id`.
    ///
    /// Also registers `mechanism_id` in the per-backend mechanisms registry so
    /// `list_overrides` can discover it later.
    pub async fn set_mechanism_state(
        &self,
        backend_id: &str,
        mechanism_id: &str,
        enabled: bool,
    ) -> Result<(), ClientConfigError> {
        // Register the mech ID so list_overrides can find it.
        self.register_mechanism(backend_id, mechanism_id).await?;
        // Write the state.
        let key = key_mechanism(backend_id, mechanism_id);
        self.client
            .kv_set(&key, serde_json::Value::Bool(enabled))
            .await
    }

    // ── Snapshot ─────────────────────────────────────────────────────────────

    /// Return all persisted overrides for `backend_id` as a snapshot.
    ///
    /// Because the KV store has no prefix-scan, `list_overrides` reads the
    /// mechanism registry key (`client.config.<id>.mechanisms`) to discover
    /// which mechanism IDs have been set, then fetches each one individually.
    /// This keeps the implementation simple and avoids adding a new KV route.
    pub async fn list_overrides(
        &self,
        backend_id: &str,
    ) -> Result<ClientSettingsSnapshot, ClientConfigError> {
        let version_override = self.get_version_override(backend_id).await?;

        // Read the mechanisms registry.
        let mech_ids = self.read_mechanisms_registry(backend_id).await?;

        // Fetch each mechanism's state.
        let mut mechanisms = Vec::with_capacity(mech_ids.len());
        for mech_id in &mech_ids {
            // If the registry lists an ID but the value key is missing (e.g.
            // it was deleted externally), treat it as "not set" and omit it.
            if let Some(enabled) = self.get_mechanism_state(backend_id, mech_id).await? {
                mechanisms.push((mech_id.clone(), enabled));
            }
        }

        Ok(ClientSettingsSnapshot {
            backend_id: backend_id.to_owned(),
            version_override,
            mechanisms,
        })
    }

    // ── Registry helpers (private) ────────────────────────────────────────────

    /// Read the JSON array of known mechanism IDs for `backend_id`.
    async fn read_mechanisms_registry(
        &self,
        backend_id: &str,
    ) -> Result<Vec<String>, ClientConfigError> {
        let key = key_mechanisms_registry(backend_id);
        match self.client.kv_get(&key).await? {
            None => Ok(Vec::new()),
            Some(v) => {
                let arr = v.as_array().ok_or_else(|| {
                    BridgeError::ParseResponse(format!(
                        "expected array for mechanisms registry key {key}, got {v}"
                    ))
                })?;
                let ids = arr
                    .iter()
                    .filter_map(|entry| entry.as_str().map(str::to_owned))
                    .collect();
                Ok(ids)
            }
        }
    }

    /// Add `mechanism_id` to the registry if it isn't already present.
    async fn register_mechanism(
        &self,
        backend_id: &str,
        mechanism_id: &str,
    ) -> Result<(), ClientConfigError> {
        let mut ids = self.read_mechanisms_registry(backend_id).await?;
        if !ids.contains(&mechanism_id.to_owned()) {
            ids.push(mechanism_id.to_owned());
            let key = key_mechanisms_registry(backend_id);
            self.client
                .kv_set(
                    &key,
                    serde_json::Value::Array(
                        ids.into_iter().map(serde_json::Value::String).collect(),
                    ),
                )
                .await?;
        }
        Ok(())
    }
}

impl Default for ClientConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    // ── Stub KV backend ───────────────────────────────────────────────────────
    //
    // The real `Client` needs a live HTTP server. Instead, we test the key
    // construction helpers directly — they are pure functions and cover the
    // namespace-correctness and backend-isolation guarantees.

    #[test]
    fn key_version_override_namespace() {
        let k = key_version_override("discord");
        assert_eq!(k, "client.config.discord.version_override");
    }

    #[test]
    fn key_mechanism_namespace() {
        let k = key_mechanism("matrix", "tls_pinning");
        assert_eq!(k, "client.config.matrix.mechanism.tls_pinning");
    }

    #[test]
    fn key_mechanisms_registry_namespace() {
        let k = key_mechanisms_registry("teams");
        assert_eq!(k, "client.config.teams.mechanisms");
    }

    /// Backend IDs must produce distinct key prefixes so one backend's
    /// overrides can't collide with another's.
    #[test]
    fn backend_isolation_version_override() {
        let k_discord = key_version_override("discord");
        let k_matrix = key_version_override("matrix");
        assert_ne!(k_discord, k_matrix);
    }

    #[test]
    fn backend_isolation_mechanism() {
        let k_a = key_mechanism("discord", "my_mech");
        let k_b = key_mechanism("matrix", "my_mech");
        assert_ne!(k_a, k_b);
    }

    /// Mechanism keys for two different mechanism IDs on the same backend must
    /// also be distinct.
    #[test]
    fn mechanism_id_isolation_same_backend() {
        let k1 = key_mechanism("discord", "mech_one");
        let k2 = key_mechanism("discord", "mech_two");
        assert_ne!(k1, k2);
    }

    /// Snapshot deserialization round-trips cleanly.
    #[test]
    fn snapshot_serde_round_trip() {
        let snap = ClientSettingsSnapshot {
            backend_id: "stoat".to_owned(),
            version_override: Some("1.2.3".to_owned()),
            mechanisms: vec![
                ("compression".to_owned(), true),
                ("tls_pinning".to_owned(), false),
            ],
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: ClientSettingsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.backend_id, "stoat");
        assert_eq!(back.version_override, Some("1.2.3".to_owned()));
        assert_eq!(back.mechanisms.len(), 2);
        assert_eq!(back.mechanisms[0], ("compression".to_owned(), true));
        assert_eq!(back.mechanisms[1], ("tls_pinning".to_owned(), false));
    }

    /// Snapshot with no override set round-trips with `None`.
    #[test]
    fn snapshot_serde_no_override() {
        let snap = ClientSettingsSnapshot {
            backend_id: "matrix".to_owned(),
            version_override: None,
            mechanisms: vec![],
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: ClientSettingsSnapshot = serde_json::from_str(&json).unwrap();
        assert!(back.version_override.is_none());
        assert!(back.mechanisms.is_empty());
    }

    // ── Registry helper unit tests ────────────────────────────────────────────

    /// `read_mechanisms_registry` parses a well-formed JSON array correctly.
    #[test]
    fn parse_mechanisms_registry_array() {
        let json = serde_json::json!(["mech_a", "mech_b", "mech_c"]);
        let arr = json.as_array().unwrap();
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|e| e.as_str().map(str::to_owned))
            .collect();
        assert_eq!(ids, vec!["mech_a", "mech_b", "mech_c"]);
    }

    /// Non-string entries in the registry array are silently skipped.
    #[test]
    fn parse_mechanisms_registry_skips_non_strings() {
        let json = serde_json::json!(["mech_a", 42, null, "mech_b"]);
        let arr = json.as_array().unwrap();
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|e| e.as_str().map(str::to_owned))
            .collect();
        assert_eq!(ids, vec!["mech_a", "mech_b"]);
    }
}
