//! Backup server sync client for Poly.
//!
//! Connects to one or more Poly backup servers to synchronize
//! encrypted settings. All data is encrypted client-side before
//! being sent to the server.
//!
//! ## Auth Flow
//! 1. `POST /api/challenge` with `{ public_key }` → `{ nonce, difficulty, expires_at }`
//! 2. Mine: find `counter` such that SHA-256(nonce + counter_decimal) has `difficulty` leading zero bits
//! 3. `POST /api/auth` with `{ public_key, nonce, counter, passphrase, device_name }` → `{ token, expires_at }`
//! 4. Use `Authorization: Bearer <token>` for all subsequent requests

// DECISION(D10): PoW challenge + server passphrase + long tokens + device tracking.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Types ─────────────────────────────────────────────────────────────────────

/// Backup server connection configuration (stored per-server in SurrealKV).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupServerConfig {
    /// Server URL (e.g., `"http://localhost:8080"`).
    pub url: String,
    /// Display name for this server.
    pub name: String,
    /// Stored session token (if authenticated).
    pub token: Option<String>,
    /// Last sync sequence number.
    pub last_sequence: u64,
}

/// PoW challenge response from `POST /api/challenge`.
#[derive(Debug, Clone, Deserialize)]
pub struct PowChallenge {
    /// Random nonce string issued by the server.
    pub nonce: String,
    /// Required number of leading zero bits in SHA-256(nonce + counter).
    pub difficulty: u32,
    /// ISO-8601 UTC timestamp when the challenge expires.
    pub expires_at: String,
}

/// Response from `POST /api/auth`.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthResponse {
    /// 128-character session token.
    pub token: String,
    /// ISO-8601 UTC expiry timestamp.
    pub expires_at: Option<String>,
}

/// Encrypted sync blob returned from `GET /api/sync/pull`.
///
/// Field names match exactly what the server serialises in [`BlobEntry`]
/// (`servers/backup-server/src/sync/mod.rs`).
#[derive(Debug, Clone, Deserialize)]
pub struct SyncBlob {
    /// Monotonically increasing sequence number.
    pub sequence: i64,
    /// Base64-encoded encrypted payload (opaque to the server).
    pub encrypted_blob: String,
    /// ISO-8601 UTC timestamp when the blob was pushed.
    pub pushed_at: String,
}

impl SyncBlob {
    /// Decode the base64 `encrypted_blob` field into raw bytes.
    pub fn decode_data(&self) -> Result<Vec<u8>, SyncError> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.encrypted_blob)
            .map_err(|e| SyncError::Protocol(format!("base64 decode: {e}")))
    }
}

/// Public metadata returned by `GET /api/info`.
///
/// Clients call this endpoint during the setup wizard before authenticating.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    /// Human-readable server name.
    pub name: String,
    /// Whether the server requires a passphrase for authentication.
    pub password_required: bool,
    /// Whether new user registrations are currently accepted.
    pub registrations_open: bool,
    /// Server software version string.
    pub version: String,
}

/// Probe a backup server's public info endpoint without authenticating.
///
/// Returns [`ServerInfo`] on success. No session token is required.
/// Use this during the setup wizard to validate the URL and learn the
/// server's name and password policy before asking the user for credentials.
pub async fn probe_server(url: &str) -> Result<ServerInfo, SyncError> {
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{url}/api/info"))
        .send()
        .await
        .map_err(|e| SyncError::Network(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        return Err(SyncError::Server(format!("HTTP {status}: {text}")));
    }

    resp.json::<ServerInfo>()
        .await
        .map_err(|e| SyncError::Protocol(format!("Bad /api/info response: {e}")))
}

/// Account status from `GET /api/sync/status`.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncStatus {
    /// This account's Ed25519 public key.
    pub public_key: String,
    /// Total number of blobs stored.
    pub blob_count: i64,
    /// Highest sequence number stored.
    pub latest_sequence: i64,
}

/// Errors from backup sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Network / HTTP error.
    #[error("network error: {0}")]
    Network(String),

    /// Authentication failed.
    #[error("auth failed: {0}")]
    AuthFailed(String),

    /// Server returned an error response.
    #[error("server error: {0}")]
    Server(String),

    /// Protocol / deserialization mismatch.
    #[error("protocol error: {0}")]
    Protocol(String),
}

// ── PoW solver ────────────────────────────────────────────────────────────────

/// Solve a PoW challenge by finding a `counter` such that
/// `SHA-256(nonce + counter.to_string())` has `difficulty` leading zero bits.
///
/// Matches the server-side `verify_pow()` in `poly-backup-server/src/auth/mod.rs`.
#[must_use] 
pub fn solve_pow(nonce: &str, difficulty: u32) -> u64 {
    let mut counter: u64 = 0;
    loop {
        let input = format!("{nonce}{counter}");
        let hash = Sha256::digest(input.as_bytes());
        if check_pow_difficulty(&hash, difficulty) {
            return counter;
        }
        counter = counter.wrapping_add(1);
    }
}

/// Check if a hash meets PoW difficulty (leading zero bits).
fn check_pow_difficulty(hash: &[u8], difficulty: u32) -> bool {
    if difficulty == 0 {
        return true;
    }
    // lint-allow-unused: difficulty/8 cannot exceed difficulty, and conversion
    // to usize is safe (difficulty is u32). Modulo by const 8 is safe.
    #[allow(
        clippy::integer_division,
        clippy::arithmetic_side_effects,
        clippy::as_conversions,
        clippy::cast_possible_truncation
    )]
    let full_bytes = (difficulty / 8) as usize;
    #[allow(clippy::arithmetic_side_effects)]
    let remaining_bits = difficulty % 8;
    for byte in hash.iter().take(full_bytes) {
        if *byte != 0 {
            return false;
        }
    }
    if remaining_bits > 0 {
        // lint-allow-unused: remaining_bits is in 1..8 (we just checked > 0
        // and it's % 8); 8 - remaining_bits stays in 1..8 — never overflows.
        #[allow(clippy::arithmetic_side_effects)]
        let mask = 0xFF_u8 << (8 - remaining_bits);
        if hash.get(full_bytes).is_some_and(|b| b & mask != 0) {
            return false;
        }
    }
    true
}

// ── Sync client ───────────────────────────────────────────────────────────────

/// Backup sync client — communicates with one `poly-backup-server` instance.
pub struct SyncClient {
    http: reqwest::Client,
    config: BackupServerConfig,
}

impl SyncClient {
    /// Create a new sync client for a backup server.
    #[must_use] 
    pub fn new(config: BackupServerConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    /// Base URL of the server.
    #[must_use] 
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Current stored session token (`None` if not yet authenticated).
    #[must_use] 
    pub fn token(&self) -> Option<&str> {
        self.config.token.as_deref()
    }

    /// Reference to the current config (e.g. to persist after auth).
    #[must_use] 
    pub fn config(&self) -> &BackupServerConfig {
        &self.config
    }

    /// `POST /api/challenge` — request a PoW nonce for the given public key.
    pub async fn request_challenge(&self, public_key: &str) -> Result<PowChallenge, SyncError> {
        let resp = self
            .http
            .post(format!("{}/api/challenge", self.config.url))
            .json(&serde_json::json!({ "public_key": public_key }))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(text));
        }

        resp.json::<PowChallenge>()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))
    }

    /// Full auth flow: challenge → PoW solve → auth → store token.
    ///
    /// On success the token is stored in `self.config.token` and returned.
    /// Persist [`Self::config()`] to storage afterwards.
    pub async fn authenticate(
        &mut self,
        passphrase: &str,
        public_key: &str,
        device_name: &str,
    ) -> Result<AuthResponse, SyncError> {
        let challenge = self.request_challenge(public_key).await?;
        let counter = solve_pow(&challenge.nonce, challenge.difficulty);

        let resp = self
            .http
            .post(format!("{}/api/auth", self.config.url))
            .json(&serde_json::json!({
                "public_key": public_key,
                "nonce": challenge.nonce,
                "counter": counter,
                "passphrase": passphrase,
                "device_name": device_name,
            }))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::AuthFailed(text));
        }

        let auth_resp = resp
            .json::<AuthResponse>()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))?;

        self.config.token = Some(auth_resp.token.clone());
        Ok(auth_resp)
    }

    /// `POST /api/sync/push` — push an encrypted blob.
    ///
    /// Encodes `encrypted_data` as base64 and sends it in the JSON body.
    /// Returns the new sequence number assigned by the server.
    pub async fn push(&self, encrypted_data: &[u8]) -> Result<u64, SyncError> {
        let token = self
            .config
            .token
            .as_deref()
            .ok_or_else(|| SyncError::AuthFailed("not authenticated".into()))?;

        let b64 = base64::engine::general_purpose::STANDARD.encode(encrypted_data);

        let resp = self
            .http
            .post(format!("{}/api/sync/push", self.config.url))
            .bearer_auth(token)
            .json(&serde_json::json!({ "encrypted_blob": b64 }))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::AuthFailed("token expired or revoked".into()));
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(text));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))?;

        Ok(body
            .get("sequence")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0))
    }

    /// `GET /api/sync/pull?since=<seq>` — pull all blobs after `since_sequence`.
    pub async fn pull(&self, since_sequence: u64) -> Result<Vec<SyncBlob>, SyncError> {
        let token = self
            .config
            .token
            .as_deref()
            .ok_or_else(|| SyncError::AuthFailed("not authenticated".into()))?;

        let resp = self
            .http
            .get(format!(
                "{}/api/sync/pull?since={since_sequence}",
                self.config.url
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::AuthFailed("token expired or revoked".into()));
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(text));
        }

        resp.json::<Vec<SyncBlob>>()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))
    }

    /// `GET /api/sync/status` — account info and latest sequence number.
    pub async fn status(&self) -> Result<SyncStatus, SyncError> {
        let token = self
            .config
            .token
            .as_deref()
            .ok_or_else(|| SyncError::AuthFailed("not authenticated".into()))?;

        let resp = self
            .http
            .get(format!("{}/api/sync/status", self.config.url))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SyncError::AuthFailed("token expired or revoked".into()));
        }
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(text));
        }

        resp.json::<SyncStatus>()
            .await
            .map_err(|e| SyncError::Protocol(e.to_string()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pow_difficulty_all_zeros() {
        let zeros = [0u8; 32];
        assert!(check_pow_difficulty(&zeros, 16));
        assert!(check_pow_difficulty(&zeros, 32));
    }

    #[test]
    fn test_pow_difficulty_partial_byte() {
        let mut hash = [0u8; 32];
        hash[1] = 0x80_u8; // 10000000 — 9th bit is 1
        assert!(check_pow_difficulty(&hash, 8)); // first 8 bits zero → ok
        assert!(!check_pow_difficulty(&hash, 9)); // bit 9 is 1 → fail
    }

    #[test]
    fn test_solve_pow_low_difficulty() {
        let nonce = "test-nonce-abc123";
        let difficulty = 4;
        let counter = solve_pow(nonce, difficulty);

        let input = format!("{nonce}{counter}");
        let hash = Sha256::digest(input.as_bytes());
        assert!(check_pow_difficulty(&hash, difficulty));
    }

    #[test]
    fn test_solve_pow_zero_difficulty() {
        assert_eq!(solve_pow("anything", 0), 0);
    }

    #[test]
    fn test_sync_blob_decode() {
        use base64::Engine as _;
        let data = b"hello world";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let blob = SyncBlob {
            sequence: 1,
            encrypted_blob: encoded,
            pushed_at: "2026-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(blob.decode_data().unwrap(), data.as_slice());
    }
}
