//! # `secret_seal` — at-rest sealing for credential rows in `poly_kv`
//!
//! Phase C.3 of `docs/plans/plan-host-substrate-capability-gating.md`.
//!
//! ## What this defends
//!
//! `poly_kv` is a plain SQLite table, and the `account_tokens` row holds
//! every backend's OAuth access token, refresh token and expiry as
//! cleartext JSON. Two exposures follow from that:
//!
//! 1. **A future KV escape.** Phase C.1 confines `/host/kv/*` callers to
//!    their own namespace, but that check is code — a bug in it turns
//!    straight into token disclosure. With sealing, a caller that manages
//!    to name `account_tokens` gets an opaque envelope instead.
//! 2. **Anything that reads the file but not the process.** Backup jobs,
//!    sync clients, a `sqlite3` one-liner in a shared screenshare, a
//!    stolen copy of `~/.local/share/poly/storage.sqlite3`.
//!
//! ## What it does *not* defend, stated plainly
//!
//! The key lives in `poly-secret.key` next to the database, mode `0600`.
//! An adversary who can read arbitrary files as the user reads the key as
//! easily as the database, so exposure (2) is only closed against
//! *database-only* copies. Closing it against full filesystem access needs
//! the key in an OS keychain, which is what
//! `plan-host-substrate-capability-gating.md` C.3 asks for; that variant
//! is a third implementation of [`SecretSealer`] and nothing outside this
//! file changes when it lands. Exposure (1) — the one the phase is
//! actually about — is fully closed either way, because a KV caller can
//! never read a file.
//!
//! ## Format
//!
//! `poly-sealed-v1:<base64(nonce‖ciphertext)>` with XChaCha20-Poly1305, a
//! fresh 24-byte nonce per write. [`SecretSealer::unseal`] passes any
//! value *without* the prefix through untouched, so databases written
//! before this landed keep working and get sealed on their next write.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chacha20poly1305::aead::{Aead as _, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use thiserror::Error;

/// Envelope marker. Values not starting with this are legacy cleartext.
pub const SEAL_PREFIX: &str = "poly-sealed-v1:";

/// Additional authenticated data — binds an envelope to this format so a
/// ciphertext from some other Poly subsystem cannot be replayed here.
const SEAL_AAD: &[u8] = b"poly-host-kv-seal-v1";

/// Nonce width for XChaCha20-Poly1305.
const NONCE_LEN: usize = 24;

/// Filename of the key material, resolved next to the SQLite file.
pub const KEY_FILE_NAME: &str = "poly-secret.key";

/// Failure modes of the sealing boundary.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SealError {
    /// The key file could not be read or created.
    #[error("secret key unavailable: {0}")]
    Key(String),
    /// Encryption failed.
    #[error("seal failed: {0}")]
    Seal(String),
    /// Decryption or envelope parsing failed.
    #[error("unseal failed: {0}")]
    Unseal(String),
}

/// The at-rest confidentiality boundary for credential-bearing KV rows.
///
/// A trait rather than a concrete type so tests get a real, deterministic
/// implementation without touching the filesystem (SOLID item 7) and so a
/// keychain-backed variant is an added file rather than an edit.
pub trait SecretSealer: Send + Sync {
    /// Wrap `plaintext` for storage.
    ///
    /// # Errors
    /// [`SealError::Seal`] if the AEAD refuses.
    fn seal(&self, plaintext: &str) -> Result<String, SealError>;

    /// Unwrap a stored value. A value without [`SEAL_PREFIX`] is returned
    /// verbatim (legacy cleartext).
    ///
    /// # Errors
    /// [`SealError::Unseal`] on a malformed or unauthentic envelope.
    fn unseal(&self, stored: &str) -> Result<String, SealError>;
}

/// XChaCha20-Poly1305 sealer. Construct with [`XChaChaSealer::in_memory`]
/// (tests) or [`XChaChaSealer::from_key_file`] (shells).
pub struct XChaChaSealer {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for XChaChaSealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("XChaChaSealer { key: <redacted> }")
    }
}

impl XChaChaSealer {
    /// Random key held only in memory. Sealed values do not survive the
    /// process — exactly what a test wants.
    ///
    /// # Errors
    /// [`SealError::Key`] if the OS CSPRNG is unavailable.
    pub fn in_memory() -> Result<Self, SealError> {
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key).map_err(|e| SealError::Key(e.to_string()))?;
        Self::from_key_bytes(&key)
    }

    /// Load the key at `path`, creating a fresh `0600` one if absent.
    ///
    /// # Errors
    /// [`SealError::Key`] on any filesystem or entropy failure, or if the
    /// existing file is not exactly 32 bytes of hex.
    pub fn from_key_file(path: &Path) -> Result<Self, SealError> {
        let key = match std::fs::read_to_string(path) {
            Ok(text) => decode_key(text.trim())?,
            Err(_missing) => create_key_file(path)?,
        };
        Self::from_key_bytes(&key)
    }

    fn from_key_bytes(key: &[u8; 32]) -> Result<Self, SealError> {
        use chacha20poly1305::KeyInit as _;
        let cipher = XChaCha20Poly1305::new_from_slice(key)
            .map_err(|e| SealError::Key(format!("bad key length: {e}")))?;
        Ok(Self { cipher })
    }

    /// Conventional location of the key for a database at `db_path`.
    #[must_use]
    pub fn key_path_for(db_path: &Path) -> PathBuf {
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(KEY_FILE_NAME)
    }
}

fn decode_key(text: &str) -> Result<[u8; 32], SealError> {
    let bytes = hex::decode(text).map_err(|e| SealError::Key(format!("key is not hex: {e}")))?;
    let sized: [u8; 32] = bytes
        .try_into()
        .map_err(|_wrong_len| SealError::Key("key must be 32 bytes".to_string()))?;
    Ok(sized)
}

fn create_key_file(path: &Path) -> Result<[u8; 32], SealError> {
    use std::io::Write as _;

    let mut key = [0_u8; 32];
    getrandom::fill(&mut key).map_err(|e| SealError::Key(e.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SealError::Key(e.to_string()))?;
    }
    let mut opts = std::fs::OpenOptions::new();
    let _w = opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let _m = opts.mode(0o600);
    }
    match opts.open(path) {
        Ok(mut file) => {
            file.write_all(hex::encode(key).as_bytes())
                .map_err(|e| SealError::Key(e.to_string()))?;
            Ok(key)
        }
        // Lost a create race with another shell — read the winner's key.
        Err(_exists) => {
            let text = std::fs::read_to_string(path).map_err(|e| SealError::Key(e.to_string()))?;
            decode_key(text.trim())
        }
    }
}

impl SecretSealer for XChaChaSealer {
    fn seal(&self, plaintext: &str) -> Result<String, SealError> {
        let mut nonce_bytes = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes).map_err(|e| SealError::Seal(e.to_string()))?;
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: SEAL_AAD,
                },
            )
            .map_err(|e| SealError::Seal(e.to_string()))?;
        let mut envelope = Vec::with_capacity(NONCE_LEN.saturating_add(ciphertext.len()));
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&ciphertext);
        Ok(format!(
            "{SEAL_PREFIX}{}",
            base64::engine::general_purpose::STANDARD.encode(&envelope)
        ))
    }

    fn unseal(&self, stored: &str) -> Result<String, SealError> {
        let Some(b64) = stored.strip_prefix(SEAL_PREFIX) else {
            return Ok(stored.to_string());
        };
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|e| SealError::Unseal(format!("base64: {e}")))?;
        let nonce_bytes = raw
            .get(..NONCE_LEN)
            .ok_or_else(|| SealError::Unseal("envelope shorter than nonce".to_string()))?;
        let ciphertext = raw
            .get(NONCE_LEN..)
            .ok_or_else(|| SealError::Unseal("envelope has no ciphertext".to_string()))?;
        let plaintext = self
            .cipher
            .decrypt(
                XNonce::from_slice(nonce_bytes),
                Payload {
                    msg: ciphertext,
                    aad: SEAL_AAD,
                },
            )
            .map_err(|e| SealError::Unseal(e.to_string()))?;
        String::from_utf8(plaintext).map_err(|e| SealError::Unseal(e.to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let sealer = XChaChaSealer::in_memory().unwrap();
        let sealed = sealer.seal(r#"[{"token":"abc"}]"#).unwrap();
        assert!(sealed.starts_with(SEAL_PREFIX));
        assert!(!sealed.contains("abc"), "plaintext leaked: {sealed}");
        assert_eq!(sealer.unseal(&sealed).unwrap(), r#"[{"token":"abc"}]"#);
    }

    #[test]
    fn nonce_is_fresh_per_write() {
        let sealer = XChaChaSealer::in_memory().unwrap();
        assert_ne!(sealer.seal("same").unwrap(), sealer.seal("same").unwrap());
    }

    #[test]
    fn legacy_cleartext_passes_through() {
        let sealer = XChaChaSealer::in_memory().unwrap();
        assert_eq!(sealer.unseal(r#"{"a":1}"#).unwrap(), r#"{"a":1}"#);
    }

    #[test]
    fn another_key_cannot_unseal() {
        let a = XChaChaSealer::in_memory().unwrap();
        let b = XChaChaSealer::in_memory().unwrap();
        let sealed = a.seal("secret").unwrap();
        assert!(matches!(b.unseal(&sealed), Err(SealError::Unseal(_))));
    }

    #[test]
    fn tampered_envelope_is_rejected() {
        let sealer = XChaChaSealer::in_memory().unwrap();
        let sealed = sealer.seal("secret").unwrap();
        let mut bytes = base64::engine::general_purpose::STANDARD
            .decode(sealed.trim_start_matches(SEAL_PREFIX))
            .unwrap();
        let last = bytes.len().saturating_sub(1);
        if let Some(b) = bytes.get_mut(last) {
            *b ^= 0xFF;
        }
        let tampered = format!(
            "{SEAL_PREFIX}{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        assert!(matches!(sealer.unseal(&tampered), Err(SealError::Unseal(_))));
    }

    #[test]
    fn key_file_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = XChaChaSealer::key_path_for(&dir.path().join("storage.sqlite3"));
        let a = XChaChaSealer::from_key_file(&key_path).unwrap();
        let sealed = a.seal("persisted").unwrap();
        let b = XChaChaSealer::from_key_file(&key_path).unwrap();
        assert_eq!(b.unseal(&sealed).unwrap(), "persisted");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&key_path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "key file must be owner-only");
        }
    }
}
