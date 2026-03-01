//! Backup server sync client for Poly.
//!
//! Connects to one or more Poly backup servers to synchronize
//! encrypted settings. All data is encrypted client-side before
//! being sent to the server.
//!
//! ## Auth Flow
//! 1. Request PoW challenge from server
//! 2. Solve PoW challenge
//! 3. Submit solution + server passphrase
//! 4. Receive long session token
//! 5. Use token for sync operations

// DECISION(D10): PoW challenge + server passphrase + long tokens + device tracking.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Backup server connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupServerConfig {
    /// Server URL (e.g., "<http://localhost:3000>").
    pub url: String,
    /// Display name for this server.
    pub name: String,
    /// Stored session token (if authenticated).
    pub token: Option<String>,
    /// Last sync sequence number.
    pub last_sequence: u64,
}

/// PoW challenge from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowChallenge {
    /// Challenge ID.
    pub id: String,
    /// Challenge data to hash.
    pub data: String,
    /// Required number of leading zero bits.
    pub difficulty: u32,
}

/// PoW solution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowSolution {
    /// Challenge ID.
    pub challenge_id: String,
    /// The nonce that produces a valid hash.
    pub nonce: u64,
}

/// Auth request to the backup server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    /// PoW solution.
    pub pow_solution: PowSolution,
    /// Server-wide passphrase.
    pub passphrase: String,
    /// User's Ed25519 public key (hex-encoded).
    pub public_key: String,
    /// Device info for token tracking.
    pub device_info: DeviceInfo,
}

/// Device information for session tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device name.
    pub name: String,
    /// Platform (desktop, mobile, web).
    pub platform: String,
    /// Client version.
    pub version: String,
}

/// Auth response from the backup server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    /// Session token.
    pub token: String,
    /// Token expiry timestamp.
    pub expires_at: Option<String>,
}

/// Encrypted sync blob (what gets pushed/pulled).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBlob {
    /// Sequence number for ordering.
    pub sequence: u64,
    /// Encrypted data (opaque to the server).
    pub data: Vec<u8>,
    /// Timestamp of this sync entry.
    pub timestamp: String,
}

/// Errors from backup sync operations.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// Network/HTTP error.
    #[error("network error: {0}")]
    Network(String),

    /// Authentication failed.
    #[error("auth failed: {0}")]
    AuthFailed(String),

    /// Server returned an error.
    #[error("server error: {0}")]
    Server(String),

    /// PoW challenge failed.
    #[error("PoW challenge failed: {0}")]
    PowFailed(String),
}

/// Solve a PoW challenge by finding a nonce that produces a hash
/// with the required number of leading zero bits.
pub fn solve_pow(challenge: &PowChallenge) -> PowSolution {
    let mut nonce: u64 = 0;
    loop {
        let mut hasher = Sha256::new();
        hasher.update(challenge.data.as_bytes());
        hasher.update(nonce.to_le_bytes());
        let hash = hasher.finalize();

        if check_pow_difficulty(&hash, challenge.difficulty) {
            return PowSolution {
                challenge_id: challenge.id.clone(),
                nonce,
            };
        }
        nonce += 1;
    }
}

/// Check if a hash meets the required difficulty (leading zero bits).
fn check_pow_difficulty(hash: &[u8], difficulty: u32) -> bool {
    let mut bits_checked = 0u32;
    for &byte in hash {
        if bits_checked + 8 <= difficulty {
            if byte != 0 {
                return false;
            }
            bits_checked += 8;
        } else {
            let remaining = difficulty - bits_checked;
            let mask = 0xFF << (8 - remaining);
            return (byte & mask) == 0;
        }
        if bits_checked >= difficulty {
            return true;
        }
    }
    true
}

/// Backup sync client that communicates with a Poly backup server.
pub struct SyncClient {
    http: reqwest::Client,
    config: BackupServerConfig,
}

impl SyncClient {
    /// Create a new sync client for a backup server.
    pub fn new(config: BackupServerConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    /// Get the server URL.
    pub fn url(&self) -> &str {
        &self.config.url
    }

    /// Request a PoW challenge from the server.
    pub async fn request_challenge(&self) -> Result<PowChallenge, SyncError> {
        let resp = self
            .http
            .get(format!("{}/api/challenge", self.config.url))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        resp.json::<PowChallenge>()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))
    }

    /// Authenticate with the server (solve PoW + passphrase).
    pub async fn authenticate(
        &mut self,
        passphrase: &str,
        public_key: &str,
        device_info: DeviceInfo,
    ) -> Result<AuthResponse, SyncError> {
        // 1. Get challenge
        let challenge = self.request_challenge().await?;

        // 2. Solve PoW
        let solution = solve_pow(&challenge);

        // 3. Submit auth
        let auth_req = AuthRequest {
            pow_solution: solution,
            passphrase: passphrase.to_string(),
            public_key: public_key.to_string(),
            device_info,
        };

        let resp = self
            .http
            .post(format!("{}/api/auth", self.config.url))
            .json(&auth_req)
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
            .map_err(|e| SyncError::Network(e.to_string()))?;

        // Store the token
        self.config.token = Some(auth_resp.token.clone());

        Ok(auth_resp)
    }

    /// Push encrypted settings blob to the server.
    pub async fn push(&self, data: Vec<u8>) -> Result<u64, SyncError> {
        let token = self
            .config
            .token
            .as_ref()
            .ok_or_else(|| SyncError::AuthFailed("not authenticated".into()))?;

        let resp = self
            .http
            .post(format!("{}/api/sync/push", self.config.url))
            .bearer_auth(token)
            .json(&serde_json::json!({ "data": data }))
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(text));
        }

        // Return the new sequence number
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;
        Ok(body["sequence"].as_u64().unwrap_or(0))
    }

    /// Pull encrypted settings changes since a sequence number.
    pub async fn pull(&self, since_sequence: u64) -> Result<Vec<SyncBlob>, SyncError> {
        let token = self
            .config
            .token
            .as_ref()
            .ok_or_else(|| SyncError::AuthFailed("not authenticated".into()))?;

        let resp = self
            .http
            .get(format!(
                "{}/api/sync/pull?since={}",
                self.config.url, since_sequence
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(SyncError::Server(text));
        }

        resp.json::<Vec<SyncBlob>>()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pow_difficulty_check() {
        // All zeros pass any difficulty
        let zeros = [0u8; 32];
        assert!(check_pow_difficulty(&zeros, 16));
        assert!(check_pow_difficulty(&zeros, 32));

        // First byte 0, second byte non-zero
        let mut hash = [0u8; 32];
        hash[1] = 0x80; // 10000000
        assert!(check_pow_difficulty(&hash, 8)); // Need 8 zero bits → first byte is 0 ✓
        assert!(check_pow_difficulty(&hash, 9)); // Need 9 zero bits → first byte 0 + second byte starts with 1 → fail
        assert!(!check_pow_difficulty(&hash, 10));
    }

    #[test]
    fn test_solve_pow() {
        let challenge = PowChallenge {
            id: "test".to_string(),
            data: "test-challenge-data".to_string(),
            difficulty: 8, // Easy difficulty for testing
        };

        let solution = solve_pow(&challenge);
        assert_eq!(solution.challenge_id, "test");

        // Verify the solution
        let mut hasher = Sha256::new();
        hasher.update(challenge.data.as_bytes());
        hasher.update(solution.nonce.to_le_bytes());
        let hash = hasher.finalize();
        assert!(check_pow_difficulty(&hash, challenge.difficulty));
    }
}
