//! Extracted from voice/mod.rs as part of SOLID B.3 split.
//!
//! XChaCha20-Poly1305 AEAD encrypt / decrypt of RTP payloads for Discord's
//! `aead_xchacha20_poly1305_rtpsize` mode.
//!
//! # Nonce construction (`_rtpsize`)
//!
//! Discord's `_rtpsize` AEAD modes do **not** derive the nonce from the RTP
//! header.  The nonce is a 32-bit big-endian counter that is
//!
//! * expanded into the low 4 bytes of the cipher nonce (rest zero),
//! * appended **unencrypted** to the end of the transmitted packet, and
//! * never reused for a given session key.
//!
//! The RTP header is authenticated as AAD only.
//!
//! Deriving the nonce from the RTP header (the previous implementation) is both
//! wire-incompatible — the SFU cannot reconstruct it — and unsound: the header
//! is fully determined by (sequence, timestamp), both of which advance
//! monotonically and wrap, so the nonce repeats within a single long call.
//! The counter here is shared by every stream on one voice connection (audio
//! and video use the same session key, so they must not draw from independent
//! counters).

// Codec/DSP math: numeric conversions in AEAD helpers are intentional
#![allow(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::arithmetic_side_effects,
    clippy::default_numeric_fallback,
    clippy::map_err_ignore,
)]

use super::*;

/// Length of the unencrypted nonce suffix appended to every `_rtpsize` packet.
pub(super) const RTPSIZE_NONCE_LEN: usize = 4;

/// Returns `true` when `mode` is an AEAD mode this module can honour.
pub(super) fn is_supported_mode(mode: &str) -> bool {
    mode.contains("xchacha20")
}

/// Expand a 32-bit `_rtpsize` nonce counter into the 24-byte XChaCha20 nonce.
///
/// The counter occupies the first 4 bytes big-endian; the remaining 20 bytes
/// are zero.
pub(super) fn xchacha_nonce(counter: u32) -> XNonce {
    let mut nonce = [0u8; 24];
    nonce[..RTPSIZE_NONCE_LEN].copy_from_slice(&counter.to_be_bytes());
    XNonce::from(nonce)
}

/// Encrypt an RTP payload and return `ciphertext || nonce_counter_be32`.
///
/// The caller transmits `rtp_header || <this return value>`.  `rtp_header` is
/// authenticated as AAD but not encrypted.
pub(super) fn encrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    plaintext: &[u8],
    mode: &str,
    nonce_counter: u32,
) -> Result<Vec<u8>, VoiceError> {
    if !is_supported_mode(mode) {
        // `aead_aes256_gcm_rtpsize` is deliberately not advertised in
        // PREFERRED_AEAD_MODES, so reaching here means a caller bypassed
        // `select_encryption_mode`.
        return Err(VoiceError::NoSupportedEncryptionMode);
    }
    let nonce = xchacha_nonce(nonce_counter);
    let mut out = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad: rtp_header })
        .map_err(|_| VoiceError::AeadDecryptFailed)?;
    out.extend_from_slice(&nonce_counter.to_be_bytes());
    Ok(out)
}

/// Decrypt an RTP payload of the form `ciphertext || nonce_counter_be32`.
pub(super) fn decrypt_rtp(
    cipher: &XChaCha20Poly1305,
    rtp_header: &[u8],
    payload: &[u8],
    mode: &str,
) -> Result<Vec<u8>, VoiceError> {
    if !is_supported_mode(mode) {
        return Err(VoiceError::NoSupportedEncryptionMode);
    }
    let split = payload
        .len()
        .checked_sub(RTPSIZE_NONCE_LEN)
        .ok_or(VoiceError::AeadDecryptFailed)?;
    let (ciphertext, nonce_bytes) = payload.split_at(split);
    let counter = u32::from_be_bytes([
        *nonce_bytes.first().ok_or(VoiceError::AeadDecryptFailed)?,
        *nonce_bytes.get(1).ok_or(VoiceError::AeadDecryptFailed)?,
        *nonce_bytes.get(2).ok_or(VoiceError::AeadDecryptFailed)?,
        *nonce_bytes.get(3).ok_or(VoiceError::AeadDecryptFailed)?,
    ]);
    let nonce = xchacha_nonce(counter);
    cipher
        .decrypt(&nonce, Payload { msg: ciphertext, aad: rtp_header })
        .map_err(|_| VoiceError::AeadDecryptFailed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn cipher() -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new_from_slice(&[9u8; 32]).unwrap()
    }

    const MODE: &str = "aead_xchacha20_poly1305_rtpsize";

    #[test]
    fn round_trip_recovers_plaintext() {
        let c = cipher();
        let header = build_rtp_header(7, 960, 4242);
        let packet = encrypt_rtp(&c, &header, b"hello voice", MODE, 1).unwrap();
        let plain = decrypt_rtp(&c, &header, &packet, MODE).unwrap();
        assert_eq!(plain, b"hello voice");
    }

    #[test]
    fn nonce_counter_is_appended_unencrypted() {
        let c = cipher();
        let header = build_rtp_header(1, 0, 1);
        let packet = encrypt_rtp(&c, &header, b"x", MODE, 0x0102_0304).unwrap();
        let tail = &packet[packet.len() - RTPSIZE_NONCE_LEN..];
        assert_eq!(tail, &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn distinct_counters_produce_distinct_ciphertext() {
        // The old header-derived nonce repeated whenever (sequence, timestamp)
        // repeated.  With an explicit counter, identical headers + plaintext
        // must still encrypt differently.
        let c = cipher();
        let header = build_rtp_header(5, 100, 9);
        let a = encrypt_rtp(&c, &header, b"same", MODE, 1).unwrap();
        let b = encrypt_rtp(&c, &header, b"same", MODE, 2).unwrap();
        assert_ne!(a, b, "same header must not reuse a nonce");
    }

    #[test]
    fn tampered_header_fails_authentication() {
        let c = cipher();
        let header = build_rtp_header(3, 480, 11);
        let packet = encrypt_rtp(&c, &header, b"payload", MODE, 3).unwrap();
        let mut bad_header = header;
        bad_header[3] ^= 0xFF; // flip a sequence bit
        assert!(decrypt_rtp(&c, &bad_header, &packet, MODE).is_err());
    }

    #[test]
    fn packet_shorter_than_nonce_suffix_is_rejected() {
        let c = cipher();
        let header = build_rtp_header(1, 0, 1);
        assert!(decrypt_rtp(&c, &header, &[0u8; 3], MODE).is_err());
    }

    #[test]
    fn unsupported_mode_is_rejected_both_ways() {
        let c = cipher();
        let header = build_rtp_header(1, 0, 1);
        assert!(matches!(
            encrypt_rtp(&c, &header, b"x", "aead_aes256_gcm_rtpsize", 1),
            Err(VoiceError::NoSupportedEncryptionMode)
        ));
        assert!(matches!(
            decrypt_rtp(&c, &header, &[0u8; 32], "aead_aes256_gcm_rtpsize"),
            Err(VoiceError::NoSupportedEncryptionMode)
        ));
    }
}
