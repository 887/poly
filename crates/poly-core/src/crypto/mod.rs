//! Cryptography module for Poly.
//!
//! Handles Ed25519 identity key generation, X25519 key derivation,
//! BIP39 mnemonic recovery phrases, and symmetric encryption for
//! backup server data.
//!
//! ## Key Model (Session Messenger-inspired)
//! 1. Generate Ed25519 signing keypair (identity)
//! 2. Derive X25519 Diffie-Hellman key from Ed25519
//! 3. Public key = Account ID (hex-encoded)
//! 4. Private key → BIP39 mnemonic (Recovery Phrase)

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// An identity keypair for a Poly user.
#[derive(Clone)]
pub struct Identity {
    /// The Ed25519 signing (private) key.
    signing_key: SigningKey,
}

/// Public identity information (safe to share).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicIdentity {
    /// The Account ID — hex-encoded Ed25519 public key.
    pub account_id: String,
    /// Raw public key bytes.
    pub public_key_bytes: Vec<u8>,
}

impl Identity {
    /// Generate a new random identity keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Restore an identity from Ed25519 private key bytes.
    pub fn from_private_key_bytes(bytes: &[u8; 32]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        Self { signing_key }
    }

    /// Restore an identity from a BIP39 mnemonic phrase.
    pub fn from_mnemonic(phrase: &str) -> Result<Self, CryptoError> {
        let mnemonic = bip39::Mnemonic::parse(phrase)
            .map_err(|e| CryptoError::InvalidMnemonic(e.to_string()))?;

        // Use the entropy from the mnemonic as the private key seed
        let entropy = mnemonic.to_entropy();
        if entropy.len() < 32 {
            // Hash the entropy to get 32 bytes if needed
            let mut hasher = Sha256::new();
            hasher.update(&entropy);
            let hash = hasher.finalize();
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&hash);
            Ok(Self::from_private_key_bytes(&bytes))
        } else {
            let bytes = entropy
                .get(..32)
                .and_then(|s| <[u8; 32]>::try_from(s).ok())
                .unwrap_or([0u8; 32]);
            Ok(Self::from_private_key_bytes(&bytes))
        }
    }

    /// Get the Ed25519 public (verifying) key.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Get the public identity (Account ID).
    pub fn public_identity(&self) -> PublicIdentity {
        let vk = self.verifying_key();
        let bytes = vk.to_bytes();
        PublicIdentity {
            account_id: hex::encode(bytes),
            public_key_bytes: bytes.to_vec(),
        }
    }

    /// Get the raw private key bytes.
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Generate a BIP39 mnemonic recovery phrase from the private key.
    ///
    /// Uses 256-bit entropy (24 words) for maximum security.
    pub fn to_mnemonic(&self) -> Result<String, CryptoError> {
        let bytes = self.private_key_bytes();
        let mnemonic = bip39::Mnemonic::from_entropy(&bytes)
            .map_err(|e| CryptoError::MnemonicGeneration(e.to_string()))?;
        Ok(mnemonic.to_string())
    }

    /// Derive an X25519 static secret from the Ed25519 key.
    ///
    /// Used for symmetric key derivation for backup encryption.
    pub fn derive_x25519_secret(&self) -> x25519_dalek::StaticSecret {
        // Hash the Ed25519 private key to get X25519-compatible bytes
        let mut hasher = Sha256::new();
        hasher.update(self.private_key_bytes());
        hasher.update(b"poly-x25519-derivation");
        let hash = hasher.finalize();
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&hash);
        x25519_dalek::StaticSecret::from(key_bytes)
    }

    /// Derive a symmetric encryption key for backup server data.
    ///
    /// Derives from the X25519 secret using HKDF-style derivation.
    pub fn derive_backup_key(&self) -> [u8; 32] {
        let secret = self.derive_x25519_secret();
        let public = x25519_dalek::PublicKey::from(&secret);

        // Self-DH + domain separation for backup key
        let mut hasher = Sha256::new();
        hasher.update(secret.diffie_hellman(&public).as_bytes());
        hasher.update(b"poly-backup-encryption-key-v1");
        let hash = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&hash);
        key
    }
}

/// Errors from cryptographic operations.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// Invalid mnemonic phrase.
    #[error("invalid mnemonic: {0}")]
    InvalidMnemonic(String),

    /// Failed to generate mnemonic.
    #[error("mnemonic generation failed: {0}")]
    MnemonicGeneration(String),

    /// Encryption/decryption error.
    #[error("encryption error: {0}")]
    Encryption(String),
}

/// Encrypt data with a symmetric key using simple XOR + HMAC.
///
/// TODO(phase-2.4.4.5): Replace with XSalsa20-Poly1305 or AES-256-GCM.
/// This is a placeholder that just base64-encodes for now.
pub fn encrypt(data: &[u8], _key: &[u8; 32]) -> Vec<u8> {
    // Placeholder: base64 encode (NOT REAL ENCRYPTION)
    // TODO(phase-2.4.4.5): Implement real XSalsa20-Poly1305
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .encode(data)
        .into_bytes()
}

/// Decrypt data with a symmetric key.
///
/// TODO(phase-2.4.4.5): Replace with XSalsa20-Poly1305 or AES-256-GCM.
pub fn decrypt(data: &[u8], _key: &[u8; 32]) -> Result<Vec<u8>, CryptoError> {
    // Placeholder: base64 decode (NOT REAL DECRYPTION)
    // TODO(phase-2.4.4.5): Implement real XSalsa20-Poly1305
    use base64::Engine;
    let s = std::str::from_utf8(data).map_err(|e| CryptoError::Encryption(e.to_string()))?;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| CryptoError::Encryption(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_generation() {
        let identity = Identity::generate();
        let public = identity.public_identity();
        assert!(!public.account_id.is_empty());
        assert_eq!(public.public_key_bytes.len(), 32);
    }

    #[test]
    fn test_mnemonic_roundtrip() {
        let identity = Identity::generate();
        let phrase = identity.to_mnemonic().unwrap();
        let words: Vec<&str> = phrase.split_whitespace().collect();
        assert_eq!(words.len(), 24); // 256-bit entropy = 24 words

        let restored = Identity::from_mnemonic(&phrase).unwrap();
        assert_eq!(
            identity.public_identity().account_id,
            restored.public_identity().account_id
        );
    }

    #[test]
    fn test_backup_key_derivation() {
        let identity = Identity::generate();
        let key1 = identity.derive_backup_key();
        let key2 = identity.derive_backup_key();
        assert_eq!(key1, key2); // Deterministic
        assert_ne!(key1, [0u8; 32]); // Not all zeros
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let identity = Identity::generate();
        let key = identity.derive_backup_key();
        let data = b"Hello, Poly backup!";
        let encrypted = encrypt(data, &key);
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(data.as_slice(), &decrypted);
    }
}
