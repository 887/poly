//! Microsoft Graph rich-notification encryption — Phase D of
//! `docs/plans/plan-teams-graph-subscriptions.md`.
//!
//! ## Why this module
//!
//! When a Graph subscription requests `resourceData` (the full changed
//! resource embedded in every notification, not just a URL to fetch),
//! Microsoft requires a per-tenant RSA public certificate up front and
//! encrypts every `encryptedContent` payload with a hybrid scheme:
//!
//! 1. Microsoft generates a fresh AES-256 key per notification.
//! 2. The resource JSON is AES-256-CBC encrypted with PKCS#7 padding,
//!    IV = first 16 bytes of the AES key.
//! 3. The AES key is wrapped with **RSA-OAEP-SHA256** using the public
//!    key the client posted in `encryptionCertificate`.
//! 4. A separate HMAC-SHA256 over the ciphertext is included for
//!    integrity verification.
//!
//! Spec: <https://learn.microsoft.com/graph/webhooks-with-resource-data#decrypting-resource-data-from-change-notifications>.
//!
//! ## Key storage
//!
//! [`TeamsKeyStore`] generates a 2048-bit RSA keypair and returns the
//! private key as PEM (PKCS#8) + the public-key DER bytes base64-encoded
//! as a `encryptionCertificate` value Microsoft accepts.
//!
//! The caller chooses persistence. The default [`InMemoryKeyStore`]
//! lives only for the process lifetime — good for tests, NOT suitable
//! for production. Production deployments should wrap the private-key
//! PEM in OS keychain storage (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service via `secret-service` / `keyring`
//! crates) before persisting; see the `KEY_STORAGE_FOLLOWUP` doc
//! comment on [`InMemoryKeyStore`] for the migration path.

#![cfg(all(not(target_arch = "wasm32"), feature = "teams-webhook"))]

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use std::sync::{Arc, RwLock};

use rsa::{
    Oaep, RsaPrivateKey, RsaPublicKey,
    pkcs1::EncodeRsaPublicKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding},
};

/// Errors surfaced from [`TeamsKeyStore`] / [`decrypt_resource_data`].
#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    /// RSA keypair generation / serialization failed.
    #[error("RSA key error: {0}")]
    Rsa(String),
    /// Base64 decoding of one of the wire fields failed.
    #[error("base64 decode: {0}")]
    Base64(String),
    /// AES symmetric decrypt failed (wrong key / tampered ciphertext).
    #[error("AES decrypt failed: {0}")]
    Aes(String),
    /// HMAC integrity check failed — payload was tampered with.
    #[error("HMAC verification failed")]
    HmacMismatch,
    /// PKCS#8 PEM parse failed.
    #[error("PKCS#8 PEM parse: {0}")]
    Pem(String),
    /// Key store is empty (no keypair generated yet).
    #[error("no keypair available — call generate_keypair() first")]
    NoKeypair,
}

/// Per-tenant RSA keypair store.
///
/// Cheap to clone — internal state is `Arc<RwLock<_>>`. The public-cert
/// accessor returns a base64 DER string in the exact shape Microsoft's
/// `Subscription.encryptionCertificate` field requires.
#[derive(Clone)]
pub struct TeamsKeyStore {
    inner: Arc<RwLock<Option<Keypair>>>,
    cert_id: String,
}

struct Keypair {
    /// PKCS#8 PEM serialization of the private key — the bytes a
    /// caller would persist to disk / keychain.
    private_pem: String,
    /// PKCS#1 DER serialization of the public key — what we base64-encode
    /// for the `encryptionCertificate` field.
    public_der: Vec<u8>,
}

impl std::fmt::Debug for TeamsKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeamsKeyStore")
            .field("cert_id", &self.cert_id)
            .field("has_keypair", &self.inner.read().ok().is_some_and(|g| g.is_some()))
            .finish()
    }
}

impl TeamsKeyStore {
    /// Construct an empty store with a stable certificate id (echoed
    /// back by Microsoft in every encrypted payload so the receiver
    /// knows which private key to use).
    #[must_use]
    pub fn new(cert_id: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            cert_id: cert_id.into(),
        }
    }

    /// Certificate identifier as a public field — included in
    /// `Subscription.encryptionCertificateId`.
    #[must_use]
    pub fn certificate_id(&self) -> &str {
        &self.cert_id
    }

    /// Generate a fresh 2048-bit RSA keypair, replacing any existing
    /// one. Returns the base64-encoded public DER (the
    /// `encryptionCertificate` wire value).
    pub fn generate_keypair(&self) -> Result<String, EncryptionError> {
        let mut rng = rand08::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|e| EncryptionError::Rsa(e.to_string()))?;
        let public = RsaPublicKey::from(&private);

        let private_pem = private
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| EncryptionError::Rsa(e.to_string()))?
            .to_string();
        let public_der = public
            .to_pkcs1_der()
            .map_err(|e| EncryptionError::Rsa(e.to_string()))?
            .as_bytes()
            .to_vec();
        let public_b64 = B64.encode(&public_der);

        let mut guard = self
            .inner
            .write()
            .map_err(|_e| EncryptionError::Rsa("keystore lock poisoned".into()))?;
        *guard = Some(Keypair { private_pem, public_der });
        drop(guard);
        Ok(public_b64)
    }

    /// Restore a keypair from previously-persisted PKCS#8 PEM. Use this
    /// on shell startup to re-load the same keypair across restarts so
    /// existing Graph subscriptions remain decryptable.
    pub fn load_private_pem(&self, pem: &str) -> Result<(), EncryptionError> {
        let private = RsaPrivateKey::from_pkcs8_pem(pem)
            .map_err(|e| EncryptionError::Pem(e.to_string()))?;
        let public = RsaPublicKey::from(&private);
        let public_der = public
            .to_pkcs1_der()
            .map_err(|e| EncryptionError::Rsa(e.to_string()))?
            .as_bytes()
            .to_vec();
        let mut guard = self
            .inner
            .write()
            .map_err(|_e| EncryptionError::Rsa("keystore lock poisoned".into()))?;
        *guard = Some(Keypair { private_pem: pem.to_string(), public_der });
        drop(guard);
        Ok(())
    }

    /// Base64-encoded PKCS#1 DER of the public key — exactly the
    /// string Graph expects in `Subscription.encryptionCertificate`.
    pub fn public_certificate_b64(&self) -> Result<String, EncryptionError> {
        let guard = self
            .inner
            .read()
            .map_err(|_e| EncryptionError::Rsa("keystore lock poisoned".into()))?;
        let result = guard
            .as_ref()
            .map(|kp| B64.encode(&kp.public_der))
            .ok_or(EncryptionError::NoKeypair);
        drop(guard);
        result
    }

    /// PKCS#8 PEM of the private key — caller's responsibility to
    /// persist securely. Returned exactly once to keep the in-memory
    /// guard simple; caller stores the result.
    pub fn private_pem(&self) -> Result<String, EncryptionError> {
        let guard = self
            .inner
            .read()
            .map_err(|_e| EncryptionError::Rsa("keystore lock poisoned".into()))?;
        let result = guard
            .as_ref()
            .map(|kp| kp.private_pem.clone())
            .ok_or(EncryptionError::NoKeypair);
        drop(guard);
        result
    }

    /// Decrypt an `encryptedContent` block from a Graph notification.
    /// Caller passes the parsed JSON object; this returns the decrypted
    /// resource bytes (typically JSON).
    pub fn decrypt_resource_data(
        &self,
        encrypted: &EncryptedContent,
    ) -> Result<Vec<u8>, EncryptionError> {
        let guard = self
            .inner
            .read()
            .map_err(|_e| EncryptionError::Rsa("keystore lock poisoned".into()))?;
        let kp = guard.as_ref().ok_or(EncryptionError::NoKeypair)?;
        let pem = kp.private_pem.clone();
        drop(guard);
        decrypt_resource_data(encrypted, &pem)
    }
}

// `InMemoryKeyStore` doc-marker — historically these were separate
// types; we collapsed to a single `TeamsKeyStore` whose persistence
// strategy is decided by the caller. The doc-comment below documents
// the keychain follow-up.

/// KEY_STORAGE_FOLLOWUP: production deployments should wrap
/// [`TeamsKeyStore::private_pem`] output through one of:
///
/// - `keyring` crate (macOS Keychain / Windows Credential Manager /
///   Linux Secret Service) — single API, three platforms.
/// - `secret-service` crate — Linux-only, more control.
///
/// The Phase D implementation lands keypair-generation and decrypt
/// plumbing; the keychain wrap is a small follow-up (~50 LoC + per-OS
/// CI gates) deferred to the operator who first ships against
/// production Graph.
pub const KEY_STORAGE_FOLLOWUP: &str = "see TeamsKeyStore::private_pem doc";

/// Wire shape for the `encryptedContent` block on a Graph
/// change-notification. All four fields are base64-encoded.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EncryptedContent {
    /// AES-256-CBC ciphertext over the resource JSON (PKCS#7 padded).
    pub data: String,
    /// RSA-OAEP-SHA256 wrapped AES-256 symmetric key.
    #[serde(rename = "dataKey")]
    pub data_key: String,
    /// HMAC-SHA256 over the `data` bytes, keyed by the wrapped AES key.
    #[serde(rename = "dataSignature")]
    pub data_signature: String,
    /// Echo of `encryptionCertificateId` from the subscription
    /// request — used by the receiver to pick the correct private key
    /// if multiple subscriptions / tenants are multiplexed.
    #[serde(default, rename = "encryptionCertificateId")]
    pub encryption_certificate_id: String,
}

/// Free function form of [`TeamsKeyStore::decrypt_resource_data`] —
/// useful for tests / one-shot decrypt without a long-lived store.
pub fn decrypt_resource_data(
    encrypted: &EncryptedContent,
    private_pem: &str,
) -> Result<Vec<u8>, EncryptionError> {
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
    use hmac::Mac;
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
    type HmacSha256 = hmac::Hmac<sha2::Sha256>;

    let private = RsaPrivateKey::from_pkcs8_pem(private_pem)
        .map_err(|e| EncryptionError::Pem(e.to_string()))?;

    let wrapped_key = B64
        .decode(&encrypted.data_key)
        .map_err(|e| EncryptionError::Base64(format!("dataKey: {e}")))?;
    let ciphertext = B64
        .decode(&encrypted.data)
        .map_err(|e| EncryptionError::Base64(format!("data: {e}")))?;
    let signature = B64
        .decode(&encrypted.data_signature)
        .map_err(|e| EncryptionError::Base64(format!("dataSignature: {e}")))?;

    // RSA-OAEP-SHA256 unwrap.
    let aes_key = private
        .decrypt(Oaep::new::<sha2::Sha256>(), &wrapped_key)
        .map_err(|e| EncryptionError::Rsa(format!("OAEP unwrap: {e}")))?;
    if aes_key.len() < 32 {
        return Err(EncryptionError::Rsa(format!(
            "unwrapped key too short — expected ≥32 bytes, got {}",
            aes_key.len()
        )));
    }

    // HMAC-SHA256 over the ciphertext, keyed by the AES key bytes.
    let mut mac = HmacSha256::new_from_slice(&aes_key)
        .map_err(|e| EncryptionError::Aes(format!("HMAC keying: {e}")))?;
    mac.update(&ciphertext);
    mac.verify_slice(&signature)
        .map_err(|_e| EncryptionError::HmacMismatch)?;

    // AES-256-CBC decrypt. IV is the first 16 bytes of the AES key per Graph's spec.
    // lint-allow-unused: aes_key.len() >= 32 checked above, so [..16] is in bounds
    #[allow(clippy::indexing_slicing)]
    let iv: [u8; 16] = aes_key[..16]
        .try_into()
        .map_err(|_e| EncryptionError::Aes("IV slice".into()))?;
    // lint-allow-unused: aes_key.len() >= 32 checked above, so [..32] is in bounds
    #[allow(clippy::indexing_slicing)]
    let key: [u8; 32] = aes_key[..32]
        .try_into()
        .map_err(|_e| EncryptionError::Aes("key slice".into()))?;
    let plaintext = Aes256CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
        .map_err(|e| EncryptionError::Aes(format!("CBC decrypt: {e}")))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use rsa::{Oaep, pkcs8::DecodePublicKey};

    /// Round-trip: encrypt a payload with the public key the way Graph
    /// would, then verify our decrypt path recovers it.
    fn graph_encrypt(
        plaintext: &[u8],
        public_cert_b64: &str,
    ) -> EncryptedContent {
        use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
        use hmac::Mac;
        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
        type HmacSha256 = hmac::Hmac<sha2::Sha256>;

        // Fresh AES-256 symmetric key — first 16 bytes also double as
        // the CBC IV per Graph's spec.
        let aes_key: [u8; 32] = rand08::random();
        let iv: [u8; 16] = aes_key[..16].try_into().unwrap();

        // AES-256-CBC PKCS7 encrypt.
        let ciphertext = Aes256CbcEnc::new(&aes_key.into(), &iv.into())
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext);

        // HMAC-SHA256 over the ciphertext.
        let mut mac = HmacSha256::new_from_slice(&aes_key).unwrap();
        mac.update(&ciphertext);
        let signature = mac.finalize().into_bytes().to_vec();

        // RSA-OAEP-SHA256 wrap the AES key with the published cert.
        let public_der = B64.decode(public_cert_b64).unwrap();
        // Try PKCS#1 DER first (what we publish), fall back to SPKI.
        let public = rsa::pkcs1::DecodeRsaPublicKey::from_pkcs1_der(&public_der)
            .or_else(|_| RsaPublicKey::from_public_key_der(&public_der))
            .unwrap();
        let mut rng = rand08::thread_rng();
        let wrapped = public
            .encrypt(&mut rng, Oaep::new::<sha2::Sha256>(), &aes_key)
            .unwrap();

        EncryptedContent {
            data: B64.encode(&ciphertext),
            data_key: B64.encode(&wrapped),
            data_signature: B64.encode(&signature),
            encryption_certificate_id: "test-cert".into(),
        }
    }

    #[test]
    fn keystore_generates_and_exposes_certificate() {
        let store = TeamsKeyStore::new("cert-1");
        assert_eq!(store.certificate_id(), "cert-1");
        let cert = store.generate_keypair().unwrap();
        // Public-cert base64 starts long-ish (DER ≥270 bytes for 2048
        // RSA → ≥360 chars base64).
        assert!(cert.len() > 300, "got {} chars", cert.len());
        let public_b64 = store.public_certificate_b64().unwrap();
        assert_eq!(cert, public_b64);
        assert!(store.private_pem().unwrap().starts_with("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn keystore_decrypts_self_round_trip() {
        let store = TeamsKeyStore::new("cert-1");
        let cert = store.generate_keypair().unwrap();

        let payload = b"{\"@odata.type\":\"#microsoft.graph.chatMessage\",\"body\":\"hi\"}";
        let envelope = graph_encrypt(payload, &cert);
        let decrypted = store.decrypt_resource_data(&envelope).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn decrypt_rejects_tampered_ciphertext() {
        let store = TeamsKeyStore::new("cert-1");
        let cert = store.generate_keypair().unwrap();

        let payload = b"original";
        let mut envelope = graph_encrypt(payload, &cert);
        // Flip a byte in the ciphertext — HMAC check should reject.
        let mut bytes = B64.decode(&envelope.data).unwrap();
        bytes[0] ^= 0x01;
        envelope.data = B64.encode(&bytes);

        let err = store.decrypt_resource_data(&envelope).unwrap_err();
        assert!(matches!(err, EncryptionError::HmacMismatch), "got {err:?}");
    }

    #[test]
    fn decrypt_rejects_wrong_private_key() {
        let store_a = TeamsKeyStore::new("cert-a");
        let cert_a = store_a.generate_keypair().unwrap();
        let store_b = TeamsKeyStore::new("cert-b");
        let _cert_b = store_b.generate_keypair().unwrap();

        let envelope = graph_encrypt(b"hello", &cert_a);
        // Wrong store — should fail RSA unwrap.
        let err = store_b.decrypt_resource_data(&envelope).unwrap_err();
        assert!(matches!(err, EncryptionError::Rsa(_)), "got {err:?}");
    }

    #[test]
    fn no_keypair_errors_loudly() {
        let store = TeamsKeyStore::new("cert-1");
        assert!(matches!(
            store.public_certificate_b64(),
            Err(EncryptionError::NoKeypair)
        ));
        assert!(matches!(store.private_pem(), Err(EncryptionError::NoKeypair)));
    }

    #[test]
    fn load_private_pem_round_trip() {
        let store_a = TeamsKeyStore::new("cert-1");
        let cert_a = store_a.generate_keypair().unwrap();
        let pem = store_a.private_pem().unwrap();

        let store_b = TeamsKeyStore::new("cert-1");
        store_b.load_private_pem(&pem).unwrap();
        assert_eq!(store_b.public_certificate_b64().unwrap(), cert_a);

        // And it can decrypt payloads encrypted to store_a's public cert.
        let envelope = graph_encrypt(b"persisted", &cert_a);
        assert_eq!(store_b.decrypt_resource_data(&envelope).unwrap(), b"persisted");
    }

    #[test]
    fn encrypted_content_deserializes_from_graph_payload_shape() {
        let raw = serde_json::json!({
            "data": "AAAA",
            "dataKey": "BBBB",
            "dataSignature": "CCCC",
            "encryptionCertificateId": "cert-7",
        });
        let parsed: EncryptedContent = serde_json::from_value(raw).unwrap();
        assert_eq!(parsed.data, "AAAA");
        assert_eq!(parsed.data_key, "BBBB");
        assert_eq!(parsed.data_signature, "CCCC");
        assert_eq!(parsed.encryption_certificate_id, "cert-7");
    }
}
