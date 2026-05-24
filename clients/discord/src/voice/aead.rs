//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! XChaCha20-Poly1305 AEAD encrypt / decrypt of RTP payloads. Pure structural move.

use super::*;

pub(super) fn encrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    plaintext: &[u8],
    mode: &str,
) -> Result<Vec<u8>, VoiceError> {
    if mode == "aead_xchacha20_poly1305_rtpsize" || mode.contains("xchacha20") {
        let nonce = rtp_header_to_xchacha_nonce(rtp_header);
        cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad: rtp_header })
            .map_err(|_| VoiceError::AeadDecryptFailed)
    } else {
        // aead_aes256_gcm_rtpsize — same nonce construction, different cipher.
        // For now, fall through to xchacha20 (mode selection ensures we only
        // get here if server supports xchacha20 first; aes256 gcm support
        // is a future extension).
        Err(VoiceError::NoSupportedEncryptionMode)
    }
}

/// Decrypt an RTP payload.
pub(super) fn decrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    ciphertext: &[u8],
    mode: &str,
) -> Result<Vec<u8>, VoiceError> {
    if mode.contains("xchacha20") {
        let nonce = rtp_header_to_xchacha_nonce(rtp_header);
        cipher
            .decrypt(&nonce, Payload { msg: ciphertext, aad: rtp_header })
            .map_err(|_| VoiceError::AeadDecryptFailed)
    } else {
        Err(VoiceError::NoSupportedEncryptionMode)
    }
}

/// Build a 24-byte XChaCha20 nonce from a 12-byte RTP header.
/// The RTP header is placed at bytes 0..12; bytes 12..24 are zero.
pub(super) fn rtp_header_to_xchacha_nonce(rtp_header: &[u8]) -> XNonce {
    let mut nonce = [0u8; 24];
    let len = rtp_header.len().min(24);
    nonce[..len].copy_from_slice(&rtp_header[..len]);
    XNonce::from(nonce)
}
